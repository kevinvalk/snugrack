//! The device mesh in its own frame: X width (left to right seen from the front), Y up, Z towards
//! the viewer, millimeters. The front face is at z = 0, the lowest point at y = 0, centered on x.

use glam::{DAffine3, DMat3, DVec3};
use serde::{Deserialize, Serialize};
use tsify::Tsify;

use crate::csg::Solid;
use crate::poly::Contour;

pub type Tri = [DVec3; 3];

/// A direction in the imported model's own axes.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize, Tsify)]
#[serde(rename_all = "camelCase")]
pub enum Direction {
    PosX,
    NegX,
    PosY,
    NegY,
    PosZ,
    NegZ,
}

impl Direction {
    fn vector(self) -> DVec3 {
        match self {
            Direction::PosX => DVec3::X,
            Direction::NegX => DVec3::NEG_X,
            Direction::PosY => DVec3::Y,
            Direction::NegY => DVec3::NEG_Y,
            Direction::PosZ => DVec3::Z,
            Direction::NegZ => DVec3::NEG_Z,
        }
    }
}

/// Which way the device's front and top face in the imported model. Between them they pick any of
/// the 24 ways a box can sit.
#[derive(Clone, Copy, Debug, PartialEq, Serialize, Deserialize, Tsify)]
#[serde(rename_all = "camelCase")]
pub struct Orientation {
    pub front: Direction,
    pub up: Direction,
    /// For models that come in the wrong size: 1 leaves them as they are.
    pub scale: f64,
}

/// Bring a flat `[x, y, z, ...]` triangle list in meters (glTF's unit) into the device frame, in
/// millimeters. Also returns the transform from the source model, so a viewer can place it.
pub fn normalize(positions: &[f32], o: &Orientation) -> (Vec<Tri>, DAffine3) {
    let front = o.front.vector();
    // A top along the front can't be; take the model's own up, or its Z if that's the front.
    let up = match o.up.vector() {
        u if u.dot(front) == 0.0 => u,
        _ if front.y == 0.0 => DVec3::Y,
        _ => DVec3::Z,
    };
    // The rows are the device's axes in model coordinates: across, up and front.
    let turn = DMat3::from_cols(up.cross(front), up, front).transpose();
    let pre = DAffine3::from_mat3(turn * 1000.0 * o.scale);
    let points: Vec<DVec3> = positions
        .as_chunks::<3>()
        .0
        .iter()
        .map(|&[x, y, z]| pre.transform_point3(DVec3::new(x.into(), y.into(), z.into())))
        .collect();
    let (min, max) = bounds(points.iter().copied());
    let shift = DVec3::new(-(min.x + max.x) / 2.0, -min.y, -max.z);
    let tris = points.as_chunks::<3>().0.iter().map(|t| t.map(|p| p + shift)).collect();
    (tris, DAffine3::from_translation(shift) * pre)
}

/// A device lofted through convex outlines with as many points each, `(z, outline)` front first:
/// a box, or a stand-in for a model in tests.
pub fn loft(rings: &[(f64, Contour)]) -> Vec<Tri> {
    let rings = rings.iter().map(|(z, c)| c.iter().map(|&[x, y]| [x, y, *z]).collect()).collect();
    let Solid::Mesh { vertices, triangles } = Solid::loft(rings) else { unreachable!("a loft is a mesh") };
    triangles.iter().map(|t| t.map(|i| DVec3::from(vertices[i as usize]))).collect()
}

pub fn bounds(points: impl Iterator<Item = DVec3>) -> (DVec3, DVec3) {
    points.fold((DVec3::INFINITY, DVec3::NEG_INFINITY), |(lo, hi), p| (lo.min(p), hi.max(p)))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn front_and_top_end_up_at_pos_z_and_pos_y() {
        use Direction::*;
        for front in [PosX, NegX, PosY, NegY, PosZ, NegZ] {
            for up in [PosX, NegX, PosY, NegY, PosZ, NegZ].into_iter().filter(|u| u.vector().dot(front.vector()) == 0.0)
            {
                // A corner with one arm along the front and one along the top.
                let [f, u] = [front.vector() * 0.1, up.vector() * 0.05];
                let positions = [0.0, 0.0, 0.0, u.x, u.y, u.z, f.x, f.y, f.z].map(|v| v as f32);
                let (tris, _) = normalize(&positions, &Orientation { front, up, scale: 1.0 });
                let t = tris[0];
                // Inputs are f32, so allow for its precision.
                assert!(t[2].z.abs() < 1e-4 && (t[0].z + 100.0).abs() < 1e-4, "{front:?} {up:?}: {t:?}");
                assert!((t[1].y - 50.0).abs() < 1e-4 && t[0].y.abs() < 1e-4, "{front:?} {up:?}: {t:?}");
            }
        }
    }
}
