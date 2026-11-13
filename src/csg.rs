//! A tiny CSG tree. The core decides what to build; manifold-3d in the web worker turns the tree
//! into a watertight mesh.

use serde::Serialize;
use tsify::Tsify;

use i_overlay::i_shape::float::area::Area;
use spade::{ConstrainedDelaunayTriangulation, Point2, Triangulation};

use crate::poly::{self, Contour, P2};

#[derive(Clone, Copy, Debug, Serialize, PartialEq, Eq, Tsify)]
#[serde(rename_all = "lowercase")]
pub enum Axis {
    X,
    Y,
    Z,
}

/// A face a [`Solid::through`] opens through.
pub type Face<'a> = (&'a dyn Fn(f64) -> Option<f64>, f64);

/// Where a point of a cut through a wall goes, from how deep into the wall, how far across and its z.
pub type Place<'a> = dyn Fn(f64, f64, f64) -> [f64; 3] + 'a;

#[derive(Clone, Debug, Serialize, Tsify)]
#[serde(tag = "op", rename_all = "camelCase")]
pub enum Solid {
    /// Prism along `axis` from `lo` to `hi`. Profile coordinates are the two other axes in
    /// cyclic order: X -> (y, z), Y -> (z, x), Z -> (x, y). Contours are filled with the
    /// non-zero rule, so overlapping contours union and CW contours inside CCW ones are holes.
    Extrude {
        axis: Axis,
        contours: Vec<Contour>,
        lo: f64,
        hi: f64,
    },
    /// A closed, outward-facing triangle mesh.
    Mesh {
        vertices: Vec<[f64; 3]>,
        triangles: Vec<[u32; 3]>,
    },
    Union {
        children: Vec<Solid>,
    },
    Difference {
        base: Box<Solid>,
        cut: Vec<Solid>,
    },
    /// Where its geometry comes from: a name, and the file and line that made it, so a face of the
    /// finished part can be traced back to the code. Labels don't nest; an outer one would take over.
    Label {
        name: String,
        at: String,
        solid: Box<Solid>,
    },
}

impl Solid {
    /// Prism along Z with an (x, y) profile.
    pub fn prism_z(contours: Vec<Contour>, z0: f64, z1: f64) -> Solid {
        Solid::prism(Axis::Z, contours, z0, z1)
    }

    /// Prism along X with a (y, z) profile.
    pub fn prism_x(contours: Vec<Contour>, x0: f64, x1: f64) -> Solid {
        Solid::prism(Axis::X, contours, x0, x1)
    }

    /// No outlines make nothing: an empty union, which the kernel handles, rather than an empty
    /// cross-section, which it doesn't.
    fn prism(axis: Axis, contours: Vec<Contour>, a: f64, b: f64) -> Solid {
        if contours.is_empty() {
            return Solid::union(Vec::new());
        }
        Solid::Extrude { axis, contours, lo: a.min(b), hi: a.max(b) }
    }

    /// A solid through `rings`, flat outlines with as many points each, every point joined straight
    /// to the same one on the next ring: slopes whose outlines keep their rounded corners, or a
    /// device built from its outlines.
    pub fn loft(rings: Vec<Vec<[f64; 3]>>) -> Solid {
        let (n, last) = (rings[0].len(), rings.len() - 1);
        assert!(rings.iter().all(|r| r.len() == n), "a loft's rings have different numbers of points");
        let at = |r: usize, i: usize| (r * n + i % n) as u32;
        let mut triangles = Vec::new();
        for r in 0..last {
            for i in 0..n {
                let (a, b, c, d) = (at(r, i), at(r, i + 1), at(r + 1, i), at(r + 1, i + 1));
                triangles.extend([[a, b, d], [a, d, c]]);
            }
        }
        for [a, b, c] in cap(&rings[0]) {
            triangles.extend([[at(0, a), at(0, c), at(0, b)], [at(last, a), at(last, b), at(last, c)]]);
        }
        let vertices: Vec<[f64; 3]> = rings.into_iter().flatten().collect();
        // Faces out, whichever way the rings run.
        let point = |i: u32| glam::DVec3::from(vertices[i as usize]);
        let volume: f64 = triangles.iter().map(|&[a, b, c]| point(a).dot(point(b).cross(point(c)))).sum();
        if volume < 0.0 {
            triangles.iter_mut().for_each(|t| t.swap(1, 2));
        }
        Solid::Mesh { vertices, triangles }
    }

    /// A cut along `axis`, X or Y, from `lo` to `hi`, of outlines in (across, z), widened across
    /// where it opens through `faces`, so the corners it leaves in the layer plane are rounded by
    /// `r`, or half the wall where that's thinner. A face is where it lies along the axis at each
    /// point across, if it runs flat enough there, and the wall's thickness behind it, signed.
    /// Only across: the outlines' tops and bottoms, bridges as the part prints, stay put.
    pub fn through(axis: Axis, contours: Vec<Contour>, (lo, hi): (f64, f64), faces: &[Face], r: f64) -> Solid {
        // Along Y a prism's profile is (z, x), so we swap the outlines' (across, z).
        let swap = |c: &Contour| c.iter().map(|p| [p[1], p[0]]).collect();
        let profile = if axis == Axis::Y { contours.iter().map(swap).collect() } else { contours.clone() };
        let place = |v: f64, u: f64, z: f64| if axis == Axis::X { [v, u, z] } else { [u, v, z] };
        let rims = rims(&contours, (lo, hi), faces, r, &place);
        Solid::union([vec![Solid::prism(axis, profile, lo, hi)], rims].concat())
    }

    /// [`Solid::through`] a wall that bends: `place` takes a point from how deep into the wall, how
    /// far across and its z to where it goes, keeping each outline flat at every depth.
    pub fn bent(contours: &[Contour], (lo, hi): (f64, f64), faces: &[Face], r: f64, place: &Place<'_>) -> Solid {
        let ring = |t: f64, c: &Contour| c.iter().map(|p| place(t, p[0], p[1])).collect();
        let body = contours.iter().map(|c| Solid::loft(vec![ring(lo, c), ring(hi, c)])).collect();
        Solid::union([body, rims(contours, (lo, hi), faces, r, place)].concat())
    }

    pub fn union(children: Vec<Solid>) -> Solid {
        Solid::Union { children }
    }

    /// Label this geometry with `name` and the place it's labeled at.
    #[track_caller]
    pub fn labeled(self, name: &str) -> Solid {
        let at = std::panic::Location::caller();
        Solid::Label { name: name.into(), at: format!("{}:{}", at.file(), at.line()), solid: Box::new(self) }
    }

    pub fn difference(base: Solid, cut: Vec<Solid>) -> Solid {
        if cut.is_empty() { base } else { Solid::Difference { base: Box::new(base), cut } }
    }

    /// The pieces of the solids labeled `name`, for tests to find what they check.
    #[cfg(test)]
    pub fn find(&self, name: &str) -> Vec<&Solid> {
        match self {
            Solid::Label { name: n, solid, .. } if n == name => solid.pieces(),
            Solid::Label { solid, .. } => solid.find(name),
            Solid::Union { children } => children.iter().flat_map(|c| c.find(name)).collect(),
            Solid::Difference { base, cut } => {
                base.find(name).into_iter().chain(cut.iter().flat_map(|c| c.find(name))).collect()
            }
            _ => Vec::new(),
        }
    }

    /// Prisms and meshes, through unions.
    #[cfg(test)]
    fn pieces(&self) -> Vec<&Solid> {
        match self {
            Solid::Union { children } => children.iter().flat_map(Solid::pieces).collect(),
            _ => vec![self],
        }
    }

    /// The prisms labeled `name`: from, to, and their outlines.
    #[cfg(test)]
    pub fn prisms(&self, name: &str) -> Vec<(f64, f64, &[Contour])> {
        let found = self.find(name).into_iter();
        found
            .filter_map(|s| match s {
                Solid::Extrude { lo, hi, contours, .. } => Some((*lo, *hi, contours.as_slice())),
                _ => None,
            })
            .collect()
    }

    /// The outlines of the prisms labeled `name`.
    #[cfg(test)]
    pub fn profiles(&self, name: &str) -> Vec<Contour> {
        self.prisms(name).into_iter().flat_map(|(.., c)| c.to_vec()).collect()
    }
}

/// What widens a cut through a wall where it opens through each of `faces` (see [`Solid::through`]).
fn rims(contours: &[Contour], (lo, hi): (f64, f64), faces: &[Face], r: f64, place: &Place<'_>) -> Vec<Solid> {
    let mut parts = Vec::new();
    for (face, behind) in faces {
        let (r, into) = (r.min(behind.abs() / 2.0), behind.signum());
        // Into the wall: a quarter circle, which meets the cut's sides along them, not at an
        // angle a nozzle would slow down for, then on in past them.
        let steps = [vec![[-2.0 * r, r]], poly::arc([r, r], r, 180.0, 90.0), vec![[1.5 * r, -0.4 * r]]].concat();
        for c in contours {
            // Each point moves across the way its corner faces, where the face runs flat, and
            // as far along as the face lies halfway across that move, where the face slopes.
            let turn = if Area::area(c.as_slice()) < 0.0 { -1.0 } else { 1.0 };
            let n = c.len();
            let across = |a: P2, b: P2| turn * (b[1] - a[1]) / (b[0] - a[0]).hypot(b[1] - a[1]).max(1e-12);
            let points: Vec<(P2, f64, Option<f64>)> = (0..n)
                .map(|i| {
                    let (a, p, b) = (c[(i + n - 1) % n], c[i], c[(i + 1) % n]);
                    let side = across(a, p) + across(p, b);
                    let at =
                        face(p[0] + side.signum() * r / 2.0).filter(|at| side.abs() > 1e-9 && (lo..=hi).contains(at));
                    (p, side.signum(), at)
                })
                .collect();
            let flat: Vec<f64> = points.iter().filter_map(|q| q.2).collect();
            if flat.is_empty() {
                continue;
            }
            let middle = flat.iter().sum::<f64>() / flat.len() as f64;
            let ring = |&[t, w]: &P2| {
                let point = |&(p, side, at): &(P2, f64, Option<f64>)| {
                    let (u, at) = (p[0] + at.map_or(0.0, |_| side * w), at.unwrap_or(middle));
                    // Out of the wall no further than the cut reaches.
                    let t = t.max(-(if into > 0.0 { at - lo } else { hi - at }));
                    place(at + into * t, u, p[1])
                };
                points.iter().map(point).collect()
            };
            parts.push(Solid::loft(steps.iter().map(ring).collect()));
        }
    }
    parts
}

/// A flat ring's triangles, by index into it, any shape it has. We look at it along the axis it
/// faces most.
fn cap(ring: &[[f64; 3]]) -> Vec<[usize; 3]> {
    let normal = (0..ring.len()).fold(glam::DVec3::ZERO, |n, i| {
        n + glam::DVec3::from(ring[i]).cross(glam::DVec3::from(ring[(i + 1) % ring.len()]))
    });
    let axis = normal.abs().max_position();
    let flat: Vec<P2> = ring.iter().map(|p| [p[(axis + 1) % 3], p[(axis + 2) % 3]]).collect();
    let mut cdt = ConstrainedDelaunayTriangulation::<Point2<f64>>::new();
    let handles: Vec<_> = flat.iter().filter_map(|p| cdt.insert(Point2::new(p[0], p[1])).ok()).collect();
    for i in 0..handles.len() {
        cdt.add_constraint(handles[i], handles[(i + 1) % handles.len()]);
    }
    let winding = Area::area(flat.as_slice()).signum();
    cdt.inner_faces()
        .map(|f| f.vertices().map(|v| v.fix().index()))
        .filter(|&[a, b, c]| {
            // Inside the ring, and turning its way.
            let [p, q, r] = [a, b, c].map(|i| flat[i]);
            poly::inside(std::slice::from_ref(&flat), [(p[0] + q[0] + r[0]) / 3.0, (p[1] + q[1] + r[1]) / 3.0])
        })
        .map(|[a, b, c]| {
            let [p, q, r] = [a, b, c].map(|i| flat[i]);
            let turn = (q[0] - p[0]) * (r[1] - p[1]) - (q[1] - p[1]) * (r[0] - p[0]);
            if turn * winding < 0.0 { [a, c, b] } else { [a, b, c] }
        })
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;

    /// A loft joins its rings point for point: rings with different numbers of points are a bug,
    /// which would leave a solid with a piece missing.
    #[test]
    #[should_panic(expected = "different numbers of points")]
    fn a_loft_needs_rings_of_one_size() {
        let ring = |z: f64, n: usize| (0..n).map(|i| [(i as f64).cos(), (i as f64).sin(), z]).collect();
        Solid::loft(vec![ring(0.0, 3), ring(1.0, 4)]);
    }

    /// A cut through a wall from y = 0 to 2 widens across, by its rounding, where it opens through
    /// each face, and not up or down: in every layer it leaves round corners, which meet its sides
    /// along them, and adds no overhang.
    #[test]
    fn a_cut_through_a_wall_widens_across_at_its_faces() {
        let (near, far) = (|_: f64| Some(0.0), |_: f64| Some(2.0));
        let faces: [Face; 2] = [(&near, 2.0), (&far, -2.0)];
        let cut = Solid::through(Axis::Y, poly::rect(0.0, 0.0, 10.0, 5.0).concat(), (-1.0, 3.0), &faces, 0.5);
        let rims: Vec<&Solid> = cut.pieces().into_iter().filter(|p| matches!(p, Solid::Mesh { .. })).collect();
        assert_eq!(rims.len(), 2);
        let near = |a: f64, b: f64| (a - b).abs() < 1e-9;
        let has = |xs: &[f64], x: f64| xs.iter().any(|&v| near(v, x));
        for rim in rims {
            let Solid::Mesh { vertices, .. } = rim else { unreachable!() };
            let face = if vertices.iter().any(|v| v[1] < 0.0) { 0.0 } else { 2.0 };
            let xs: Vec<f64> = vertices.iter().filter(|v| near(v[1], face)).map(|v| v[0]).collect();
            assert!(has(&xs, -0.5) && has(&xs, 10.5), "{xs:?} at the face");
            let side = face + if face == 0.0 { 0.5 } else { -0.5 };
            let sides: Vec<f64> = vertices.iter().filter(|v| near(v[1], side)).map(|v| v[0]).collect();
            assert!(has(&sides, 0.0) && has(&sides, 10.0), "{sides:?} where the rounding meets the sides");
            assert!(vertices.iter().all(|v| v[2] == 0.0 || v[2] == 5.0), "the cut moves up or down");
        }
    }
}
