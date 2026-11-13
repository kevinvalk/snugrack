//! What the device looks like end-on, depth by depth. The faceplate nest is a stack of these
//! silhouettes: when the device slides through a plane, everything ahead of it passes that plane, so
//! the plane must clear the silhouette of everything ahead. The cage's cavity and struts take the
//! whole device's, and that of what stands out behind its back face.
//!
//! All of this is clip, project and union, so the device mesh need not be watertight.

use glam::DVec3;

use crate::mesh::Tri;
use crate::poly::{self, Contour, Shapes};

/// Entry `i` is the projection onto XY of everything within `depths[i]` of the front face
/// (z = 0), holes filled and tidied. `depths` must increase.
pub fn front_silhouettes(tris: &[Tri], depths: &[f64]) -> Vec<Shapes> {
    // Faces along the depth axis project to nothing; dropping them up front saves most clipping.
    let mut live: Vec<(Tri, f64, f64)> = tris
        .iter()
        .filter(|t| (t[1] - t[0]).cross(t[2] - t[0]).z.abs() > 1e-9)
        .map(|t| (*t, t[0].z.min(t[1].z).min(t[2].z), t[0].z.max(t[1].z).max(t[2].z)))
        .collect();
    live.sort_by(|a, b| b.2.total_cmp(&a.2));

    // Sweep backwards, adding only the slab between the previous plane and this one.
    let (mut next, mut active, mut upper) = (0, Vec::new(), f64::INFINITY);
    let mut silhouette = Shapes::new();
    let mut out = Vec::with_capacity(depths.len());
    for &depth in depths {
        let plane = -depth;
        while next < live.len() && live[next].2 >= plane {
            active.push(next);
            next += 1;
        }
        let pieces: Vec<Contour> = active
            .iter()
            .map(|&i| clip(&clip(&live[i].0, plane, true), upper, false))
            .filter(|piece| piece.len() >= 3)
            .map(|piece| piece.iter().map(|p| [snap(p.x), snap(p.y)]).collect())
            .collect();
        if !pieces.is_empty() {
            let contours = silhouette.iter().flatten().cloned().chain(pieces);
            silhouette = poly::fill_holes(&poly::union_polygons(contours), 0.5);
        }
        active.retain(|&i| live[i].1 < plane);
        upper = plane;
        out.push(poly::tidy(&silhouette));
    }
    out
}

/// Like [`front_silhouettes`], with depths measured forwards from the rear face.
pub fn rear_silhouettes(tris: &[Tri], depths: &[f64]) -> Vec<Shapes> {
    let back = tris.iter().flatten().map(|p| p.z).fold(f64::INFINITY, f64::min);
    let turned: Vec<Tri> = tris.iter().map(|t| t.map(|p| DVec3::new(p.x, p.y, back - p.z))).collect();
    front_silhouettes(&turned, depths)
}

/// Facets seen almost edge-on project to slivers whose edges run a hair apart along the same line,
/// and the union crawls through all the places where they nearly cross. On a grid this fine, far
/// below anything a printer resolves, they run along one line instead.
fn snap(v: f64) -> f64 {
    (v / 1e-4).round() * 1e-4
}

/// Clip a convex polygon against `z >= c` (`keep_above`) or `z <= c`.
fn clip(polygon: &[DVec3], c: f64, keep_above: bool) -> Vec<DVec3> {
    let inside = |p: &DVec3| if keep_above { p.z >= c } else { p.z <= c };
    let mut out = Vec::with_capacity(polygon.len() + 1);
    for (i, &a) in polygon.iter().enumerate() {
        let b = polygon[(i + 1) % polygon.len()];
        if inside(&a) {
            out.push(a);
        }
        if inside(&a) != inside(&b) {
            out.push(a.lerp(b, (c - a.z) / (b.z - a.z)));
        }
    }
    out
}

#[cfg(test)]
mod tests {
    use super::*;

    fn width(s: &Shapes) -> f64 {
        let (lo, hi) = poly::bounds(s).unwrap();
        hi[0] - lo[0]
    }

    /// Facets seen almost edge-on, as a rounded edge is along its length, project to slivers a millionth
    /// of a millimeter wide along the same line. Hundreds of them took the union a second, and a real
    /// model's design twenty.
    #[test]
    fn slivers_along_a_line_union_quickly() {
        let mut seed = 7u64;
        let mut random = || {
            seed = seed.wrapping_mul(6364136223846793005).wrapping_add(1442695040888963407);
            (seed >> 11) as f64 / (1u64 << 53) as f64
        };
        let at = |t: f64, off: f64, z: f64| DVec3::new(t * 0.8 - off * 0.6, t * 0.6 + off * 0.8, z);
        let tris: Vec<Tri> = (0..300)
            .map(|_| {
                let (t, len, off) = (random() * 0.6, 0.2 + random() * 0.6, (random() - 0.5) * 1e-6);
                [at(t, off, -0.2), at(t + len, off, -0.8), at(t + len, off + random() * 1e-6, -0.5)]
            })
            .collect();
        let started = std::time::Instant::now();
        front_silhouettes(&tris, &[1.0]);
        assert!(started.elapsed().as_secs_f64() < 0.25, "took {:?}", started.elapsed());
    }

    #[test]
    fn silhouettes_grow_with_the_chamfer_from_either_end() {
        let depths = [0.0, 1.0, 2.5, 5.0, 30.0];
        // A device 100 wide, 20 tall and 60 deep whose four vertical edges are chamfered 5 mm at
        // 45 degrees, front and back: 90 wide at both faces, 100 from 5 mm in.
        let ring = |z: f64, w: f64| (z, poly::rounded_rect(-w, 0.0, w, 20.0, 0.0));
        let chamfered = crate::mesh::loft(&[ring(0.0, 45.0), ring(-5.0, 50.0), ring(-55.0, 50.0), ring(-60.0, 45.0)]);
        for sweep in [front_silhouettes, rear_silhouettes] {
            let widths: Vec<f64> = sweep(&chamfered, &depths).iter().map(width).collect();
            for (w, expected) in widths.iter().zip([90.0, 92.0, 95.0, 100.0, 100.0]) {
                assert!((w - expected).abs() < 1e-6, "{widths:?}");
            }
        }
    }
}
