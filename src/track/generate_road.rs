//! 真实道路路由轨迹生成器（模式 B）。
//!
//! 与经典模式（`generator::build`）的分工：
//! - 几何源为路网闭环路线（而非 Catmull-Rom 环）；
//! - 配速为运动学速度剖面：OU 均值回归 + 弯道预判刹车（`kinematics::pace_profile`）；
//! - 步频/步幅为生物力学 OU 耦合（`biomech::ou_cadence_series` + `gait`）；
//! - GPS 抖动为 AR(1) 相关漂移 + 高斯测量噪声 + 建筑 SDF 衰减（`noise::GpsJitter`）。
//!
//! 协议字段/哨兵/断崖/吸附/10s 窗与经典模式完全一致。

use super::generator::{SPEED_CEIL, SPEED_FLOOR};
use super::geom::{
    fmt_gain_time, ring_point_at, round_to, to_bd, wgs84_to_bd09, Rng, MET_PER_DEG_LAT,
    MET_PER_DEG_LNG,
};
use super::model::{GenPoint, Track};
use super::postfix::apply_post_fixes;
use route_planner::{
    fatigue_from_km, gait, ou_cadence_series, pace_profile, plan_route_split, point_in_polygon,
    Coord, GpsJitter, KinParams, RoadGraph, Route, RouteOptions, Sdf,
};

/// 路线算法模式（二选一）。
#[derive(Clone, Copy, PartialEq, Eq, Debug, Default)]
pub enum RouteMode {
    /// 经典打卡点环（Catmull-Rom）。
    #[default]
    Legacy,
    /// 真实道路拓扑路由。
    Road,
}

impl RouteMode {
    pub fn from_str(s: &str) -> Self {
        match s {
            "road" => RouteMode::Road,
            _ => RouteMode::Legacy,
        }
    }
}

/// 从本地文件加载 OSM 路网。
pub fn load_network_path(path: &str) -> Result<RoadGraph, String> {
    if path.trim().is_empty() {
        return Err("未配置 OSM 路网，请先导入路网或主动切换经典模式".into());
    }
    let bytes = std::fs::read(path).map_err(|e| format!("读取 {path} 失败: {e}"))?;
    route_planner::load_osm(&bytes)
}

/// 路网预览计划（供地图显示）：对齐后的路网/建筑 + 生成的路线（BD 系）。
pub struct RoadPlan {
    pub route: Vec<(f64, f64)>,
    pub length_m: f64,
    pub edges: Vec<Vec<(f64, f64)>>,
    pub buildings: Vec<Vec<(f64, f64)>>,
    pub checkpoints: Vec<(f64, f64)>,
    pub fences: Vec<Vec<(f64, f64)>>,
}

/// 用围栏（BD 系，与打卡点同系）裁剪路网：仅保留围栏内道路；空路网交由路由器明确报错。
pub fn apply_fences(net: &RoadGraph, fences: &[Vec<(f64, f64)>]) -> RoadGraph {
    if fences.is_empty() {
        return net.clone();
    }
    let mut g = net.clone();
    let polys: Vec<Vec<Coord>> = fences
        .iter()
        .map(|r| r.iter().map(|p| Coord::new(p.1, p.0)).collect())
        .collect();
    g.retain_inside_any(&polys);
    // 进一步裁剪：移除「穿越围栏边界」的边（端点在内但中段跑出围栏，如跨校区道路）。
    g.graph.retain_edges(|graph, e| {
        let (a, b) = graph.edge_endpoints(e).unwrap();
        edge_inside(&polys, graph[a].coord, graph[b].coord)
    });
    g.rebuild_index();
    g
}

/// 边是否整体落在任一围栏内：沿边等距采样，所有采样点（含端点）必须在围栏内。
fn edge_inside(polys: &[Vec<Coord>], a: Coord, b: Coord) -> bool {
    const SEGS: usize = 8;
    for k in 0..=SEGS {
        let t = k as f64 / SEGS as f64;
        let p = Coord::new(a.lon + (b.lon - a.lon) * t, a.lat + (b.lat - a.lat) * t);
        if !polys.iter().any(|poly| point_in_polygon(poly, p)) {
            return false;
        }
    }
    true
}

/// 生成路网预览（对齐 + 围栏裁剪 + 闭环路由），不产生协议字段。
///
/// 调用方提供合并后的 policy/实时点位，预览与生成使用同一个路由函数。
pub fn plan_road_view(
    net: &RoadGraph,
    points_bd: &[(f64, f64)],
    dist: f64,
    dur: i64,
    seed: u64,
    fences: &[Vec<(f64, f64)>],
) -> Result<RoadPlan, String> {
    let mut g = net.clone();
    align_network(&mut g);
    g = apply_fences(&g, fences);
    let ordered = points_bd.to_vec();
    let route = required_route(&g, &ordered, dist, dur, seed)?;
    let route_pts: Vec<(f64, f64)> = route.points.iter().map(|p| (p.lat, p.lon)).collect();
    let edges: Vec<Vec<(f64, f64)>> = g
        .edges_deg()
        .iter()
        .map(|e| e.iter().map(|c| (c.lat, c.lon)).collect())
        .collect();
    let buildings: Vec<Vec<(f64, f64)>> = g
        .buildings
        .iter()
        .map(|r| r.iter().map(|c| (c.lat, c.lon)).collect())
        .collect();
    Ok(RoadPlan {
        route: route_pts,
        length_m: route.length_m,
        edges,
        buildings,
        checkpoints: ordered,
        fences: fences.to_vec(),
    })
}

/// 将 WGS84 路网逐节点转换为 BD-09（与打卡点同系）。
///
/// OSM 为 WGS84，打卡点为 BD-09。两者不是常数平移关系（境内 GCJ/BD 偏移随位置
/// 变化），故用标准 WGS84→GCJ-02→BD-09 变换逐节点转换，校园尺度下与真实道路
/// 对齐（优于旧版质心常数平移的米级~数十米残差）。
pub fn align_network(net: &mut RoadGraph) {
    if net.graph.node_count() == 0 {
        return;
    }
    for node in net.graph.node_weights_mut() {
        let (lat, lon) = wgs84_to_bd09(node.coord.lat, node.coord.lon);
        node.coord.lat = lat;
        node.coord.lon = lon;
    }
    for ring in net.buildings.iter_mut() {
        for c in ring.iter_mut() {
            let (lat, lon) = wgs84_to_bd09(c.lat, c.lon);
            c.lat = lat;
            c.lon = lon;
        }
    }
    net.compute_anchor();
    net.rebuild_index();
}

/// 按质心角度排序打卡点，形成自然环序。
pub fn radial_order(points_bd: &[(f64, f64)]) -> Vec<(f64, f64)> {
    let n = points_bd.len();
    if n == 0 {
        return Vec::new();
    }
    let (cx, cy) = (
        points_bd.iter().map(|q| q.0).sum::<f64>() / n as f64,
        points_bd.iter().map(|q| q.1).sum::<f64>() / n as f64,
    );
    let mut ordered = points_bd.to_vec();
    ordered.sort_by(|a, b| {
        (a.0 - cx)
            .atan2(a.1 - cy)
            .partial_cmp(&(b.0 - cx).atan2(b.1 - cy))
            .unwrap()
    });
    ordered
}

/// 路线 → 平面折线 + 弧长表。
///
/// `arcs[k]` 必须与 `dense[k]` 一一对应（`ring_point_at` 按同下标取弧长），
/// 因此直接取每个采样点的累计弧长 `p.s`（`dense` 末点已闭合回起点）。
fn route_ring(route: &Route, c_lat: f64, c_lng: f64) -> (Vec<(f64, f64)>, Vec<f64>) {
    let dense: Vec<(f64, f64)> = route
        .points
        .iter()
        .map(|p| {
            (
                (p.lon - c_lng) * MET_PER_DEG_LNG,
                (p.lat - c_lat) * MET_PER_DEG_LAT,
            )
        })
        .collect();
    let arcs: Vec<f64> = route.points.iter().map(|p| p.s).collect();
    (dense, arcs)
}

pub fn effective_duration(dist: f64, dur: i64) -> i64 {
    let speed = (dist / dur.max(1) as f64).clamp(SPEED_FLOOR + 0.1, SPEED_CEIL - 0.1);
    (dist / speed).round().max(1.0) as i64
}

fn required_route(
    net: &RoadGraph,
    points: &[(f64, f64)],
    dist: f64,
    dur: i64,
    seed: u64,
) -> Result<Route, String> {
    if !dist.is_finite() || dist <= 0.0 || dur <= 0 {
        return Err("距离和时长必须为正数".into());
    }
    let avg = dist / effective_duration(dist, dur) as f64;
    let opts = RouteOptions {
        a_c_max: 2.5,
        min_radius_m: (avg * avg / 2.5).max(6.0),
        ..Default::default()
    };
    let waypoints: Vec<Coord> = points.iter().map(|p| Coord::new(p.1, p.0)).collect();
    let route = plan_route_split(net, &waypoints, &waypoints, dist, seed, &opts)?;
    for (i, &(lat, lon)) in points.iter().enumerate() {
        let nearest = route
            .points
            .iter()
            .map(|p| {
                let dy = (p.lat - lat) * MET_PER_DEG_LAT;
                let dx = (p.lon - lon) * 111_320.0 * lat.to_radians().cos();
                dx.hypot(dy)
            })
            .fold(f64::INFINITY, f64::min);
        if nearest > 40.0 {
            return Err(format!(
                "路网无法到达第 {} 个点位（最近 {:.1} 米，超过 40 米）",
                i + 1,
                nearest
            ));
        }
    }
    Ok(route)
}

/// 模式 B：真实道路拓扑路由轨迹生成。
///
/// `route_bd[0]` 为起点；`route_bd` 和 `must_bd` 合并为强制必经点。
/// 不使用离路吸附；平滑路线和最终采样均须经过全部点位的 40 米范围。
pub fn build_road(
    dist: f64,
    dur: i64,
    seed: u64,
    start_ms: i64,
    route_bd: &[(f64, f64)],
    must_bd: &[(f64, f64)],
    net: &RoadGraph,
) -> Result<Track, String> {
    if dur <= 0 || !dist.is_finite() || dist <= 0.0 {
        return Err("距离和时长必须为正数".into());
    }
    let mut rng = Rng::new(seed);
    let dur_f = dur as f64;
    let avg_v = dist / dur_f;
    let a_c_max = 2.5;
    let mut required = route_bd.to_vec();
    for p in must_bd {
        if !required.contains(p) {
            required.push(*p);
        }
    }
    let route = required_route(net, &required, dist, dur, seed)?;

    let n_pts = route_bd.len().max(1);
    let (c_lat, c_lng) = (
        route_bd.iter().map(|q| q.0).sum::<f64>() / n_pts as f64,
        route_bd.iter().map(|q| q.1).sum::<f64>() / n_pts as f64,
    );
    let (dense, arcs) = route_ring(&route, c_lat, c_lng);

    // 时间网格（同经典模式）
    let mut times = Vec::new();
    let mut t = 0.0;
    while t < dur_f {
        times.push(t);
        t += if rng.random() < 0.80 {
            5.0
        } else {
            rng.choice(&[1.0, 2.0, 3.0, 4.0, 6.0, 7.0, 8.0])
        };
    }
    let n = times.len();
    let mut dts: Vec<f64> = (0..n - 1).map(|i| times[i + 1] - times[i]).collect();
    dts.push(1.0f64.max(dur_f - times[n - 1]));

    // 运动学速度剖面：OU 配速 + 弯道预判刹车 + 精确命中目标距离
    let kin = KinParams {
        a_c_max,
        ..Default::default()
    };
    let speed_seed = seed.wrapping_add(0x9E37_79B9_7F4A_7C15);
    let w = pace_profile(
        &route,
        avg_v,
        &dts,
        dist,
        speed_seed,
        &kin,
        SPEED_FLOOR,
        SPEED_CEIL,
    );
    let speeds: Vec<f64> = w.clone();
    let seg_dist: Vec<f64> = (0..n).map(|i| w[i] * dts[i]).collect();

    // 点位类型（同经典模式）
    let tl_w = [
        ((-1i64, 4i64), 54u32),
        ((-1, 1), 27),
        ((-1, 12), 13),
        ((-1, 5), 3),
        ((-1, 6), 2),
    ];
    let mut kinds: Vec<(i64, i64)> = Vec::with_capacity(n);
    for _ in 0..n {
        let u = rng.random();
        if u < 0.39 {
            kinds.push((3, 1));
        } else if u < 0.93 {
            kinds.push((0, 1));
        } else if u < 0.96 {
            kinds.push(rng.weighted(&tl_w));
        } else {
            kinds.push((rng.choice(&[1, 1, 1, 1, 2, 2]), 1));
        }
    }
    for i in 1..kinds.len() {
        let prev_drift = (-1 == kinds[i - 1].0) || kinds[i - 1].0 == 5 || kinds[i - 1].0 == 6;
        if kinds[i].0 != -1 && prev_drift && rng.random() < 0.08 {
            kinds[i] = (if rng.random() < 0.75 { 7 } else { 8 }, 1);
        }
    }
    for i in 1..n {
        if kinds[i].0 == -1 && kinds[i - 1].0 == -1 {
            kinds[i] = (rng.choice(&[3, 0]), 1);
        }
    }
    let normal_idx: Vec<usize> = (0..n).filter(|&i| kinds[i].0 != -1 && i > 1).collect();
    let share: f64 = normal_idx.iter().map(|&i| seg_dist[i]).sum();
    let share = if share == 0.0 { 1.0 } else { share };
    let mut disp_of = vec![0.0f64; n];
    for &i in &normal_idx {
        disp_of[i] = seg_dist[i] * (dist / share);
    }

    // 建筑 SDF（自适应放大漂移）
    let sdf = Sdf::from_buildings(
        &net.buildings,
        |c| {
            [
                (c.lon - c_lng) * MET_PER_DEG_LNG,
                (c.lat - c_lat) * MET_PER_DEG_LAT,
            ]
        },
        2.0,
        30.0,
    );

    // 生物力学：步频在目标值附近 OU 波动，步幅反解严格满足 v=(cad/60)·stride
    let targets: Vec<f64> = (0..n)
        .map(|i| {
            let km_done = (times[i] / dur_f) * dist / 1000.0;
            gait(speeds[i], fatigue_from_km(km_done)).cadence
        })
        .collect();
    let cadence_seed = seed.wrapping_add(0xC6A4_A793_5BD1_E995);
    let cadences = ou_cadence_series(&targets, &dts, cadence_seed, 0.25, 0.5, 110.0, 220.0);

    // GPS 抖动：AR(1) 相关漂移 + 高斯测量噪声（SDF 逐点放大）
    let mut jitter = GpsJitter::new(0.72, 0.75, 0.3);

    let mut locs: Vec<GenPoint> = Vec::with_capacity(n);
    let mut s = 0.0f64;
    let mut t_acc = 0.0f64;
    let mut dist_acc = 0.0f64;
    let mut steps_acc = 0.0f64;
    let mut alt = 82.0 + rng.uniform(-1.0, 1.0);
    let n_est = 1.max((dur_f / 5.0) as i64);
    let alt_sigma = rng.uniform(3.8, 6.2) / (0.40 * n_est as f64);
    for i in 0..n {
        let dt = dts[i];
        let (typ, lt) = kinds[i];
        t_acc += dt;
        let pos = |ss: f64| ring_point_at(&dense, &arcs, ss);
        let mut d_step = 0.0f64;
        let px;
        let py;
        let x;
        let y;
        let rad;
        let state;
        if typ != -1 {
            d_step = disp_of[i];
            s += d_step;
            let (bx, by) = pos(s);
            x = bx;
            y = by;
            // AR(1) 相关漂移 + 高斯测量噪声，SDF 自适应放大
            let sc = sdf.sigma_scale([bx, by]);
            let z = [
                rng.gauss(0.0, 1.0),
                rng.gauss(0.0, 1.0),
                rng.gauss(0.0, 1.0),
                rng.gauss(0.0, 1.0),
            ];
            let (jxd, jyd) = jitter.step(sc, z);
            px = bx + jxd;
            py = by + jyd;
            rad = round_to(
                if typ == 3 {
                    rng.uniform(1.4, 5.1)
                } else {
                    rng.uniform(1.4, 2.4)
                },
                2,
            );
            state = if typ == 0 {
                rng.weighted(&[(1, 145), (2, 45), (3, 164)])
            } else {
                rng.weighted(&[(1, 145), (2, 256), (3, 151)])
            };
        } else {
            if rng.random() >= 0.20 {
                d_step = rng.uniform(0.0, super::generator::MAX_EXCEPTION_DRIFT_M);
            }
            let (bx, by) = pos(s);
            x = bx;
            y = by;
            if d_step > 0.0 {
                let ang = rng.uniform(0.0, std::f64::consts::TAU);
                px = x + d_step * ang.sin();
                py = y + d_step * ang.cos();
            } else if let Some(last) = locs.last() {
                py = (last.gLat - c_lat) * MET_PER_DEG_LAT;
                px = (last.gLng - c_lng) * MET_PER_DEG_LNG;
            } else {
                let (jx, jy) = jitter.state();
                px = x + jx;
                py = y + jy;
            }
            rad = round_to(rng.uniform(1.6, 10.0), 2);
            state = rng.weighted(&[(1, 102), (2, 124), (3, 136)]);
        }
        if typ != -1 {
            dist_acc += d_step;
        }
        let (lat, lng) = to_bd(px, py, c_lat, c_lng);
        alt += 0.04 * (82.0 - alt) + rng.gauss(0.0, alt_sigma);
        // 生物力学联动：v=(cad/60)·stride
        let v_now = speeds[i];
        let mut stride = v_now * 60.0 / cadences[i];
        if stride > 1.7 {
            stride = 1.7;
        } else if stride < 0.5 {
            stride = 0.5;
        }
        let cad = v_now * 60.0 / stride;
        steps_acc += cad / 60.0 * dt;
        let nxt = pos(s + 2.0);
        let brg =
            ((nxt.0 - x).atan2(nxt.1 - y).to_degrees() + rng.gauss(0.0, 35.0)).rem_euclid(360.0);
        let (avg_sp, gps_speed) = if typ == -1 {
            let avg = round_to(dist_acc / t_acc.max(1.0), 4);
            let gps = rng.uniform(0.5, super::generator::MAX_EXCEPTION_GPS_SPEED_KMH);
            (avg, round_to(gps, 4))
        } else {
            let avg = round_to(d_step / dt, 4);
            let kmh = avg * 3.6;
            let sigma = (kmh * 0.08).max(0.05);
            let gps = if rng.random() < 0.20 {
                0.0
            } else {
                round_to((kmh + rng.gauss(0.0, sigma)).max(0.0), 4)
            };
            (avg, gps)
        };

        locs.push(GenPoint {
            id: i as i64 + 1,
            flag: start_ms,
            lat: -1.0,
            lng: -1.0,
            gLat: round_to(lat, 7),
            gLng: round_to(lng, 7),
            speed: round_to(gps_speed, 4),
            avgSpeed: avg_sp,
            radius: rad,
            accuracy: rad,
            ptype: typ,
            locType: lt,
            hasAltitude: true,
            totalTime: round_to(t_acc, 0) as i64,
            totalDis: round_to(dist_acc, 4),
            validDis: round_to(dist_acc, 4),
            validTime: round_to(t_acc, 0) as i64,
            steps: steps_acc as i64,
            stepDistance: 0.0,
            gainTime: fmt_gain_time(start_ms + (t_acc * 1000.0) as i64),
            gainTimeMs: start_ms + (t_acc * 1000.0) as i64,
            queueNum: 0,
            coorType: "gcj02".into(),
            bdA: round_to(alt, 2),
            bdD: round_to(brg, 2),
            bdS: round_to((avg_sp * rng.uniform(0.6, 0.95)).max(0.0), 3),
            bdG: rng.choice(&[1, 1, 1, -1]),
            count: rng.randint(20, 88),
            dtr: 0.0,
            state,
            locationId: String::new(),
        });
    }
    apply_post_fixes(&mut locs, &mut rng, start_ms);

    let total_dis = round_to(dist, 3);
    let mut track = Track {
        totalTime: round_to(t_acc, 0) as i64,
        totalDistance: total_dis,
        validDistance: total_dis,
        validTime: round_to(t_acc, 0) as i64,
        startTime: start_ms,
        startLatitude: locs[0].gLat,
        startLongitude: locs[0].gLng,
        totalSteps: steps_acc as i64,
        locations: locs,
        speedPerTenSec: Vec::new(),
        stepsPerTenSec: Vec::new(),
        segments: Vec::new(),
    };
    super::stats::rebuild(&mut track);
    track.validate_consistency()?;
    for point in route_bd.iter().chain(must_bd.iter()) {
        if !super::wire::passes_point(&track, *point) {
            return Err("最终道路轨迹未经过全部点位，请调整距离或路网".into());
        }
    }
    Ok(track)
}

#[cfg(test)]
mod preview_tests {
    use super::*;
    #[test]
    fn preview_and_generation_share_required_points_options_and_seed() {
        let mut net = RoadGraph::new();
        let wgs = [
            (39.9, 116.4),
            (39.9, 116.402),
            (39.902, 116.402),
            (39.902, 116.4),
        ];
        let nodes: Vec<_> = wgs
            .iter()
            .map(|&(lat, lon)| net.add_node(Coord::new(lon, lat)))
            .collect();
        for i in 0..4 {
            let edge = route_planner::graph::EdgeData {
                way_id: i as u64,
                oneway: false,
                highway: "footway".into(),
                maxspeed_kmh: None,
                length_m: 200.0,
            };
            net.add_edge(nodes[i], nodes[(i + 1) % 4], edge.clone());
            net.add_edge(nodes[(i + 1) % 4], nodes[i], edge);
        }
        net.compute_anchor();
        net.rebuild_index();
        let points: Vec<_> = wgs.iter().map(|&(a, b)| wgs84_to_bd09(a, b)).collect();
        let preview = plan_road_view(&net, &points, 2000.0, 800, 7, &[]).unwrap();
        let mut aligned = net.clone();
        align_network(&mut aligned);
        let route = required_route(&aligned, &points, 2000.0, 800, 7).unwrap();
        assert_eq!(preview.checkpoints, points);
        assert_eq!(
            preview.route,
            route
                .points
                .iter()
                .map(|p| (p.lat, p.lon))
                .collect::<Vec<_>>()
        );
        let track = build_road(2000.0, 800, 7, 0, &points, &points, &aligned).unwrap();
        assert!(points
            .iter()
            .all(|p| super::super::wire::passes_point(&track, *p)));
        for p in &track.locations {
            let nearest = preview
                .route
                .iter()
                .map(|&(lat, lon)| {
                    ((p.gLat - lat) * MET_PER_DEG_LAT).hypot((p.gLng - lon) * MET_PER_DEG_LNG)
                })
                .fold(f64::INFINITY, f64::min);
            assert!(nearest < 40.0, "preview/generation mismatch: {nearest}");
        }
    }
}
