//! Rounded edges, after the fact: the parts are built sharp, and every edge a printer draws as a
//! corner of a layer, any that doesn't lie flat, is rounded off in the layer plane. Every part
//! prints in layers across z, so that's the corner each layer has there. We only take material
//! away, from corners that stand out: a device's room, a slit or a fillet we designed never
//! changes. And only as far as the faces either side run flat to where the round meets them: a round
//! that met a curve, or ran off a narrow face, would leave a crease of its own. Where they don't run
//! far enough, it's smaller, down to a fifth; what stays sharp is somewhere the design leaves no
//! room for a round.

use glam::{DVec2, DVec3};

use crate::csg::Solid;
use crate::poly::Contour;

/// How much each corner is rounded.
pub const RADIUS: f64 = 0.5;
/// Faces meeting at less than this many degrees are one curve, drawn in facets: nothing to round.
const SHARP: f64 = 30.0;
/// An edge rising less than this, per unit along it, lies flat: it's a layer's outline, not a corner of it. A face
/// facing up or down less than this, per unit of its normal, lies flat: it has no corner in a layer.
const RISE: f64 = 0.2;
const STEEP: f64 = 0.3;
/// Faces whose normals agree this closely lie in one plane, and a point this near one lies on it: a
/// mesh in float32 is no closer than that, a hundred millimeters out.
const FLAT: f64 = 1.0 - 1e-5;
const NEAR: f64 = 1e-3;
/// What share of the radius to try, biggest first, where the faces don't reach far enough for all of it.
const SIZES: [f64; 4] = [1.0, 0.6, 0.35, 0.2];
/// A point sharper than this, in degrees, is left: rounding would take most of it.
const POINT: f64 = 37.0;
/// Each cutter reaches this far past the edge's ends, and out past the faces, so the kernel never
/// sees it end exactly on them.
const PAST: f64 = 0.02;

/// What to take away from a part, a mesh with its triangles counterclockwise seen from outside, to
/// round its edges: along each, the sliver between its corner and a quarter circle in the layer.
pub fn cutters(vertices: &[[f64; 3]], triangles: &[[u32; 3]], r: f64) -> Solid {
    // A kernel's mesh can hold a point twice, once for each shape whose faces meet there: one, each.
    let mut first = std::collections::HashMap::new();
    let same: Vec<u32> = vertices
        .iter()
        .enumerate()
        .map(|(i, v)| *first.entry(v.map(|c| (c * 1e4).round() as i64)).or_insert(i as u32))
        .collect();
    let triangles: Vec<[u32; 3]> = triangles.iter().map(|t| t.map(|i| same[i as usize])).collect();
    let at = |i: u32| DVec3::from(vertices[i as usize]);
    let normals: Vec<DVec3> =
        triangles.iter().map(|&[a, b, c]| (at(b) - at(a)).cross(at(c) - at(a)).normalize_or_zero()).collect();
    // Each directed edge, with its face and that face's third corner.
    let mut edges = std::collections::HashMap::new();
    for (f, &[a, b, c]) in triangles.iter().enumerate() {
        for (u, v, w) in [(a, b, c), (b, c, a), (c, a, b)] {
            edges.insert((u, v), (f, w));
        }
    }
    // Faces that lie in one plane, edge to edge, as one: its triangles, by face.
    let mut flat: Vec<usize> = (0..triangles.len()).collect();
    fn root(flat: &mut [usize], f: usize) -> usize {
        let mut f = f;
        while flat[f] != f {
            flat[f] = flat[flat[f]];
            f = flat[f];
        }
        f
    }
    for (&(u, v), &(f, _)) in &edges {
        if let Some(&(g, _)) = edges.get(&(v, u)).filter(|&&(g, _)| normals[f].dot(normals[g]) > FLAT) {
            let (a, b) = (root(&mut flat, f), root(&mut flat, g));
            flat[a] = b;
        }
    }
    let region: Vec<usize> = (0..triangles.len()).map(|f| root(&mut flat, f)).collect();
    let mut planes: std::collections::HashMap<usize, Vec<usize>> = std::collections::HashMap::new();
    for (f, &r) in region.iter().enumerate() {
        planes.entry(r).or_default().push(f);
    }
    let plane = |f: usize| &planes[&region[f]];
    // Whether `p` lies on the flat face `f` is part of.
    let on = |f: usize, p: DVec3| {
        plane(f).iter().any(|&t| {
            let [a, b, c] = triangles[t].map(at);
            let n = normals[t];
            let inside = |x: DVec3, y: DVec3| (y - x).cross(p - x).dot(n) >= -NEAR;
            (p - a).dot(n).abs() < NEAR && inside(a, b) && inside(b, c) && inside(c, a)
        })
    };
    // Each edge to round, from its lower end up, with its faces' outward normals in the layer, in the
    // order that turns counterclockwise, and how much of the radius fits.
    let mut found = Vec::new();
    for (&(u, v), &(f, _)) in &edges {
        let Some(&(g, w)) = edges.get(&(v, u)).filter(|_| u < v) else { continue };
        let (n1, n2) = (normals[f], normals[g]);
        let (a, b) = (at(u), at(v));
        let along = b - a;
        // Sharp, standing out, and rising.
        if n1.dot(n2) > SHARP.to_radians().cos() || n1.dot(at(w) - a) > 1e-9 || along.z.abs() < RISE * along.length() {
            continue;
        }
        let (m1, m2) = (n1.truncate(), n2.truncate());
        if m1.length() < STEEP || m2.length() < STEEP {
            continue;
        }
        let (m1, m2) = (m1.normalize(), m2.normalize());
        // The biggest round both faces reach a little past, all along the edge.
        let fits = |k: f64| {
            sliver(m1, m2, k * r).is_some_and(|(_, t1, t2)| {
                // A little in from its ends, where the faces either side end too.
                [0.05, 0.5, 0.95]
                    .map(|k| a.lerp(b, k))
                    .iter()
                    .all(|&o| on(f, o + (t1 * (1.0 + PAST)).extend(0.0)) && on(g, o + (t2 * (1.0 + PAST)).extend(0.0)))
            })
        };
        let Some(k) = SIZES.into_iter().find(|&k| fits(k)) else { continue };
        let (lo, hi) = if a.z <= b.z { (u, v) } else { (v, u) };
        let (m1, m2) = if m1.perp_dot(m2) > 0.0 { (m1, m2) } else { (m2, m1) };
        found.push((lo, hi, m1, m2, k));
    }
    // Edges end to end that round alike, one after another up, make one chain: one cut, swept along
    // it, so its round runs on round the corners where it turns.
    let mut ends: std::collections::HashMap<u32, Vec<usize>> = std::collections::HashMap::new();
    for (i, &(lo, hi, ..)) in found.iter().enumerate() {
        ends.entry(lo).or_default().push(i);
        ends.entry(hi).or_default().push(i);
    }
    let alike = |i: usize, j: usize| {
        let ((.., a1, a2, ka), (.., b1, b2, kb)) = (found[i], found[j]);
        ka == kb && a1.dot(b1) > 0.999 && a2.dot(b2) > 0.999
    };
    // The one edge going on from `point`, the other way from `from`, if it rounds alike.
    let next = |point: u32, from: usize, up: bool| {
        let others = ends.get(&point).filter(|e| e.len() == 2)?;
        let j = if others[0] == from { others[1] } else { others[0] };
        let (lo, hi, ..) = found[j];
        (alike(from, j) && (if up { lo } else { hi }) == point).then_some(j)
    };
    let mut done = vec![false; found.len()];
    let mut pieces = Vec::new();
    for start in 0..found.len() {
        if done[start] {
            continue;
        }
        let mut chain = std::collections::VecDeque::from([start]);
        done[start] = true;
        while let Some(j) = next(found[*chain.back().unwrap()].1, *chain.back().unwrap(), true).filter(|&j| !done[j]) {
            done[j] = true;
            chain.push_back(j);
        }
        while let Some(j) = next(found[chain[0]].0, chain[0], false).filter(|&j| !done[j]) {
            done[j] = true;
            chain.push_front(j);
        }
        let (_, _, m1, m2, k) = found[chain[0]];
        let Some((profile, ..)) = sliver(m1, m2, k * r) else { continue };
        let mut points: Vec<DVec3> = chain.iter().map(|&i| at(found[i].0)).collect();
        points.push(at(found[*chain.back().unwrap()].1));
        let n = points.len();
        let (first, last) = ((points[1] - points[0]).normalize(), (points[n - 1] - points[n - 2]).normalize());
        points[0] -= first * PAST;
        points[n - 1] += last * PAST;
        let ring = |o: DVec3| profile.iter().map(|p| [o.x + p[0], o.y + p[1], o.z]).collect::<Vec<_>>();
        pieces.push(Solid::loft(points.into_iter().map(ring).collect()));
    }
    Solid::union(pieces).labeled("rounded edges")
}

/// In the layer plane, about a corner at the origin whose faces face `m1` and `m2` outward: what
/// lies between the corner and the arc of radius `r` touching both faces, reaching a little out past
/// them, and where the arc meets each face. None for a point too sharp to round.
fn sliver(m1: DVec2, m2: DVec2, r: f64) -> Option<(Contour, DVec2, DVec2)> {
    // The faces turn by `turn` at the corner; the material's angle there is 180 degrees less that.
    let turn = m1.angle_to(m2);
    if 180.0 - turn.abs().to_degrees() < POINT {
        return None;
    }
    let out = (m1 + m2).normalize();
    // The arc's center lies along the bisector into the material, where it's `r` from both faces.
    let center = -out * r / (turn / 2.0).cos();
    let steps = ((turn.abs() / (std::f64::consts::PI / 16.0)).ceil() as usize).max(2);
    let arc = (0..=steps).map(|i| center + DVec2::from_angle(m1.to_angle() + turn * i as f64 / steps as f64) * r);
    let mut ring: Contour = arc.map(|p| p.to_array()).collect();
    let (first, last) = (DVec2::from(ring[0]), DVec2::from(ring[steps]));
    ring.extend([last + m2 * PAST, out * (r + PAST), first + m1 * PAST].map(|p| p.to_array()));
    Some((ring, first, last))
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::poly;

    /// A box's vertical edges are each rounded, by a sliver as tall as the edge; its top and bottom
    /// edges lie flat, across the layers, and stay.
    #[test]
    fn a_box_rounds_its_upright_edges() {
        let Solid::Mesh { vertices, triangles } = Solid::loft(vec![ring(0.0), ring(10.0)]) else { panic!() };
        let cut = cutters(&vertices, &triangles, RADIUS);
        let Solid::Label { solid, .. } = cut else { panic!("a labeled union") };
        let Solid::Union { children } = *solid else { panic!("a union") };
        assert_eq!(children.len(), 4);
        for c in &children {
            let Solid::Mesh { vertices, .. } = c else { panic!("a mesh") };
            let (lo, hi) = vertices.iter().fold(([f64::MAX; 3], [f64::MIN; 3]), |(lo, hi), v| {
                (std::array::from_fn(|k| lo[k].min(v[k])), std::array::from_fn(|k| hi[k].max(v[k])))
            });
            // From just below to just above, and only round its corner.
            assert!(lo[2] < 0.0 && hi[2] > 10.0 && hi[0] - lo[0] < 2.0 * RADIUS, "{lo:?} {hi:?}");
        }
    }

    /// The sliver at a square corner is what a quarter circle leaves of a square of its radius.
    #[test]
    fn the_sliver_is_what_the_arc_leaves() {
        let (s, ..) = sliver(DVec2::X, DVec2::Y, 1.0).unwrap();
        let inside = |p: [f64; 2]| poly::inside(std::slice::from_ref(&s), p);
        // Near the corner it's cut, past the arc it isn't.
        assert!(inside([0.0, 0.0]) && inside([-0.1, -0.1]) && !inside([-0.4, -0.4]));
        assert!(sliver(DVec2::X, DVec2::from_angle(170f64.to_radians()), 1.0).is_none());
    }

    /// A box that rises, then leans over at 45 degrees: each of its upright edges and the leaning one
    /// it runs on into round alike, so each corner is one cut, swept round where it turns.
    #[test]
    fn an_edge_that_turns_is_one_cut() {
        let lean = |z: f64, x: f64| ring(z).into_iter().map(|p| [p[0] + x, p[1], p[2]]).collect();
        let Solid::Mesh { vertices, triangles } = Solid::loft(vec![ring(0.0), ring(10.0), lean(20.0, 10.0)]) else {
            panic!()
        };
        let Solid::Label { solid, .. } = cutters(&vertices, &triangles, RADIUS) else { panic!() };
        let Solid::Union { children } = *solid else { panic!() };
        assert_eq!(children.len(), 4);
    }

    /// A fin too thin for the whole radius is rounded less, as far as its faces reach.
    #[test]
    fn a_thin_fin_gets_a_smaller_round() {
        let fin = |z: f64| [[0.0, 0.0], [10.0, 0.0], [10.0, 0.3], [0.0, 0.3]].map(|p| [p[0], p[1], z]).to_vec();
        let Solid::Mesh { vertices, triangles } = Solid::loft(vec![fin(0.0), fin(10.0)]) else { panic!() };
        let Solid::Label { solid, .. } = cutters(&vertices, &triangles, RADIUS) else { panic!() };
        let Solid::Union { children } = *solid else { panic!() };
        assert_eq!(children.len(), 4);
        for c in &children {
            let Solid::Mesh { vertices, .. } = c else { panic!() };
            let ys = vertices.iter().map(|v| v[1]);
            let span = ys.clone().fold(f64::MIN, f64::max) - ys.fold(f64::MAX, f64::min);
            assert!(span < RADIUS, "{span}: as round as a wide face gets");
        }
    }

    /// A box 10 x 6, from z 0 to 10.
    fn ring(z: f64) -> Vec<[f64; 3]> {
        [[0.0, 0.0], [10.0, 0.0], [10.0, 6.0], [0.0, 6.0]].iter().map(|p| [p[0], p[1], z]).collect()
    }
}
