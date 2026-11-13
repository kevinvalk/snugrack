//! Smooth solids through a stack of nested outlines, for the faceplate nest: the surface passes
//! through each outline at its own height and slopes straight between neighbors, so it follows
//! the device instead of stepping by a print layer height.
//!
//! We build it a band at a time. A band is the solid under two neighboring outlines: flat at the
//! inner one's height inside it, sloping to the outer one's height at its edge, with walls straight
//! down from there to a flat base. Each band only ever slopes between its own two heights, however
//! the outlines touch or share edges; the union of the bands is the whole surface, since at every
//! point the band it lies in reaches furthest. A band only matters where its outlines differ, so we
//! keep it to a strip around that, which is little of a device's outline; the smallest outline gets
//! a flat piece of its own.

use spade::handles::FixedVertexHandle;
use spade::{ConstrainedDelaunayTriangulation, Point2, Triangulation};

use crate::csg::Solid;
use crate::poly::{self, Contour, Shapes};

/// How far a band reaches past where its outlines differ, into the flat inside the inner one.
const NEAR: f64 = 0.5;

/// `layers` are nested outlines, smallest first, each with the height of its surface. `base` is
/// the height of the flat side, beyond every layer's height.
pub fn terrain(layers: &[(f64, Vec<Contour>)], base: f64) -> Solid {
    let Some(smallest) = layers.first() else { return Solid::union(Vec::new()) };
    let mut parts = vec![band(std::slice::from_ref(smallest), base)];
    for pair in layers.windows(2) {
        let [(h_in, inner), (h_out, outer)] = pair else { unreachable!() };
        let (inner, outer) = (vec![inner.clone()], vec![outer.clone()]);
        let differ = poly::difference(&outer, &inner);
        if differ.is_empty() {
            continue;
        }
        let strip = poly::intersection(&outer, &poly::offset(&differ, NEAR));
        let flat = poly::intersection(&inner, &strip);
        parts.push(band(&[(*h_in, flat.concat()), (*h_out, strip.concat())], base));
    }
    Solid::union(parts)
}

/// Where a terrain's surface reaches `z` or higher, seen from above.
pub fn section(terrain: &Solid, z: f64) -> Shapes {
    let mut faces = Vec::new();
    let mut gather = vec![terrain];
    while let Some(s) = gather.pop() {
        match s {
            Solid::Mesh { vertices, triangles } => {
                faces.extend(triangles.iter().map(|t| t.map(|i| vertices[i as usize])))
            }
            Solid::Union { children } => gather.extend(children),
            Solid::Label { solid, .. } => gather.push(solid),
            _ => {}
        }
    }
    poly::union_polygons(faces.iter().map(|t| {
        let mut out = Vec::new();
        for k in 0..3 {
            let (a, b) = (t[k], t[(k + 1) % 3]);
            if a[2] >= z {
                out.push([a[0], a[1]]);
            }
            if (a[2] >= z) != (b[2] >= z) {
                let f = (z - a[2]) / (b[2] - a[2]);
                out.push([a[0] + f * (b[0] - a[0]), a[1] + f * (b[1] - a[1])]);
            }
        }
        out
    }))
}

/// One band: `layers` is an outline, or an outline and the next one out.
fn band(layers: &[(f64, Vec<Contour>)], base: f64) -> Solid {
    let outer = &layers[layers.len() - 1].1;
    // Every outline vertex first. The largest outline is then a hard constraint, so no face
    // straddles its edge; spade splits it wherever another vertex lies on it. The others only add
    // fidelity, so where one would cross a constraint we skip it and the surface slopes across.
    let mut cdt = ConstrainedDelaunayTriangulation::<Point2<f64>>::new();
    let handles: Vec<Vec<Vec<FixedVertexHandle>>> = layers
        .iter()
        .map(|(_, l)| {
            l.iter().map(|c| c.iter().filter_map(|p| cdt.insert(Point2::new(p[0], p[1])).ok()).collect()).collect()
        })
        .collect();
    let edges = |contour: &[FixedVertexHandle]| {
        let n = contour.len();
        (0..n).map(move |i| (contour[i], contour[(i + 1) % n])).filter(|(a, b)| a != b).collect::<Vec<_>>()
    };
    for (from, to) in handles.last().into_iter().flatten().flat_map(|c| edges(c)) {
        cdt.add_constraint(from, to);
    }
    for (from, to) in handles.iter().rev().skip(1).flatten().flat_map(|c| edges(c)) {
        cdt.try_add_constraint(from, to);
    }

    // The surface's height at each vertex: that of the smallest layer holding it. Layers are
    // nested, so a binary search finds it.
    let heights: Vec<f64> = cdt
        .vertices()
        .map(|v| {
            let p = [v.position().x, v.position().y];
            let first = layers.partition_point(|(_, l)| !poly::inside(l, p));
            layers[first.min(layers.len() - 1)].0
        })
        .collect();

    // Faces inside the largest outline, and the edges where they meet the outside.
    let kept: Vec<[usize; 3]> = cdt
        .inner_faces()
        .map(|f| f.vertices().map(|v| v.fix().index()))
        .filter(|[a, b, c]| {
            let p = |i: usize| cdt.vertex(FixedVertexHandle::from_index(i)).position();
            let (pa, pb, pc) = (p(*a), p(*b), p(*c));
            poly::inside(outer, [(pa.x + pb.x + pc.x) / 3.0, (pa.y + pb.y + pc.y) / 3.0])
        })
        .collect();
    let mut edge_faces = std::collections::HashMap::new();
    for [a, b, c] in &kept {
        for (from, to) in [(*a, *b), (*b, *c), (*c, *a)] {
            *edge_faces.entry((from.min(to), from.max(to))).or_insert(0) += 1;
        }
    }

    // Vertices twice over: on the surface, then on the base. Faces turn outwards: the surface
    // faces away from the base, whichever side that is.
    let n = cdt.num_vertices();
    let mut vertices: Vec<[f64; 3]> =
        cdt.vertices().zip(&heights).map(|(v, z)| [v.position().x, v.position().y, *z]).collect();
    vertices.extend(cdt.vertices().map(|v| [v.position().x, v.position().y, base]));
    let up = base < layers[0].0;
    let mut triangles = Vec::new();
    for &[a, b, c] in &kept {
        let (surface, flat) = ([a, b, c], [a + n, c + n, b + n]);
        triangles.extend(if up { [surface, flat] } else { [[a, c, b], [a + n, b + n, c + n]] });
        // Each edge on the outline, taken in the face's counterclockwise order, gets a wall.
        for (from, to) in [(a, b), (b, c), (c, a)] {
            if edge_faces[&(from.min(to), from.max(to))] == 1 {
                let wall = if up {
                    [[from, from + n, to + n], [from, to + n, to]]
                } else {
                    [[from, to + n, from + n], [from, to, to + n]]
                };
                triangles.extend(wall);
            }
        }
    }
    Solid::Mesh { vertices, triangles: triangles.into_iter().map(|t| t.map(|i| i as u32)).collect() }
}

#[cfg(test)]
mod tests {
    use super::*;

    use crate::poly::P2;

    fn square(r: f64) -> Vec<Contour> {
        poly::rect(-r, -r, r, r).concat()
    }

    /// A frustum: a 2 mm square at height 0 widening to a 4 mm square at -1, on a base at -3.
    #[test]
    fn a_band_slopes_between_its_outlines_and_closes() {
        let Solid::Mesh { vertices, triangles } = band(&[(0.0, square(1.0)), (-1.0, square(2.0))], -3.0) else {
            panic!("expected a mesh")
        };
        // Closed: every directed edge has exactly one opposite twin.
        let mut directed = std::collections::HashMap::new();
        for t in &triangles {
            for k in 0..3 {
                *directed.entry((t[k], t[(k + 1) % 3])).or_insert(0) += 1;
            }
        }
        for (&(a, b), &count) in &directed {
            assert_eq!((count, directed.get(&(b, a))), (1, Some(&1)), "edge {a}-{b}");
        }
        // Volume, by the divergence theorem: the 4x4x2 block below -1, plus the frustum on top.
        let volume: f64 = triangles
            .iter()
            .map(|t| {
                let [a, b, c] = t.map(|i| glam::DVec3::from_array(vertices[i as usize]));
                a.dot(b.cross(c)) / 6.0
            })
            .sum();
        let frustum = (4.0 + 16.0 + (4.0f64 * 16.0).sqrt()) / 3.0;
        assert!((volume - (32.0 + frustum)).abs() < 1e-9, "{volume}");
    }

    /// Outlines that share edges, as a device's straight sides make them: wherever a point lies,
    /// the surface over it stays between the heights of the two outlines either side of it.
    #[test]
    fn every_point_stays_within_its_band() {
        let rect = |x0, y0, x1, y1| poly::rect(x0, y0, x1, y1).concat();
        let layers = [
            (0.0, rect(-1.0, -1.0, 1.0, 1.0)),
            (-1.0, rect(-1.0, -1.0, 3.0, 1.0)),
            (-2.0, rect(-1.0, -1.0, 3.0, 1.5)),
            (-10.0, rect(-2.0, -2.0, 4.0, 2.0)),
        ];
        let Solid::Union { children } = terrain(&layers, -20.0) else { panic!("expected bands") };
        // The surface over a point: the highest face over it, of any band.
        let height = |p: P2| {
            let mut best = f64::NEG_INFINITY;
            for child in &children {
                let Solid::Mesh { vertices, triangles } = child else { panic!("expected a mesh") };
                for t in triangles {
                    let [a, b, c] = t.map(|i| vertices[i as usize]);
                    let d = (b[1] - c[1]) * (a[0] - c[0]) + (c[0] - b[0]) * (a[1] - c[1]);
                    if d.abs() < 1e-12 {
                        continue;
                    }
                    let l1 = ((b[1] - c[1]) * (p[0] - c[0]) + (c[0] - b[0]) * (p[1] - c[1])) / d;
                    let l2 = ((c[1] - a[1]) * (p[0] - c[0]) + (a[0] - c[0]) * (p[1] - c[1])) / d;
                    if l1 >= -1e-9 && l2 >= -1e-9 && 1.0 - l1 - l2 >= -1e-9 {
                        best = best.max(l1 * a[2] + l2 * b[2] + (1.0 - l1 - l2) * c[2]);
                    }
                }
            }
            best
        };
        for i in 0..60 {
            for j in 0..40 {
                let p = [-2.0 + 6.0 * (i as f64 + 0.5) / 60.0, -2.0 + 4.0 * (j as f64 + 0.5) / 40.0];
                let k = layers.iter().position(|(_, l)| poly::inside(l, p)).unwrap();
                let (lo, hi) = (layers[k].0, if k == 0 { layers[0].0 } else { layers[k - 1].0 });
                let h = height(p);
                assert!(h >= lo - 1e-9 && h <= hi + 1e-9, "{p:?} in band {k}: {h}, not within {lo}..{hi}");
            }
        }
    }
}
