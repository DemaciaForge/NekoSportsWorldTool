//! 自然轨迹生成器。
//!
//! 画像驱动：打卡点沿线段组成闭合环（每段 18 采样）→ 弧长表；
//! 采样间隔主 5s（80%）；速度曲线 = ramp × 疲劳 × 三正弦 × 余弦凹陷 × 微噪；
//! 正常点按速度曲线分配位移并归一到精确总距离；所有采样点保持在打卡点闭环线上；
//! 哨兵点（首 type=0、索引1 type=5、末 type=6）；结尾断崖；点位吸附。
#![allow(non_snake_case)]

use super::geom::{
    clamp_inside_fence, fmt_gain_time, make_point_ring, make_point_ring_with_fence,
    parse_fence_plane, ring_point_at, round_to, to_bd, Rng, MET_PER_DEG_LAT, MET_PER_DEG_LNG,
};
use super::model::{GenPoint, Segment, Track};
use super::postfix::apply_post_fixes;
use super::stadium::{make_stadium_ring, TrackSpec};
use crate::api::model::TrackColorMode;

/// 有效配速窗口（判定规则 2'21"-10'00"/km ≈ 1.667-7.092 m/s），硬边界留余量。
pub const SPEED_FLOOR: f64 = 1.90;
pub const SPEED_CEIL: f64 = 6.30;

/// 等比缩放逐点速度至目标总距：越界点钳在窗口边界，剩余差量由未饱和点分摊（迭代收敛）。
/// 与整体等比缩放的区别：任何一点的瞬时配速都不会越出有效窗口。
fn fit_speeds(w: &mut [f64], dts: &[f64], target: f64) {
    for _ in 0..24 {
        let cur: f64 = w.iter().zip(dts).map(|(x, dt)| x * dt).sum();
        if (cur - target).abs() <= 1.0 {
            break;
        }
        let k = target / cur;
        for x in w.iter_mut() {
            *x = (*x * k).clamp(SPEED_FLOOR, SPEED_CEIL);
        }
    }
    for x in w.iter_mut() {
        *x = x.clamp(SPEED_FLOOR, SPEED_CEIL);
    }
}

/// 轨迹生成主入口。points_bd 为 BD 系打卡点。
pub fn build(
    dist: f64,
    dur: i64,
    seed: u64,
    _center: (f64, f64),
    start_ms: i64,
    points_bd: &[(f64, f64)],
) -> Track {
    build_with_fence(
        dist,
        dur,
        seed,
        _center,
        start_ms,
        points_bd,
        None,
        TrackColorMode::FullGreen,
    )
}

/// Build a route with the server-provided campus fence. `track_color_mode`
/// is intentionally kept explicit: official detail-page color classification
/// is server-side, so no undocumented type/state values are invented here.
pub fn build_with_fence(
    dist: f64,
    dur: i64,
    seed: u64,
    _center: (f64, f64),
    start_ms: i64,
    points_bd: &[(f64, f64)],
    fence_json: Option<&str>,
    track_color_mode: TrackColorMode,
) -> Track {
    build_with_fence_and_spec(
        dist,
        dur,
        seed,
        _center,
        start_ms,
        points_bd,
        fence_json,
        track_color_mode,
        TrackSpec::Auto,
    )
}

/// Build a route using either the server fence or a standard school track
/// preset. The old entry point remains available for tests and integrations.
pub fn build_with_fence_and_spec(
    dist: f64,
    dur: i64,
    seed: u64,
    _center: (f64, f64),
    start_ms: i64,
    points_bd: &[(f64, f64)],
    fence_json: Option<&str>,
    track_color_mode: TrackColorMode,
    track_spec: TrackSpec,
) -> Track {
    let mut rng = Rng::new(seed);
    let dur_f = dur as f64;
    let (dense, arcs, pc) = if !matches!(track_spec, TrackSpec::Auto) {
        make_stadium_ring(points_bd, fence_json, track_spec).unwrap_or_else(|error| {
            panic!("显式跑道规格拟合失败：{error}");
        })
    } else if fence_json.is_some() {
        make_point_ring_with_fence(points_bd, fence_json)
    } else {
        make_point_ring(points_bd)
    };
    let _ = track_color_mode;
    let (c_lat, c_lng) = pc;
    let fence_plane = fence_json.and_then(|text| parse_fence_plane(Some(text), c_lat, c_lng));
    // The route ring has been ordered geometrically around the track; the
    // source point array remains unchanged for the server's fixed-point data.
    let direction = 1.0;
    let s0 = 0.0;
    let phase_v = rng.uniform(0.0, std::f64::consts::TAU);
    let phase_track = rng.uniform(0.0, std::f64::consts::TAU);

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

    let n_dips = rng.choice(&[1, 1, 2]);
    let mut dips = Vec::new();
    for _ in 0..n_dips {
        dips.push((
            rng.uniform(0.15, 0.75) * dur_f,
            rng.uniform(25.0, 55.0),
            rng.uniform(0.08, 0.16),
        ));
    }
    let dip_factor = |t: f64| -> f64 {
        let mut f = 1.0;
        for &(c, hw, d) in &dips {
            if (t - c).abs() < hw {
                f *= 1.0 - d * 0.5 * (1.0 + std::f64::consts::PI * (t - c) / hw).cos();
            }
        }
        f
    };
    let base = dist / dur_f;
    let mut w = Vec::with_capacity(n);
    for &tt in &times {
        let ramp = if tt < 8.0 {
            (0.85 + 0.15 * (tt / 1.0f64.max(8.0f64.min(dur_f / 20.0)))).min(1.0)
        } else {
            1.0
        } * if tt > dur_f - 8.0 {
            1.0 + 0.03 * (tt - (dur_f - 8.0)) / 8.0
        } else {
            1.0
        };
        let km_done = (tt / dur_f) * dist / 1000.0;
        let fatigue = if km_done <= 0.5 {
            1.03
        } else {
            (1.03 - 0.06 * (km_done - 0.5)).max(0.80)
        };
        let wave = (1.0
            + 0.010 * (std::f64::consts::TAU * tt / 115.0 + phase_v).sin()
            + 0.035 * (std::f64::consts::TAU * tt / 47.0 + phase_v * 2.3).sin()
            + 0.015 * (std::f64::consts::TAU * tt / 19.0 + phase_v * 3.7).sin())
            * dip_factor(tt);
        let noise = 1.0 + rng.gauss(0.0, 0.008);
        w.push(base * ramp * fatigue * wave * noise);
    }
    let mut dts: Vec<f64> = (0..n - 1).map(|i| times[i + 1] - times[i]).collect();
    dts.push(1.0f64.max(dur_f - times[n - 1]));
    // 逐点速度全部约束在有效配速窗口内，并精确命中目标距离
    fit_speeds(&mut w, &dts, dist);
    let seg_dist: Vec<f64> = (0..n).map(|i| w[i] * dts[i]).collect();
    let speeds: Vec<f64> = w.clone();

    // Keep campus route samples on one ordinary point protocol. The start and
    // end sentinels are assigned explicitly by apply_post_fixes below.
    let kinds: Vec<(i64, i64)> = vec![(0, 1); n];
    let normal_idx: Vec<usize> = (0..n).filter(|&i| kinds[i].0 != -1).collect();
    let share: f64 = normal_idx.iter().map(|&i| seg_dist[i]).sum();
    let share = if share == 0.0 { 1.0 } else { share };
    let mut disp_of = vec![0.0f64; n];
    for &i in &normal_idx {
        disp_of[i] = seg_dist[i] * (dist / share);
    }

    let mut locs: Vec<GenPoint> = Vec::with_capacity(n);
    let mut s = s0;
    let mut t_acc = 0.0f64;
    let mut dist_acc = 0.0f64;
    // Steps are derived from the same cumulative distance that drives the
    // route.  Keeping one target total prevents cadence windows, segments,
    // laps and point fields from drifting apart when sample intervals vary.
    let nominal_stride = (0.62 + 0.17 * (dist / dur_f)).clamp(0.72, 1.25);
    let target_total_steps = (dist / nominal_stride).round().max(1.0) as i64;
    let mut steps_acc = 0i64;
    let mut interval_steps = Vec::with_capacity(n);
    // Cadence is a smooth human-scale signal rather than a direct rounded
    // distance/stride quotient at every GPS sample.  Build one cumulative
    // step target from two low-frequency waves, then reuse it for point
    // steps, ten-second windows, segments and laps.
    let cadence_phase = rng.uniform(0.0, std::f64::consts::TAU);
    let mut cadence_mass = Vec::with_capacity(n);
    let mut cadence_total = 0.0f64;
    for i in 0..n {
        let midpoint = times[i] + dts[i] * 0.5;
        let profile = (1.0
            + 0.045 * (std::f64::consts::TAU * midpoint / 180.0 + cadence_phase).sin()
            + 0.018 * (std::f64::consts::TAU * midpoint / 72.0 + cadence_phase * 0.63).sin())
        .max(0.90);
        cadence_total += profile * dts[i];
        cadence_mass.push(cadence_total);
    }
    // Real records have a broad, low-frequency change with short correlated
    // GPS/elevation fluctuations. A single sinusoid makes the detail chart
    // look artificial (one clean hill); precompute an AR(1)-like perturbation
    // so the result stays continuous while still resembling sampled terrain.
    let altitude_base = 11.5 + rng.uniform(-1.5, 1.5);
    let altitude_macro = (2.0 + (dist / 4000.0).clamp(0.25, 0.8)).clamp(2.0, 2.8);
    let altitude_phase = rng.uniform(0.0, std::f64::consts::TAU);
    let mut altitude_state = 0.0f64;
    let mut altitude_samples = Vec::with_capacity(n);
    for i in 0..n {
        let progress = (times[i] / dur_f.max(1.0)).clamp(0.0, 1.0);
        altitude_state = altitude_state * 0.84 + rng.gauss(0.0, 0.34);
        let broad = altitude_macro
            * (std::f64::consts::TAU * (1.15 * progress) + altitude_phase).sin()
            + altitude_macro
                * 0.36
                * (std::f64::consts::TAU * (2.65 * progress) + altitude_phase * 0.63).sin();
        let local = altitude_state + rng.gauss(0.0, 0.06);
        altitude_samples.push(altitude_base + broad + local);
    }

    for i in 0..n {
        let dt = dts[i];
        let (typ, lt) = kinds[i];
        t_acc += dt;
        let base_pos = |ss: f64| ring_point_at(&dense, &arcs, ss);
        let pos = |ss: f64| {
            let (bx, by) = base_pos(ss);
            let (px, py) = base_pos(ss - 2.0);
            let (nx, ny) = base_pos(ss + 2.0);
            let tx = nx - px;
            let ty = ny - py;
            let norm = (tx * tx + ty * ty).sqrt().max(1e-6);
            let amp = 0.5
                + 0.5
                    * (std::f64::consts::TAU * ss / 180.0 + phase_track)
                        .sin()
                        .abs();
            let wobble = amp * (std::f64::consts::TAU * ss / 95.0 + phase_track * 1.7).sin();
            let candidate = (bx - ty / norm * wobble, by + tx / norm * wobble);
            clamp_inside_fence(candidate, (bx, by), fence_plane.as_deref())
        };
        let mut d_step = 0.0f64;
        let px;
        let py;
        let x;
        let y;
        let rad;
        let state;
        if typ != -1 {
            d_step = disp_of[i];
            s += direction * d_step;
            let (bx, by) = pos(s);
            x = bx;
            y = by;
            // Keep route positions on the sampled checkpoint ring.
            px = bx;
            py = by;
            rad = round_to(rng.uniform(1.4, 2.4), 2);
            state = 1;
        } else {
            match lt {
                4 => {
                    if rng.random() >= 0.68 {
                        d_step = if rng.random() < 0.95 {
                            rng.uniform(2.0, 60.0)
                        } else {
                            rng.uniform(60.0, 250.0)
                        };
                    }
                }
                1 => {
                    if rng.random() >= 0.83 {
                        d_step = rng.uniform(0.5, 36.0);
                    }
                }
                12 => {
                    d_step = if rng.random() < 0.9 {
                        rng.uniform(5.0, 80.0)
                    } else {
                        rng.uniform(80.0, 220.0)
                    };
                }
                5 => d_step = rng.uniform(5.0, 60.0),
                _ => d_step = rng.uniform(100.0, 300.0),
            }
            let (bx, by) = pos(s);
            x = bx;
            y = by;
            if d_step > 0.0 {
                let ang = rng.uniform(0.0, std::f64::consts::TAU);
                px = x + d_step * ang.sin();
                py = y + d_step * ang.cos();
            } else if let Some(last) = locs.last() {
                // 零位移：精确复制上一点坐标（BD→平面）
                py = (last.gLat - c_lat) * MET_PER_DEG_LAT;
                px = (last.gLng - c_lng) * MET_PER_DEG_LNG;
            } else {
                px = x;
                py = y;
            }
            rad = if lt == 4 {
                round_to(
                    if rng.random() < 0.75 {
                        rng.uniform(30.0, 100.0)
                    } else {
                        rng.uniform(100.0, 550.0)
                    },
                    2,
                )
            } else if lt == 1 {
                round_to(
                    if rng.random() < 0.75 {
                        rng.uniform(1.6, 12.0)
                    } else {
                        rng.uniform(12.0, 95.0)
                    },
                    2,
                )
            } else if lt == 12 {
                round_to(rng.uniform(30.0, 125.0), 2)
            } else if lt == 5 {
                round_to(rng.uniform(25.0, 300.0), 2)
            } else {
                550.0
            };
            state = rng.weighted(&[(1, 102), (2, 124), (3, 136)]);
        }
        if typ != -1 {
            dist_acc += d_step; // 轨迹点位移计入累计距离
        }
        let (lat, lng) = to_bd(px, py, c_lat, c_lng);
        let alt = altitude_samples[i];
        // Use the distance-driven cumulative target for conservation, but
        // smooth the displayed stride independently.  Five-second samples
        // otherwise round to alternating 11/12 steps and produce a visibly
        // discontinuous step-length chart.
        let target_steps = if i + 1 == n {
            target_total_steps
        } else {
            (target_total_steps as f64 * cadence_mass[i] / cadence_total.max(1e-9))
                .round()
                .clamp(0.0, target_total_steps as f64) as i64
        };
        let added_steps = (target_steps - steps_acc).max(0);
        steps_acc += added_steps;
        interval_steps.push(added_steps);
        let nxt = pos(s + direction * 2.0);
        let brg =
            ((nxt.0 - x).atan2(nxt.1 - y).to_degrees() + rng.gauss(0.0, 35.0)).rem_euclid(360.0);
        // 保留累计均值与 GPS 瞬时速度的轻微波动，但不生成越出跑步区域的漂移点。
        let (avg_sp, gps_speed) = if typ == -1 {
            let avg = round_to(dist_acc / t_acc.max(1.0), 4);
            let gps = if rng.random() < 0.12 {
                rng.uniform(15.0, 46.0)
            } else {
                rng.uniform(0.5, 6.0)
            };
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
            // The Android detail protocol leaves this per-sample field at
            // zero; stride is derived from the conserved step windows/laps.
            stepDistance: 0.0,
            gainTime: fmt_gain_time(start_ms + (t_acc * 1000.0) as i64),
            gainTimeMs: start_ms + (t_acc * 1000.0) as i64,
            queueNum: 0,
            coorType: "gcj02".into(),
            bdA: round_to(alt, 2),
            bdD: round_to(brg, 2),
            bdS: round_to((avg_sp * rng.uniform(0.6, 0.95)).max(0.0), 3),
            bdG: 1,
            count: rng.randint(20, 88),
            dtr: 0.0,
            state,
            locationId: String::new(),
        });
    }
    let mut segments: Vec<Segment> = Vec::new();
    let (mut seg_t, mut seg_d, mut seg_steps) = (0.0f64, 0.0f64, 0i64);
    let mut seg_start = 0.0f64;
    let mut seg_v: Vec<f64> = Vec::new();
    for i in 0..n {
        seg_t += dts[i];
        seg_d += seg_dist[i];
        seg_v.push(speeds[i]);
        seg_steps += interval_steps[i];
        if seg_t >= 60.0 || i == n - 1 {
            segments.push(Segment {
                totalTime: round_to(seg_t, 0) as i64,
                distance: round_to(seg_d, 0) as i64,
                startTime: round_to(seg_start * 1000.0, 0) as i64,
                endTime: round_to((times[i] + dts[i]) * 1000.0, 0) as i64,
                avgSpeed: round_to(seg_v.iter().sum::<f64>() / seg_v.len() as f64, 3),
                avgStep: round_to(seg_steps as f64 / seg_t.max(1.0) * 60.0, 0) as i64,
                state: 0,
            });
            seg_t = 0.0;
            seg_d = 0.0;
            seg_v.clear();
            seg_steps = 0;
            seg_start = times[i] + dts[i];
        }
    }

    // Keep both zeroed start sentinels separate from the first measured
    // interval, so no time, distance, or steps are discarded or rolled back.
    if let Some(first) = locs.first().cloned() {
        let (start_x, start_y) = ring_point_at(&dense, &arcs, s0);
        let (start_lat, start_lng) = to_bd(start_x, start_y, c_lat, c_lng);
        let mut sentinel = first;
        sentinel.gLat = round_to(start_lat, 7);
        sentinel.gLng = round_to(start_lng, 7);
        sentinel.totalTime = 0;
        sentinel.totalDis = 0.0;
        sentinel.validDis = 0.0;
        sentinel.validTime = 0;
        sentinel.steps = 0;
        sentinel.stepDistance = 0.0;
        sentinel.speed = 0.0;
        sentinel.avgSpeed = 0.0;
        sentinel.bdS = 0.0;
        sentinel.gainTime = fmt_gain_time(start_ms);
        sentinel.gainTimeMs = start_ms;
        sentinel.state = 1;
        sentinel.ptype = 0;
        sentinel.locType = 1;
        locs.insert(0, sentinel.clone());
        locs.insert(1, sentinel);
        for (index, point) in locs.iter_mut().enumerate() {
            point.id = index as i64 + 1;
        }
    }
    apply_post_fixes(&mut locs, &mut rng, start_ms);

    // 点位吸附：为每个服务器打卡点选择不同的真实轨迹采样点。
    // 旧逻辑允许所有点复用同一个最近采样点，详情页收到相同通过时刻
    // 时通常不会绘制多个勾；同时不能把起点/终点哨兵改成打卡点。
    let mut used_checkpoint_indices = std::collections::HashSet::new();
    for pl in points_bd {
        let mut best_i = None;
        let mut best_d = 1e18f64;
        for (i, q) in locs.iter().enumerate() {
            if i < 2 || i + 1 == locs.len() || used_checkpoint_indices.contains(&i) {
                continue;
            }
            let dd = ((q.gLat - pl.0) * MET_PER_DEG_LAT).powi(2)
                + ((q.gLng - pl.1) * MET_PER_DEG_LNG).powi(2);
            if dd < best_d {
                best_d = dd;
                best_i = Some(i);
            }
        }
        if let Some(i) = best_i {
            if best_d < 40.0 * 40.0 {
                used_checkpoint_indices.insert(i);
                locs[i].gLat = round_to(pl.0, 7);
                locs[i].gLng = round_to(pl.1, 7);
            }
        }
    }

    let total_dis = round_to(dist, 3);
    let mut track = Track {
        totalTime: round_to(t_acc, 0) as i64,
        totalDistance: total_dis,
        validDistance: total_dis,
        validTime: round_to(t_acc, 0) as i64,
        startTime: start_ms,
        startLatitude: locs[0].gLat,
        startLongitude: locs[0].gLng,
        totalSteps: steps_acc,
        locations: locs,
        speedPerTenSec: Vec::new(),
        stepsPerTenSec: Vec::new(),
        segments,
        altitude_gain_override: None,
    };
    let (speed_windows, step_windows) = track.ten_second_windows();
    track.speedPerTenSec = speed_windows;
    track.stepsPerTenSec = step_windows;
    track
}

/// Return the nearest generated route distance for every server checkpoint.
/// This is used both by tests and by the live flow before submission so a
/// malformed fence/point response cannot silently produce a record without
/// visible checkpoint markers.
pub fn checkpoint_distances_m(track: &Track, points_bd: &[(f64, f64)]) -> Vec<f64> {
    points_bd
        .iter()
        .map(|point| {
            track
                .locations
                .iter()
                .map(|sample| {
                    (((sample.gLat - point.0) * MET_PER_DEG_LAT).powi(2)
                        + ((sample.gLng - point.1) * MET_PER_DEG_LNG).powi(2))
                    .sqrt()
                })
                .fold(f64::INFINITY, f64::min)
        })
        .collect()
}

pub fn validate_checkpoint_hits(track: &Track, points_bd: &[(f64, f64)]) -> Result<(), String> {
    let distances = checkpoint_distances_m(track, points_bd);
    if let Some((index, distance)) = distances
        .iter()
        .enumerate()
        .find(|(_, distance)| **distance > 1.5)
    {
        return Err(format!(
            "轨迹未经过第 {} 个服务器打卡点（最近 {:.2}m）",
            index + 1,
            distance
        ));
    }
    Ok(())
}
