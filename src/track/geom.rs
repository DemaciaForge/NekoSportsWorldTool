//! 轨迹几何与随机工具。
//!
//! round 封装、RNG、打卡点线段环 + 弧长表 + 弧长插值。

use chrono::{Local, TimeZone};
use rand::rngs::StdRng;
use rand::SeedableRng;
use rand_distr::{Distribution, Normal};
use serde_json::Value;

pub const MET_PER_DEG_LAT: f64 = 111_132.0;
pub const MET_PER_DEG_LNG: f64 = 86_600.0;

/// round(x, n)：按精确二进制值四舍五入。
pub fn round_to(x: f64, n: usize) -> f64 {
    let s = format!("{x:.n$}");
    s.parse().unwrap_or(x)
}

/// 可播种 RNG 封装。
pub struct Rng {
    inner: StdRng,
}

impl Rng {
    pub fn new(seed: u64) -> Self {
        Self {
            inner: StdRng::seed_from_u64(seed),
        }
    }
    pub fn random(&mut self) -> f64 {
        rand::Rng::gen_range(&mut self.inner, 0.0..1.0)
    }
    pub fn uniform(&mut self, a: f64, b: f64) -> f64 {
        rand::Rng::gen_range(&mut self.inner, a..=b)
    }
    pub fn randint(&mut self, a: i64, b: i64) -> i64 {
        rand::Rng::gen_range(&mut self.inner, a..=b)
    }
    pub fn gauss(&mut self, mu: f64, sigma: f64) -> f64 {
        Normal::new(mu, sigma).unwrap().sample(&mut self.inner)
    }
    pub fn choice<T>(&mut self, items: &[T]) -> T
    where
        T: Copy,
    {
        items[rand::Rng::gen_range(&mut self.inner, 0..items.len())]
    }
    pub fn weighted<T>(&mut self, items: &[(T, u32)]) -> T
    where
        T: Copy,
    {
        let total: u32 = items.iter().map(|(_, w)| *w).sum();
        let mut u = self.uniform(0.0, total as f64);
        for (item, w) in items {
            u -= *w as f64;
            if u < 0.0 {
                return *item;
            }
        }
        items[items.len() - 1].0
    }
}

/// 打卡点 BD 系 (lat, lng) → 闭合平面路径 + 弧长表 + 中心。
pub type PointRing = (Vec<(f64, f64)>, Vec<f64>, (f64, f64));
pub fn make_point_ring(bd_points: &[(f64, f64)]) -> PointRing {
    make_point_ring_with_fence(bd_points, None)
}

/// Build a smooth closed route through the server checkpoints.  The old
/// implementation linearly connected adjacent points, which made every bend
/// a visible chord.  Catmull-Rom interpolation keeps every checkpoint on the
/// route while rounding the corners like a running track.
pub fn make_point_ring_with_fence(bd_points: &[(f64, f64)], fence_json: Option<&str>) -> PointRing {
    if bd_points.is_empty() {
        return (vec![(0.0, 0.0)], vec![0.0, 1.0], (0.0, 0.0));
    }
    let n = bd_points.len();
    let cx = bd_points.iter().map(|q| q.0).sum::<f64>() / n as f64;
    let cy = bd_points.iter().map(|q| q.1).sum::<f64>() / n as f64;
    // The point endpoint returns business records, not points ordered around
    // the track. Connecting that array directly can create a bow-tie route.
    // Keep the server array untouched for fixed-point metadata, but order the
    // route itself around its center so it follows the outside of the track.
    let mut ordered = bd_points.to_vec();
    ordered.sort_by(|a, b| {
        let angle_a = (a.0 - cx).atan2(a.1 - cy);
        let angle_b = (b.0 - cx).atan2(b.1 - cy);
        angle_a
            .partial_cmp(&angle_b)
            .unwrap_or(std::cmp::Ordering::Equal)
    });
    let plane: Vec<(f64, f64)> = ordered
        .iter()
        .map(|q| ((q.1 - cy) * MET_PER_DEG_LNG, (q.0 - cx) * MET_PER_DEG_LAT))
        .collect();
    let samples = 32usize;
    let mut dense = Vec::with_capacity(n * samples);
    for i in 0..n {
        let p0 = plane[(i + n - 1) % n];
        let p1 = plane[i];
        let p2 = plane[(i + 1) % n];
        let p3 = plane[(i + 2) % n];
        for j in 0..samples {
            let t = j as f64 / samples as f64;
            let t2 = t * t;
            let t3 = t2 * t;
            let x = 0.5
                * (2.0 * p1.0
                    + (-p0.0 + p2.0) * t
                    + (2.0 * p0.0 - 5.0 * p1.0 + 4.0 * p2.0 - p3.0) * t2
                    + (-p0.0 + 3.0 * p1.0 - 3.0 * p2.0 + p3.0) * t3);
            let y = 0.5
                * (2.0 * p1.1
                    + (-p0.1 + p2.1) * t
                    + (2.0 * p0.1 - 5.0 * p1.1 + 4.0 * p2.1 - p3.1) * t2
                    + (-p0.1 + 3.0 * p1.1 - 3.0 * p2.1 + p3.1) * t3);
            dense.push((x, y));
        }
    }
    // Keep interpolated samples inside the real campus fence.  Checkpoints
    // themselves are left untouched; only curve overshoot is clamped.
    if let Some(fence) = parse_fence_plane(fence_json, cx, cy) {
        let centroid = (
            fence.iter().map(|p| p.0).sum::<f64>() / fence.len() as f64,
            fence.iter().map(|p| p.1).sum::<f64>() / fence.len() as f64,
        );
        for point in &mut dense {
            if !point_in_polygon(*point, &fence) {
                let nearest = nearest_on_polygon(*point, &fence);
                // Move just inside the boundary.  The tiny inward offset also
                // prevents rounded coordinates from falling on the fence.
                let vx = centroid.0 - nearest.0;
                let vy = centroid.1 - nearest.1;
                let norm = (vx * vx + vy * vy).sqrt().max(1.0);
                *point = (nearest.0 + vx / norm, nearest.1 + vy / norm);
            }
        }
    }
    let mut arcs = vec![0.0f64];
    for i in 1..=dense.len() {
        let a = dense[i - 1];
        let b = dense[i % dense.len()];
        arcs.push(arcs[i - 1] + ((b.0 - a.0).powi(2) + (b.1 - a.1).powi(2)).sqrt());
    }
    (dense, arcs, (cx, cy))
}

/// Parse the server fence into the local metre plane used by the route
/// generator.  The endpoint has returned both arrays and JSON-encoded strings
/// over time, so unwrap strings before looking for a polygon.
pub fn parse_fence_plane(text: Option<&str>, c_lat: f64, c_lng: f64) -> Option<Vec<(f64, f64)>> {
    let text = text?.trim();
    let root = serde_json::from_str::<Value>(text).ok()?;
    let points = find_polygon_points(&root)?;
    let mut out = Vec::new();
    for point in points {
        let (lat, lng) = if let Some(object) = point.as_object() {
            (
                number(&Value::Object(object.clone()), &["lat", "latitude", "glat"])?,
                number(
                    &Value::Object(object.clone()),
                    &["lng", "lon", "longitude", "glng", "glon"],
                )?,
            )
        } else {
            let pair = point.as_array()?;
            let lng = pair.first().and_then(number_value)?;
            let lat = pair.get(1).and_then(number_value)?;
            (lat, lng)
        };
        out.push((
            (lng - c_lng) * MET_PER_DEG_LNG,
            (lat - c_lat) * MET_PER_DEG_LAT,
        ));
    }
    (out.len() >= 3).then_some(out)
}

fn find_polygon_points(value: &Value) -> Option<Vec<Value>> {
    if let Some(array) = value.as_array() {
        if array.len() >= 3
            && array.iter().all(|item| {
                item.as_object().is_some_and(|object| {
                    object
                        .keys()
                        .any(|key| ["lat", "latitude", "glat"].contains(&key.as_str()))
                        && object.keys().any(|key| {
                            ["lng", "lon", "longitude", "glng", "glon"].contains(&key.as_str())
                        })
                }) || item.as_array().is_some_and(|pair| {
                    pair.len() >= 2
                        && pair.first().and_then(number_value).is_some()
                        && pair.get(1).and_then(number_value).is_some()
                })
            })
        {
            return Some(array.clone());
        }
        for item in array {
            if let Some(found) = find_polygon_points(item) {
                return Some(found);
            }
        }
    }
    if let Some(object) = value.as_object() {
        for key in [
            "points",
            "vertices",
            "polygon",
            "path",
            "coordinates",
            "geoFence",
            "geoFences",
            "fences",
            "data",
        ] {
            if let Some(child) = object.get(key) {
                if let Some(found) = find_polygon_points(child) {
                    return Some(found);
                }
            }
        }
    }
    if let Some(text) = value.as_str() {
        if let Ok(parsed) = serde_json::from_str::<Value>(text) {
            return find_polygon_points(&parsed);
        }
    }
    None
}

fn number(value: &Value, names: &[&str]) -> Option<f64> {
    names.iter().find_map(|name| {
        value.get(*name).and_then(|v| {
            v.as_f64()
                .or_else(|| v.as_str().and_then(|s| s.trim().parse().ok()))
                .filter(|n: &f64| n.is_finite())
        })
    })
}

fn number_value(value: &Value) -> Option<f64> {
    value
        .as_f64()
        .or_else(|| value.as_str().and_then(|s| s.trim().parse().ok()))
        .filter(|n: &f64| n.is_finite())
}

fn point_in_polygon(point: (f64, f64), polygon: &[(f64, f64)]) -> bool {
    let mut inside = false;
    for i in 0..polygon.len() {
        let a = polygon[i];
        let b = polygon[(i + 1) % polygon.len()];
        let crosses = (a.1 > point.1) != (b.1 > point.1)
            && point.0 < (b.0 - a.0) * (point.1 - a.1) / (b.1 - a.1) + a.0;
        if crosses {
            inside = !inside;
        }
    }
    inside
}

fn nearest_on_polygon(point: (f64, f64), polygon: &[(f64, f64)]) -> (f64, f64) {
    polygon
        .iter()
        .enumerate()
        .map(|(i, &a)| {
            let b = polygon[(i + 1) % polygon.len()];
            let dx = b.0 - a.0;
            let dy = b.1 - a.1;
            let t = if dx * dx + dy * dy > 0.0 {
                ((point.0 - a.0) * dx + (point.1 - a.1) * dy) / (dx * dx + dy * dy)
            } else {
                0.0
            };
            let t = t.clamp(0.0, 1.0);
            let q = (a.0 + dx * t, a.1 + dy * t);
            (q, (q.0 - point.0).powi(2) + (q.1 - point.1).powi(2))
        })
        .min_by(|a, b| a.1.partial_cmp(&b.1).unwrap_or(std::cmp::Ordering::Equal))
        .map(|item| item.0)
        .unwrap_or(point)
}

/// Keep a human-like lateral offset inside the actual campus fence. `base`
/// must already be a point on the constrained route; when the candidate is
/// outside, binary search along the short offset segment rather than snapping
/// to a polygon vertex, which would create a visible corner.
pub fn clamp_inside_fence(
    candidate: (f64, f64),
    base: (f64, f64),
    fence: Option<&[(f64, f64)]>,
) -> (f64, f64) {
    let Some(polygon) = fence else {
        return candidate;
    };
    if point_in_polygon(candidate, polygon) {
        return candidate;
    }
    if !point_in_polygon(base, polygon) {
        return nearest_on_polygon(candidate, polygon);
    }
    let mut lo = 0.0;
    let mut hi = 1.0;
    for _ in 0..24 {
        let mid = (lo + hi) * 0.5;
        let point = (
            base.0 + (candidate.0 - base.0) * mid,
            base.1 + (candidate.1 - base.1) * mid,
        );
        if point_in_polygon(point, polygon) {
            lo = mid;
        } else {
            hi = mid;
        }
    }
    let t = (lo * 0.999).max(0.0);
    (
        base.0 + (candidate.0 - base.0) * t,
        base.1 + (candidate.1 - base.1) * t,
    )
}

/// 环线弧长 → 坐标（线性插值）。
pub fn ring_point_at(dense: &[(f64, f64)], arcs: &[f64], s: f64) -> (f64, f64) {
    let total = *arcs.last().unwrap_or(&1.0);
    let s = s.rem_euclid(total);
    let mut lo = 0usize;
    let mut hi = arcs.len();
    while lo < hi {
        let mid = (lo + hi) / 2;
        if arcs[mid] < s {
            lo = mid + 1;
        } else {
            hi = mid;
        }
    }
    let i = lo.max(1);
    let a = dense[(i - 1) % dense.len()];
    let b = dense[i % dense.len()];
    let seg = arcs[i] - arcs[i - 1];
    let t = if seg > 0.0 {
        (s - arcs[i - 1]) / seg
    } else {
        0.0
    };
    (a.0 + (b.0 - a.0) * t, a.1 + (b.1 - a.1) * t)
}

pub fn to_bd(x: f64, y: f64, c_lat: f64, c_lng: f64) -> (f64, f64) {
    (c_lat + y / MET_PER_DEG_LAT, c_lng + x / MET_PER_DEG_LNG)
}

// ── 坐标基准转换：WGS84 ↔ GCJ-02 ↔ BD-09 ─────────────────────────
// 打卡点来自服务端为 BD-09；OSM 路网为 WGS84。真实道路路由需先把路网
// 逐节点转成 BD-09，与经典模式（直接在 BD-09 上拟合打卡点环）保持一致。

const X_PI: f64 = std::f64::consts::PI * 3000.0 / 180.0;
const WGS_A: f64 = 6378245.0;
const WGS_EE: f64 = 0.00669342162296594323;

fn out_of_china(lat: f64, lng: f64) -> bool {
    lng < 72.004 || lng > 137.8347 || lat < 0.8293 || lat > 55.8271
}

fn transform_lat(x: f64, y: f64) -> f64 {
    let pi = std::f64::consts::PI;
    let mut ret = -100.0 + 2.0 * x + 3.0 * y + 0.2 * y * y + 0.1 * x * y + 0.2 * x.abs().sqrt();
    ret += (20.0 * (6.0 * x * pi).sin() + 20.0 * (2.0 * x * pi).sin()) * 2.0 / 3.0;
    ret += (20.0 * (y * pi).sin() + 40.0 * (y / 3.0 * pi).sin()) * 2.0 / 3.0;
    ret += (160.0 * (y / 12.0 * pi).sin() + 320.0 * (y * pi / 30.0).sin()) * 2.0 / 3.0;
    ret
}

fn transform_lng(x: f64, y: f64) -> f64 {
    let pi = std::f64::consts::PI;
    let mut ret = 300.0 + x + 2.0 * y + 0.1 * x * x + 0.1 * x * y + 0.1 * x.abs().sqrt();
    ret += (20.0 * (6.0 * x * pi).sin() + 20.0 * (2.0 * x * pi).sin()) * 2.0 / 3.0;
    ret += (20.0 * (x * pi).sin() + 40.0 * (x / 3.0 * pi).sin()) * 2.0 / 3.0;
    ret += (150.0 * (x / 12.0 * pi).sin() + 300.0 * (x / 30.0 * pi).sin()) * 2.0 / 3.0;
    ret
}

/// WGS84 → GCJ-02（火星坐标，境内偏移）。
pub fn wgs84_to_gcj02(lat: f64, lng: f64) -> (f64, f64) {
    if out_of_china(lat, lng) {
        return (lat, lng);
    }
    let pi = std::f64::consts::PI;
    let mut dlat = transform_lat(lng - 105.0, lat - 35.0);
    let mut dlng = transform_lng(lng - 105.0, lat - 35.0);
    let radlat = lat / 180.0 * pi;
    let mut magic = radlat.sin();
    magic = 1.0 - WGS_EE * magic * magic;
    let sqrtmagic = magic.sqrt();
    dlat = (dlat * 180.0) / ((WGS_A * (1.0 - WGS_EE)) / (magic * sqrtmagic) * pi);
    dlng = (dlng * 180.0) / (WGS_A / sqrtmagic * radlat.cos() * pi);
    (lat + dlat, lng + dlng)
}

/// GCJ-02 → BD-09（`wire::bd09_to_gcj02` 的逆变换）。
pub fn gcj02_to_bd09(gcj_lat: f64, gcj_lng: f64) -> (f64, f64) {
    let z = (gcj_lng * gcj_lng + gcj_lat * gcj_lat).sqrt() + 0.00002 * (gcj_lat * X_PI).sin();
    let theta = gcj_lat.atan2(gcj_lng) + 0.000003 * (gcj_lng * X_PI).cos();
    (z * theta.sin() + 0.006, z * theta.cos() + 0.0065)
}

/// WGS84 → BD-09（OSM 路网对齐打卡点用）。
pub fn wgs84_to_bd09(lat: f64, lng: f64) -> (f64, f64) {
    let (g_lat, g_lng) = wgs84_to_gcj02(lat, lng);
    gcj02_to_bd09(g_lat, g_lng)
}

pub fn fmt_gain_time(ms: i64) -> String {
    Local
        .timestamp_millis_opt(ms)
        .single()
        .map(|t| t.format("%Y-%m-%d %H:%M:%S").to_string())
        .unwrap_or_default()
}
