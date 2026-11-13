//! 2D polygons on top of `i_overlay`: a shape is an outer contour (CCW) followed by its holes
//! (CW), and shapes are disjoint.

use i_overlay::core::fill_rule::FillRule;
use i_overlay::core::overlay_rule::OverlayRule;
use i_overlay::float::simplify::SimplifyShape;
use i_overlay::float::single::SingleFloatOverlay;
use i_overlay::i_shape::float::area::Area;
use i_overlay::mesh::float::outline::offset::OutlineOffset;
use i_overlay::mesh::float::style::{LineJoin, OutlineStyle};

pub type P2 = [f64; 2];
pub type Contour = Vec<P2>;
pub type Shapes = Vec<Vec<Contour>>;

/// Union of polygons in any winding. Projected triangles come both ways round and would cancel
/// out under the non-zero rule, so we turn them all counterclockwise first.
pub fn union_polygons(polys: impl IntoIterator<Item = Contour>) -> Shapes {
    let contours: Vec<Contour> = polys
        .into_iter()
        .filter(|c| c.area().abs() > 1e-7)
        .map(|mut c| {
            if c.area() < 0.0 {
                c.reverse();
            }
            c
        })
        .collect();
    contours.simplify_shape(FillRule::NonZero)
}

pub fn union(a: &Shapes, b: &Shapes) -> Shapes {
    a.overlay(b, OverlayRule::Union, FillRule::NonZero)
}

pub fn difference(a: &Shapes, b: &Shapes) -> Shapes {
    a.overlay(b, OverlayRule::Difference, FillRule::NonZero)
}

pub fn intersection(a: &Shapes, b: &Shapes) -> Shapes {
    a.overlay(b, OverlayRule::Intersect, FillRule::NonZero)
}

/// Outer contours only, without specks smaller than `min_area`.
pub fn fill_holes(s: &Shapes, min_area: f64) -> Shapes {
    s.iter().filter(|sh| sh[0].area() >= min_area).map(|sh| vec![sh[0].clone()]).collect()
}

/// Grow (positive) or shrink (negative) with round corners.
pub fn offset(s: &Shapes, delta: f64) -> Shapes {
    if s.is_empty() || delta == 0.0 {
        return s.clone();
    }
    s.outline(&OutlineStyle::new(delta).line_join(LineJoin::Round(step(delta.abs()))))
}

/// Concave corners rounded by `r`: grown, then shrunk back. Only ever gets bigger.
pub fn closed(s: &Shapes, r: f64) -> Shapes {
    offset(&offset(s, r), -r)
}

/// Corners rounded by `r`, inside and out: closed, then opened, and tidied of the points that
/// leaves where it had nothing to do. Parts narrower than 2 `r` go.
pub fn rounded(s: &Shapes, r: f64) -> Shapes {
    tidy(&offset(&offset(&closed(s, r), -r), r))
}

/// Where the line on which coordinate `1 - along` is `at` crosses the outlines: coordinate
/// `along` there, and how much the edge it crosses runs along the line for each unit across it.
pub fn crossings(s: &[Contour], along: usize, at: f64) -> Vec<(f64, f64)> {
    let (i, j) = (1 - along, along);
    let edges = s.iter().flat_map(|c| c.iter().zip(c.iter().cycle().skip(1)));
    let crossing = |(a, b): (&P2, &P2)| {
        ((a[i] <= at) != (b[i] <= at)).then(|| {
            let slope = (b[j] - a[j]) / (b[i] - a[i]);
            (a[j] + (at - a[i]) * slope, slope)
        })
    };
    edges.filter_map(crossing).collect()
}

pub fn bounds(s: &Shapes) -> Option<(P2, P2)> {
    let points = s.iter().flatten().flatten();
    let (lo, hi) = points.fold(([f64::INFINITY; 2], [f64::NEG_INFINITY; 2]), |(lo, hi), p| {
        ([lo[0].min(p[0]), lo[1].min(p[1])], [hi[0].max(p[0]), hi[1].max(p[1])])
    });
    lo[0].is_finite().then_some((lo, hi))
}

/// Whether a point is inside or on the outlines, by the non-zero rule.
pub fn inside(contours: &[Contour], p: P2) -> bool {
    let mut winding = 0;
    for c in contours {
        for (i, a) in c.iter().enumerate() {
            let b = c[(i + 1) % c.len()];
            let cross = (b[0] - a[0]) * (p[1] - a[1]) - (p[0] - a[0]) * (b[1] - a[1]);
            let on_segment =
                cross.abs() < 1e-9 && (p[0] - a[0]) * (p[0] - b[0]) <= 1e-12 && (p[1] - a[1]) * (p[1] - b[1]) <= 1e-12;
            if on_segment {
                return true;
            }
            if a[1] <= p[1] && b[1] > p[1] && cross > 0.0 {
                winding += 1;
            } else if a[1] > p[1] && b[1] <= p[1] && cross < 0.0 {
                winding -= 1;
            }
        }
    }
    winding != 0
}

/// How far `p` is from the nearest edge of the outlines.
pub fn distance(s: &Shapes, p: P2) -> f64 {
    let edges = s.iter().flatten().flat_map(|c| c.iter().zip(c.iter().cycle().skip(1)));
    edges.fold(f64::INFINITY, |best, (a, b)| {
        let (d, e) = ([b[0] - a[0], b[1] - a[1]], [p[0] - a[0], p[1] - a[1]]);
        let t = ((e[0] * d[0] + e[1] * d[1]) / (d[0] * d[0] + d[1] * d[1]).max(1e-18)).clamp(0.0, 1.0);
        best.min((e[0] - t * d[0]).hypot(e[1] - t * d[1]))
    })
}

/// How finely outlines follow what they stand for: curves are chords no further than this from them,
/// detail below it is dropped, and outlines snap to it. Far below what a printer resolves.
pub const RESOLUTION: f64 = 0.01;

/// The angle a chord of a circle of radius `r` may span to stay within [`RESOLUTION`] of it.
fn step(r: f64) -> f64 {
    2.0 * (1.0 - RESOLUTION / r).max(-1.0).acos()
}

/// Drop detail below [`RESOLUTION`] and snap to it. Silhouettes of real meshes have thousands of
/// points that would make every later offset slow. Snapping also lets stacked layers that share
/// an edge share it exactly; edges a hair apart would leave slivers that turn non-manifold once
/// written as a float32 STL.
pub fn tidy(s: &Shapes) -> Shapes {
    simplify(s, RESOLUTION)
}

/// Smooth, without ever coming inside the original: grown by `by`, then simplified to within
/// `by`. Wobbles smaller than that straighten out, and faceted curves become even chords.
pub fn fair(s: &Shapes, by: f64) -> Shapes {
    simplify(&offset(s, by), by)
}

/// Douglas-Peucker to within `tolerance`, then snapped to [`RESOLUTION`], so outlines that should share vertices
/// do. A closed ring has no chord to measure against, so we split each at the point farthest from its start.
fn simplify(s: &Shapes, tolerance: f64) -> Shapes {
    let q = |v: f64| (v / RESOLUTION).round() * RESOLUTION;
    let ring = |c: &Contour| -> Contour {
        let d2 = |p: &P2| (p[0] - c[0][0]).powi(2) + (p[1] - c[0][1]).powi(2);
        let far = (0..c.len()).max_by(|&i, &j| d2(&c[i]).total_cmp(&d2(&c[j]))).unwrap_or(0);
        let mut ring = c.to_vec();
        ring.push(c[0]);
        let mut out = chord(&ring[..=far], tolerance);
        out.pop();
        out.extend(chord(&ring[far..], tolerance));
        out.pop();
        out.iter().map(|p| [q(p[0]), q(p[1])]).collect()
    };
    // Snapping can fold a contour onto itself; a union with the non-zero rule cleans that up.
    s.iter().flatten().map(ring).collect::<Vec<_>>().simplify_shape(FillRule::NonZero)
}

fn chord(pts: &[P2], tolerance: f64) -> Contour {
    let (a, b) = (pts[0], pts[pts.len() - 1]);
    let (dx, dy) = (b[0] - a[0], b[1] - a[1]);
    let len = dx.hypot(dy).max(1e-12);
    let distance = |p: &P2| ((p[0] - a[0]) * dy - (p[1] - a[1]) * dx).abs() / len;
    let inner = pts.iter().enumerate().take(pts.len().saturating_sub(1)).skip(1);
    match inner.max_by(|x, y| distance(x.1).total_cmp(&distance(y.1))) {
        Some((i, p)) if distance(p) > tolerance => {
            let mut left = chord(&pts[..=i], tolerance);
            left.pop();
            left.extend(chord(&pts[i..], tolerance));
            left
        }
        _ => vec![a, b],
    }
}

pub fn translate(s: &Shapes, dx: f64, dy: f64) -> Vec<Contour> {
    s.iter().flatten().map(|c| c.iter().map(|p| [p[0] + dx, p[1] + dy]).collect()).collect()
}

// ---- outlines ----------------------------------------------------------------------------

/// A rectangle, as a shape of its own.
pub fn rect(x0: f64, y0: f64, x1: f64, y1: f64) -> Shapes {
    vec![vec![rounded_rect(x0, y0, x1, y1, 0.0)]]
}

pub fn rounded_rect(x0: f64, y0: f64, x1: f64, y1: f64, r: f64) -> Contour {
    sided_rect(x0, y0, x1, y1, r, r)
}

/// A rectangle with its left and right corners rounded by their own radius; 0 keeps them square.
pub fn sided_rect(x0: f64, y0: f64, x1: f64, y1: f64, left: f64, right: f64) -> Contour {
    let fit = |r: f64| r.min((x1 - x0) / 2.0).min((y1 - y0) / 2.0);
    let (l, r) = (fit(left), fit(right));
    let corners =
        [(x1 - r, y0 + r, -90.0, r), (x1 - r, y1 - r, 0.0, r), (x0 + l, y1 - l, 90.0, l), (x0 + l, y0 + l, 180.0, l)];
    corners
        .iter()
        .flat_map(|&(cx, cy, start, r)| if r > 0.0 { arc([cx, cy], r, start, 90.0) } else { vec![[cx, cy]] })
        .collect()
}

/// Slot with round ends between two centers on a horizontal line.
pub fn stadium(x0: f64, x1: f64, y: f64, r: f64) -> Contour {
    let mut c = arc([x1, y], r, -90.0, 180.0);
    c.extend(arc([x0, y], r, 90.0, 180.0));
    c
}

/// Round hole with a 45-degree point towards -v, for horizontal holes printed with -v up.
pub fn teardrop(u: f64, v: f64, r: f64) -> Contour {
    let mut c = arc([u, v], r, -45.0, 270.0);
    c.push([u, v - r * std::f64::consts::SQRT_2]);
    c
}

/// Each corner rounded by up to `r`, as far as half of each edge beside it allows.
pub fn fillet(c: &[P2], r: f64) -> Contour {
    let n = c.len();
    let unit = |a: P2, b: P2| {
        let l = (b[0] - a[0]).hypot(b[1] - a[1]);
        ([(b[0] - a[0]) / l, (b[1] - a[1]) / l], l)
    };
    (0..n)
        .flat_map(|i| {
            let (a, p, b) = (c[(i + n - 1) % n], c[i], c[(i + 1) % n]);
            let ((u, lu), (v, lv)) = (unit(a, p), unit(p, b));
            let turn = (u[0] * v[1] - u[1] * v[0]).atan2(u[0] * v[0] + u[1] * v[1]);
            let tan = (turn.abs() / 2.0).tan();
            if tan < 1e-6 {
                return vec![p];
            }
            let r = r.min(lu.min(lv) / 2.0 / tan);
            let side = turn.signum();
            let center = [p[0] - u[0] * r * tan - side * u[1] * r, p[1] - u[1] * r * tan + side * u[0] * r];
            let start = (p[1] - u[1] * r * tan - center[1]).atan2(p[0] - u[0] * r * tan - center[0]).to_degrees();
            arc(center, r, start, turn.to_degrees())
        })
        .collect()
}

/// Points on a circle around `c` from `start` degrees on through `sweep`, both ends included, as few
/// as keep it within [`RESOLUTION`], so arcs of one radius and sweep get as many points.
pub fn arc(c: P2, r: f64, start: f64, sweep: f64) -> Contour {
    let steps = (sweep.abs().to_radians() / step(r)).ceil().max(1.0) as usize;
    (0..=steps)
        .map(|k| {
            let a = (start + sweep * k as f64 / steps as f64).to_radians();
            [c[0] + r * a.cos(), c[1] + r * a.sin()]
        })
        .collect()
}

/// Where outlines turn more than 20 degrees with a radius under 0.4 mm, a nozzle has to slow
/// down: such corners, one point each. Turning points under a millimeter apart count as one.
#[cfg(test)]
pub fn sharp_corners(s: &[Contour]) -> Vec<P2> {
    let mut corners = Vec::new();
    for c in s {
        let n = c.len();
        let turn = |i: usize| {
            let (a, p, b) = (c[(i + n - 1) % n], c[i], c[(i + 1) % n]);
            let (u, v) = ([p[0] - a[0], p[1] - a[1]], [b[0] - p[0], b[1] - p[1]]);
            (u[0] * v[1] - u[1] * v[0]).atan2(u[0] * v[0] + u[1] * v[1])
        };
        let len = |i: usize| (c[(i + 1) % n][0] - c[i][0]).hypot(c[(i + 1) % n][1] - c[i][1]);
        let mut i = 0;
        // Start where a corner can't be running on from the point before.
        let start = (0..n).find(|&i| turn(i).abs() < 1e-3 || len((i + n - 1) % n) >= 1.0).unwrap_or(0);
        while i < n {
            let first = (start + i) % n;
            i += 1;
            if turn(first).abs() < 1e-3 {
                continue;
            }
            let (sign, mut total, mut path, mut points, mut last) = (turn(first).signum(), turn(first), 0.0, 1, first);
            while i < n && len(last) < 1.0 && turn((start + i) % n).signum() == sign {
                path += len(last);
                last = (start + i) % n;
                total += turn(last);
                points += 1;
                i += 1;
            }
            let radius = if points > 1 { path * points as f64 / (points - 1) as f64 / total.abs() } else { 0.0 };
            if total.abs() > 20f64.to_radians() && radius < 0.4 {
                corners.push(c[first]);
            }
        }
    }
    corners
}

#[cfg(test)]
mod tests {
    use super::*;

    /// A square's corners are sharp; rounded by a millimeter, they aren't.
    #[test]
    fn sharp_corners_are_the_ones_under_a_radius() {
        assert_eq!(sharp_corners(&[rounded_rect(0.0, 0.0, 10.0, 5.0, 0.0)]).len(), 4);
        assert_eq!(sharp_corners(&[rounded_rect(0.0, 0.0, 10.0, 5.0, 1.0)]), Vec::<P2>::new());
        assert_eq!(sharp_corners(&[fillet(&rounded_rect(0.0, 0.0, 10.0, 5.0, 0.0), 1.0)]), Vec::<P2>::new());
    }

    #[test]
    fn union_handles_mixed_winding() {
        let a = vec![[0.0, 0.0], [10.0, 0.0], [10.0, 10.0]];
        let b = vec![[0.0, 10.0], [10.0, 10.0], [0.0, 0.0]];
        let u = union_polygons([a, b]);
        assert_eq!(u.len(), 1);
        assert!((u.area() - 100.0).abs() < 1e-6);
    }

    #[test]
    fn offset_grows_and_shrinks() {
        let s = vec![vec![vec![[0.0, 0.0], [10.0, 0.0], [10.0, 10.0], [0.0, 10.0]]]];
        for (delta, lo, hi) in [(1.0, -1.0, 11.0), (-1.0, 1.0, 9.0)] {
            let (mn, mx) = bounds(&offset(&s, delta)).unwrap();
            assert!((mn[0] - lo).abs() < 1e-3 && (mx[0] - hi).abs() < 1e-3);
        }
    }

    /// A wobbly edge comes out straight, and nothing of the original is left outside.
    #[test]
    fn fairing_straightens_and_only_grows() {
        let mut c: Contour = (0..=100).map(|i| [i as f64 / 10.0, if i % 2 == 0 { 0.0 } else { 0.03 }]).collect();
        c.extend([[10.0, 10.0], [0.0, 10.0]]);
        let wobbly = vec![vec![c]];
        let fair = fair(&wobbly, 0.05);
        assert!(difference(&wobbly, &fair).area() < 1e-6, "it cut into the original");
        let points: usize = fair.iter().flatten().map(Vec::len).sum();
        assert!(points < 30, "{points} points");
    }
}
