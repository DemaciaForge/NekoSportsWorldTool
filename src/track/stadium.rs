//! Standard athletics-track geometry.
//!
//! The server's checkpoint response is useful for locating and orienting a
//! track, but connecting those points directly produces long chords between
//! bends.  This module builds a fixed-size two-straight/two-semicircle track
//! instead.  Coordinates are returned in the metre plane used by `geom`.

use serde::{Deserialize, Serialize};

use super::geom::{parse_fence_plane, PointRing, MET_PER_DEG_LAT, MET_PER_DEG_LNG};

/// Requested track preset. `Auto` preserves the legacy checkpoint-ring path.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum TrackSpec {
    #[serde(rename = "auto", alias = "Auto")]
    Auto,
    #[serde(rename = "200m", alias = "200", alias = "m200", alias = "M200")]
    M200,
    #[serde(rename = "300m", alias = "300", alias = "m300", alias = "M300")]
    M300,
    #[serde(rename = "400m", alias = "400", alias = "m400", alias = "M400")]
    M400,
    /// A school-specific oval length.  The value is the nominal measurement
    /// line length in metres and is kept bounded by `dimensions`.
    #[serde(rename = "custom")]
    Custom { total_m: u32 },
}

impl Default for TrackSpec {
    fn default() -> Self {
        Self::Auto
    }
}

impl TrackSpec {
    pub fn parse(value: &str) -> Option<Self> {
        match value.trim().to_ascii_lowercase().as_str() {
            "auto" | "default" | "自动" => Some(Self::Auto),
            "200" | "200m" | "m200" => Some(Self::M200),
            "300" | "300m" | "m300" => Some(Self::M300),
            "400" | "400m" | "m400" => Some(Self::M400),
            _ => None,
        }
    }

    /// Parse a preset or a custom selector when the length is supplied by a
    /// separate CLI/UI field.  A bare `custom` without a length is rejected
    /// instead of silently inventing a track size.
    pub fn parse_with_length(value: &str, custom_length_m: Option<u32>) -> Option<Self> {
        if value.trim().eq_ignore_ascii_case("custom") || value.trim() == "自定义" {
            return custom_length_m
                .filter(|length| (100..=1000).contains(length))
                .map(|total_m| Self::Custom { total_m });
        }
        Self::parse(value)
    }

    pub const fn label(self) -> &'static str {
        match self {
            Self::Auto => "自动（服务器点位）",
            Self::M200 => "200 米标准跑道",
            Self::M300 => "300 米标准跑道",
            Self::M400 => "400 米标准跑道",
            Self::Custom { total_m } => {
                // Keep the label allocation-free for the existing UI API.
                // Callers that need the exact value can format `total_m`.
                if total_m == 0 {
                    "自定义跑道"
                } else {
                    "自定义跑道"
                }
            }
        }
    }

    /// (straight length, measurement-line radius), in metres.
    ///
    /// The 400 m values are the IAAF-style measurement line dimensions used
    /// by the project: 84.39 m straights and a 36.80 m radius.  Smaller
    /// school tracks commonly use 36.80/20.00 m (nominal 200 m) and
    /// 71.46/25.00 m (nominal 300 m) layouts.
    pub const fn dimensions(self) -> Option<TrackDimensions> {
        let total_m = match self {
            Self::Auto => return None,
            Self::M200 => 200,
            Self::M300 => 300,
            Self::M400 => 400,
            Self::Custom { total_m } if total_m >= 100 && total_m <= 1000 => total_m,
            Self::Custom { .. } => return None,
        };
        // Use the 400 m measurement line as the sole geometry baseline. This
        // keeps 200/300 m presets physically consistent and makes custom
        // lengths a true proportional variant rather than unrelated shapes.
        let base = TrackDimensions {
            straight_m: 84.39,
            radius_m: 36.80,
        };
        let base_length = base.nominal_length_m();
        let scale = total_m as f64 / base_length;
        Some(TrackDimensions {
            straight_m: base.straight_m * scale,
            radius_m: base.radius_m * scale,
        })
    }
}

#[derive(Debug, Clone, Copy, PartialEq)]
pub struct TrackDimensions {
    pub straight_m: f64,
    pub radius_m: f64,
}

impl TrackDimensions {
    pub const fn nominal_length_m(self) -> f64 {
        2.0 * self.straight_m + 2.0 * std::f64::consts::PI * self.radius_m
    }
}

/// Build a fixed-size athletics track around the server location.
///
/// `bd_points` are `(latitude, longitude)` in BD-09. `fence_json`, when
/// present, is converted to the same local metre plane and used as a hard
/// containment check.  A standard track that cannot fit the supplied fence
/// returns an error instead of silently scaling or bending the track.
pub fn make_stadium_ring(
    bd_points: &[(f64, f64)],
    fence_json: Option<&str>,
    spec: TrackSpec,
) -> Result<PointRing, String> {
    let dimensions = spec
        .dimensions()
        .ok_or_else(|| "TrackSpec::Auto 不应直接调用标准跑道生成器".to_string())?;
    if bd_points.is_empty() && fence_json.is_none() {
        return Err("没有服务器点位或围栏，无法定位标准跑道".to_string());
    }
    if bd_points
        .iter()
        .any(|(lat, lng)| !lat.is_finite() || !lng.is_finite())
    {
        return Err("服务器打卡点包含非有限坐标".to_string());
    }

    let (mut c_lat, mut c_lng) = mean_bd(bd_points).unwrap_or((0.0, 0.0));
    let mut fence = fence_json.and_then(|text| parse_fence_plane(Some(text), c_lat, c_lng));
    if fence_json.is_some() && fence.is_none() {
        return Err("服务器围栏 JSON 无法解析为至少 3 个坐标点".to_string());
    }
    if fence.as_ref().is_some_and(|polygon| polygon.len() < 3) {
        return Err("围栏顶点少于 3 个，无法拟合标准跑道".to_string());
    }
    // With no checkpoints the initial origin is arbitrary. Recenter on the
    // fence centroid and parse again so the generated ring and fence share
    // the same local metre plane.
    if bd_points.is_empty() {
        if let Some(polygon) = fence.as_deref() {
            let (fx, fy) = centroid(polygon);
            c_lng += fx / MET_PER_DEG_LNG;
            c_lat += fy / MET_PER_DEG_LAT;
            fence = fence_json.and_then(|text| parse_fence_plane(Some(text), c_lat, c_lng));
        }
    }

    let source = source_plane(bd_points, fence.as_deref(), c_lat, c_lng);
    // Fit the orientation and centre against the actual rounded-rectangle
    // centerline. PCA is unstable when only a few checkpoints are returned,
    // while this bounded search remains deterministic and handles rotated
    // school tracks.
    let (theta, fit_offset) = fit_pose(&source, dimensions);
    let dense_local = rounded_rectangle(dimensions);
    let mut dense: Vec<(f64, f64)> = dense_local
        .into_iter()
        .map(|point| rotate((point.0 + fit_offset.0, point.1 + fit_offset.1), theta))
        .collect();

    // A fixed-size preset must still describe the same facility as the
    // server points. Reject a gross mismatch early so the caller can fall
    // back to Auto or report the selected specification to the user.
    if !bd_points.is_empty() {
        if let Some(polygon) = fence.as_deref() {
            let outside = dense
                .iter()
                .enumerate()
                .find(|(_, point)| !inside_or_near(**point, polygon, 0.35));
            if let Some((index, point)) = outside {
                return Err(format!(
                "{} 米跑道无法完整落入服务器围栏：第 {} 个采样点 ({:.1}, {:.1}) 越界；请检查跑道规格或围栏",
                spec_label_short(spec), index, point.0, point.1
            ));
            }
        }

        // Checkpoints are expected to lie on (or just beside) the selected
        // running line. A tight tolerance prevents a wildly wrong preset from
        // producing long straight joins after anchor insertion.
        let tolerance = dimensions.radius_m * 0.25 + 2.0;
        let max_distance = bd_points
            .iter()
            .map(|&(lat, lng)| {
                let point = rotate(
                    (
                        (lng - c_lng) * MET_PER_DEG_LNG,
                        (lat - c_lat) * MET_PER_DEG_LAT,
                    ),
                    -theta,
                );
                distance_to_centerline((point.0 - fit_offset.0, point.1 - fit_offset.1), dimensions)
            })
            .fold(0.0, f64::max);
        if max_distance > tolerance {
            return Err(format!(
                "服务器打卡点与{}米跑道中心线不匹配：最大偏差 {:.1}m（允许 {:.1}m）",
                spec_label_short(spec),
                max_distance,
                tolerance
            ));
        }
    }

    // Make every server checkpoint an actual route sample.  The old generator
    // only snapped the emitted location field after sampling, so the path
    // between two checkpoints could still be a long chord.  Assigning unique
    // nearest samples keeps the ring smooth while guaranteeing a route hit.
    let anchor_indices = inject_checkpoint_anchors(&mut dense, bd_points, c_lat, c_lng, theta)?;

    // Replacing a sampled point with the exact server coordinate changes the
    // two adjacent chord lengths.  This is normally a sub-metre difference,
    // but it made the fixed-size track occasionally fail its length check
    // (for example 401.05 m for a nominal 400 m track).  Normalize only the
    // non-anchor samples around the ring centroid so every checkpoint remains
    // exact while the closed route keeps its requested circumference.
    normalize_ring_length(&mut dense, dimensions.nominal_length_m(), &anchor_indices)?;

    if let Some(polygon) = fence.as_deref() {
        if let Some((index, point)) = dense
            .iter()
            .enumerate()
            .find(|(_, point)| !inside_or_near(**point, polygon, 0.35))
        {
            return Err(format!(
                "{} 米跑道长度归一化后越出服务器围栏：第 {} 个采样点 ({:.1}, {:.1})",
                spec_label_short(spec),
                index,
                point.0,
                point.1
            ));
        }
    }

    let expected = dimensions.nominal_length_m();
    let mut arcs = vec![0.0f64];
    for i in 1..=dense.len() {
        let a = dense[i - 1];
        let b = dense[i % dense.len()];
        arcs.push(arcs[i - 1] + distance(a, b));
    }
    let actual = *arcs.last().unwrap_or(&0.0);
    if (actual - expected).abs() > 2.0 {
        return Err(format!(
            "标准跑道采样长度异常：实际 {:.2}m，设计 {:.2}m",
            actual, expected
        ));
    }
    Ok((dense, arcs, (c_lat, c_lng)))
}

fn spec_label_short(spec: TrackSpec) -> &'static str {
    match spec {
        TrackSpec::M200 => "200",
        TrackSpec::M300 => "300",
        TrackSpec::M400 => "400",
        TrackSpec::Custom { .. } => "自定义",
        TrackSpec::Auto => "自动",
    }
}

fn inject_checkpoint_anchors(
    dense: &mut [(f64, f64)],
    bd_points: &[(f64, f64)],
    c_lat: f64,
    c_lng: f64,
    _theta: f64,
) -> Result<Vec<usize>, String> {
    if bd_points.is_empty() {
        return Ok(Vec::new());
    }
    if dense.len() < bd_points.len() {
        return Err("标准跑道采样点不足，无法为所有服务器打卡点分配唯一锚点".to_string());
    }
    let mut nearest: Vec<(usize, (f64, f64), f64)> = bd_points
        .iter()
        .map(|&(lat, lng)| {
            let world = (
                (lng - c_lng) * MET_PER_DEG_LNG,
                (lat - c_lat) * MET_PER_DEG_LAT,
            );
            let target = world;
            let (index, _) = dense
                .iter()
                .enumerate()
                .map(|(index, &point)| (index, distance(point, target)))
                .min_by(|a, b| a.1.partial_cmp(&b.1).unwrap_or(std::cmp::Ordering::Equal))
                .unwrap_or((0, f64::INFINITY));
            (index, world, distance(dense[index], world))
        })
        .collect();
    nearest.sort_by_key(|entry| entry.0);
    let mut used = std::collections::HashSet::new();
    let mut anchors = Vec::with_capacity(bd_points.len());
    for (preferred, world, distance_to_ring) in nearest {
        let mut chosen = None;
        // Search in both directions around the nearest sample. The generated
        // ring has hundreds of samples, so this remains cheap and handles two
        // checkpoints that quantize to the same sample without collisions.
        for offset in 0..dense.len() {
            let candidates = [
                (preferred + offset) % dense.len(),
                (preferred + dense.len() - offset % dense.len()) % dense.len(),
            ];
            if let Some(index) = candidates.into_iter().find(|index| !used.contains(index)) {
                chosen = Some(index);
                break;
            }
        }
        let Some(index) = chosen else {
            return Err("服务器打卡点无法分配唯一跑道锚点".to_string());
        };
        used.insert(index);
        anchors.push(index);
        // Preserve the nominal oval when the server point is already close to
        // it. For small endpoint rounding offsets, retaining the exact point
        // makes the detail page's passed marker land on the route. A larger
        // mismatch is rejected by the centerline validation above and remains
        // on the standard curve here rather than creating a long chord.
        if distance_to_ring <= 3.0 {
            dense[index] = world;
        }
    }
    Ok(anchors)
}

/// Preserve a fixed-size ring's circumference after exact checkpoint
/// insertion.  Anchors are kept byte-for-byte unchanged; all other samples
/// are scaled around the ring centroid and a monotonic bisection chooses the
/// scale whose polygonal circumference matches `target`.
fn normalize_ring_length(
    dense: &mut [(f64, f64)],
    target: f64,
    anchor_indices: &[usize],
) -> Result<(), String> {
    if dense.len() < 3 || !target.is_finite() || target <= 0.0 {
        return Err("标准跑道长度归一化参数无效".to_string());
    }
    let anchor_set: std::collections::HashSet<usize> = anchor_indices
        .iter()
        .copied()
        .filter(|&index| index < dense.len())
        .collect();
    let center = centroid(dense);
    let original = dense.to_vec();
    let length_at = |scale: f64| -> f64 {
        let point_at = |index: usize| {
            let point = original[index];
            if anchor_set.contains(&index) {
                point
            } else {
                (
                    center.0 + (point.0 - center.0) * scale,
                    center.1 + (point.1 - center.1) * scale,
                )
            }
        };
        (0..original.len())
            .map(|index| distance(point_at(index), point_at((index + 1) % original.len())))
            .sum()
    };

    let current = length_at(1.0);
    if (current - target).abs() <= 0.01 {
        return Ok(());
    }

    let mut lo = 0.25;
    let mut hi = 1.75;
    let mut lo_length = length_at(lo);
    let mut hi_length = length_at(hi);
    // The normal correction is tiny, but expand the bracket for unusual
    // checkpoint offsets before declaring the fixed anchor geometry
    // impossible to normalize.
    for _ in 0..8 {
        if lo_length <= target && target <= hi_length {
            break;
        }
        if target < lo_length {
            lo *= 0.5;
            lo_length = length_at(lo);
        } else if target > hi_length {
            hi *= 2.0;
            hi_length = length_at(hi);
        }
    }
    if target < lo_length - 0.01 || target > hi_length + 0.01 {
        // Keeping checkpoint anchors exact can leave a small irreducible
        // perimeter residual.  GPS uncertainty is larger than this; accept
        // the route when the observed error is at most two metres.
        if (current - target).abs() <= 2.0 {
            return Ok(());
        }
        return Err(format!(
            "标准跑道无法在保留打卡点的情况下归一化长度：当前 {:.2}m，目标 {:.2}m",
            current, target
        ));
    }

    for _ in 0..64 {
        let mid = (lo + hi) / 2.0;
        if length_at(mid) < target {
            lo = mid;
        } else {
            hi = mid;
        }
    }
    let scale = (lo + hi) / 2.0;
    for (index, point) in dense.iter_mut().enumerate() {
        if !anchor_set.contains(&index) {
            point.0 = center.0 + (original[index].0 - center.0) * scale;
            point.1 = center.1 + (original[index].1 - center.1) * scale;
        }
    }
    Ok(())
}

fn mean_bd(points: &[(f64, f64)]) -> Option<(f64, f64)> {
    if points.is_empty() {
        return None;
    }
    Some((
        points.iter().map(|point| point.0).sum::<f64>() / points.len() as f64,
        points.iter().map(|point| point.1).sum::<f64>() / points.len() as f64,
    ))
}

fn source_plane(
    points: &[(f64, f64)],
    fence: Option<&[(f64, f64)]>,
    c_lat: f64,
    c_lng: f64,
) -> Vec<(f64, f64)> {
    let mut source: Vec<(f64, f64)> = points
        .iter()
        .map(|(lat, lng)| {
            (
                (lng - c_lng) * MET_PER_DEG_LNG,
                (lat - c_lat) * MET_PER_DEG_LAT,
            )
        })
        .collect();
    // Checkpoints identify the track better than a broad campus polygon. If
    // there are too few points, the fence provides the only orientation hint.
    if source.len() < 2 {
        if let Some(fence) = fence {
            source = fence.to_vec();
        }
    }
    source
}

fn centroid(points: &[(f64, f64)]) -> (f64, f64) {
    let count = points.len().max(1) as f64;
    (
        points.iter().map(|point| point.0).sum::<f64>() / count,
        points.iter().map(|point| point.1).sum::<f64>() / count,
    )
}

fn bbox_center(points: &[(f64, f64)]) -> (f64, f64) {
    let Some(&(first_x, first_y)) = points.first() else {
        return (0.0, 0.0);
    };
    let (mut min_x, mut max_x, mut min_y, mut max_y) = (first_x, first_x, first_y, first_y);
    for &(x, y) in &points[1..] {
        min_x = min_x.min(x);
        max_x = max_x.max(x);
        min_y = min_y.min(y);
        max_y = max_y.max(y);
    }
    ((min_x + max_x) / 2.0, (min_y + max_y) / 2.0)
}

fn fit_pose(points: &[(f64, f64)], dimensions: TrackDimensions) -> (f64, (f64, f64)) {
    if points.len() < 2 {
        return (0.0, bbox_center(points));
    }

    let score = |theta: f64| {
        let local: Vec<_> = points.iter().map(|&point| rotate(point, -theta)).collect();
        let offset = bbox_center(&local);
        let error = local
            .iter()
            .map(|&(x, y)| {
                let distance = distance_to_centerline((x - offset.0, y - offset.1), dimensions);
                distance * distance
            })
            .sum::<f64>();
        (error, offset)
    };

    let mut best_theta = 0.0;
    let mut best = f64::INFINITY;
    let coarse_step = std::f64::consts::PI / 180.0;
    for index in 0..=180 {
        let theta = -std::f64::consts::FRAC_PI_2 + index as f64 * coarse_step;
        let (error, _) = score(theta);
        if error < best {
            best = error;
            best_theta = theta;
        }
    }

    // Refine the coarse result to keep checkpoint anchors within a metre even
    // when the supplied point set is sparse or slightly noisy.
    let fine_step = coarse_step / 20.0;
    let mut theta = best_theta - coarse_step;
    while theta <= best_theta + coarse_step {
        let (error, _) = score(theta);
        if error < best {
            best = error;
            best_theta = theta;
        }
        theta += fine_step;
    }
    let (_, offset) = score(best_theta);
    (best_theta, offset)
}

fn rounded_rectangle(dimensions: TrackDimensions) -> Vec<(f64, f64)> {
    let straight = dimensions.straight_m;
    let radius = dimensions.radius_m;
    let mut points = Vec::new();
    let sample_count = |length: f64| (length / 0.75).ceil().max(8.0) as usize;

    let n_straight = sample_count(straight);
    for i in 0..n_straight {
        let t = i as f64 / n_straight as f64;
        points.push((-straight / 2.0 + straight * t, radius));
    }
    let n_arc = sample_count(std::f64::consts::PI * radius);
    for i in 0..n_arc {
        let t = i as f64 / n_arc as f64;
        let angle = std::f64::consts::FRAC_PI_2 - std::f64::consts::PI * t;
        points.push((straight / 2.0 + radius * angle.cos(), radius * angle.sin()));
    }
    for i in 0..n_straight {
        let t = i as f64 / n_straight as f64;
        points.push((straight / 2.0 - straight * t, -radius));
    }
    for i in 0..n_arc {
        let t = i as f64 / n_arc as f64;
        let angle = -std::f64::consts::FRAC_PI_2 - std::f64::consts::PI * t;
        points.push((-straight / 2.0 + radius * angle.cos(), radius * angle.sin()));
    }
    points
}

fn rotate(point: (f64, f64), angle: f64) -> (f64, f64) {
    let (sin, cos) = angle.sin_cos();
    (point.0 * cos - point.1 * sin, point.0 * sin + point.1 * cos)
}

fn distance(a: (f64, f64), b: (f64, f64)) -> f64 {
    (a.0 - b.0).hypot(a.1 - b.1)
}

fn distance_to_centerline(point: (f64, f64), dimensions: TrackDimensions) -> f64 {
    let half_straight = dimensions.straight_m / 2.0;
    let radius = dimensions.radius_m;
    if point.0.abs() <= half_straight {
        (point.1.abs() - radius).abs()
    } else {
        let center_x = if point.0.is_sign_negative() {
            -half_straight
        } else {
            half_straight
        };
        ((point.0 - center_x).hypot(point.1) - radius).abs()
    }
}

fn inside_or_near(point: (f64, f64), polygon: &[(f64, f64)], tolerance: f64) -> bool {
    if point_in_polygon(point, polygon) {
        return true;
    }
    polygon.iter().enumerate().any(|(index, &a)| {
        let b = polygon[(index + 1) % polygon.len()];
        distance_to_segment(point, a, b) <= tolerance
    })
}

fn point_in_polygon(point: (f64, f64), polygon: &[(f64, f64)]) -> bool {
    let mut inside = false;
    for index in 0..polygon.len() {
        let a = polygon[index];
        let b = polygon[(index + 1) % polygon.len()];
        if (a.1 > point.1) != (b.1 > point.1)
            && point.0 < (b.0 - a.0) * (point.1 - a.1) / (b.1 - a.1) + a.0
        {
            inside = !inside;
        }
    }
    inside
}

fn distance_to_segment(point: (f64, f64), a: (f64, f64), b: (f64, f64)) -> f64 {
    let dx = b.0 - a.0;
    let dy = b.1 - a.1;
    let denominator = dx * dx + dy * dy;
    let t = if denominator > 0.0 {
        ((point.0 - a.0) * dx + (point.1 - a.1) * dy) / denominator
    } else {
        0.0
    }
    .clamp(0.0, 1.0);
    distance(point, (a.0 + t * dx, a.1 + t * dy))
}

#[cfg(test)]
mod tests {
    use super::*;

    fn center_points(lat: f64, lng: f64, dimensions: TrackDimensions) -> Vec<(f64, f64)> {
        let local = rounded_rectangle(dimensions);
        local
            .iter()
            .step_by((local.len() / 8).max(1))
            .map(|&(x, y)| (lat + y / MET_PER_DEG_LAT, lng + x / MET_PER_DEG_LNG))
            .collect()
    }

    #[test]
    fn specs_parse_and_have_expected_dimensions() {
        assert_eq!(TrackSpec::parse("200m"), Some(TrackSpec::M200));
        assert_eq!(TrackSpec::parse("400"), Some(TrackSpec::M400));
        assert_eq!(TrackSpec::parse("wat"), None);
        assert!((TrackSpec::M400.dimensions().unwrap().nominal_length_m() - 400.0012).abs() < 0.01);
    }

    #[test]
    fn standard_ring_preserves_nominal_length() {
        for spec in [TrackSpec::M200, TrackSpec::M300, TrackSpec::M400] {
            let dimensions = spec.dimensions().unwrap();
            let raw = rounded_rectangle(dimensions);
            let raw_len: f64 = raw
                .iter()
                .enumerate()
                .map(|(i, a)| distance(*a, raw[(i + 1) % raw.len()]))
                .sum();
            let _ = raw_len;
            let points = center_points(38.9, 121.54, dimensions);
            let ring = make_stadium_ring(&points, None, spec).unwrap();
            let actual = *ring.1.last().unwrap();
            assert!((actual - dimensions.nominal_length_m()).abs() < 0.2);
        }
    }

    #[test]
    fn checkpoint_anchor_injection_keeps_nominal_length() {
        let dimensions = TrackSpec::M400.dimensions().unwrap();
        let points = center_points(38.9, 121.54, dimensions);
        let ring = make_stadium_ring(&points, None, TrackSpec::M400).unwrap();
        let actual = *ring.1.last().unwrap();
        assert!((actual - dimensions.nominal_length_m()).abs() < 0.05);
    }

    #[test]
    fn ring_uses_checkpoint_orientation() {
        let dimensions = TrackSpec::M400.dimensions().unwrap();
        let mut points = center_points(38.9, 121.54, dimensions);
        for point in &mut points {
            let x = (point.1 - 121.54) * MET_PER_DEG_LNG;
            let y = (point.0 - 38.9) * MET_PER_DEG_LAT;
            let rotated = rotate((x, y), 0.35);
            point.0 = 38.9 + rotated.1 / MET_PER_DEG_LAT;
            point.1 = 121.54 + rotated.0 / MET_PER_DEG_LNG;
        }
        let ring = make_stadium_ring(&points, None, TrackSpec::M400).unwrap();
        assert!(ring.0.iter().any(|point| point.0.abs() > 75.0));
    }

    #[test]
    fn fence_that_is_too_small_is_reported() {
        let fence = serde_json::json!({
            "polygon": [[121.5397, 38.8997], [121.5403, 38.8997], [121.5403, 38.9003], [121.5397, 38.9003]]
        })
        .to_string();
        let points = [(38.9, 121.54), (38.9, 121.5401)];
        let error = make_stadium_ring(&points, Some(&fence), TrackSpec::M400).unwrap_err();
        assert!(error.contains("无法完整落入服务器围栏"));
    }

    #[test]
    fn malformed_fence_is_reported_instead_of_ignored() {
        let points = [(38.9, 121.54), (38.9002, 121.5402)];
        let error = make_stadium_ring(&points, Some("{}"), TrackSpec::M200).unwrap_err();
        assert!(error.contains("围栏 JSON 无法解析"));
    }
}
