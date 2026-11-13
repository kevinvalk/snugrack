//! One device's share of a rack: the nest it sits in, the collar on the faceplate, and the cage
//! that plugs into the collar and holds the rest of the device.
//!
//! Rack frame: X right seen from the front, Y up, Z towards the viewer with the panel front at
//! z = 0. Parts extend into negative z. The faceplate prints front down; its nest only grows away
//! from the front, so every layer rests on the one below, and it follows the device smoothly
//! (see `terrain`), whatever layer height it's printed at. The cage prints back down: a tube the
//! device slides into, open wherever it can be, ending in struts the device's back rests on.

use glam::{DVec2, DVec3};
use i_overlay::i_shape::float::area::Area;

use crate::csg::{Face, Solid};
use crate::mesh::{Tri, bounds};
use crate::poly::{self, Contour, P2, Shapes};
use crate::project::{Build, Fit, Project};
use crate::rack::{Front, Issue};
use crate::silhouette::{front_silhouettes, rear_silhouettes};
use crate::terrain::terrain;

/// Overlap between solids we union, so the kernel never sees faces that only touch.
pub const EPS: f64 = 0.5;
/// Below this the front opening is too close to the device's outline to hold it, so we set the
/// device back behind a lip instead of flush.
const MIN_RETENTION: f64 = 0.8;
const LIP_DEPTH: f64 = 1.2;
const LIP: f64 = 1.5;
/// We look at the device every so often through the plate, and halfway between two looks wherever
/// the surface, running straight between them, would miss the device there by more than FOLLOW,
/// down to looks FINEST apart.
const NEST_STEP: f64 = 0.5;
const FOLLOW: f64 = 0.1;
const FINEST: f64 = 0.01;
/// How finely we look for the device's front and back faces.
const FACE_STEP: f64 = 0.05;
/// How much of the device a sweep takes in at once: a union's cost grows faster than its size.
const SLAB: f64 = 1.0;
/// The nest reaches this far past the plate's front, so its cut crosses that face instead of
/// ending exactly on it.
const PAST: f64 = 0.05;
/// The cage's back is open, but for struts this wide that the device's back rests against, as near
/// SPACING apart as the width allows, and at least two. How deep they are is the user's (`Build::back`).
const STRUT: f64 = 6.0;
const SPACING: f64 = 50.0;
/// The cage's walls are openings, with solid rails this wide round its back end, BAND wide behind the
/// collar, where the walls bend when squeezed, and COLUMN wide along its edges and between openings
/// side by side, which carry the cage as it stands. With air under the device, the columns in the
/// floor stand a ridge RIDGE wide and half as tall as the air into it.
const RAIL: f64 = 6.0;
const BAND: f64 = 10.0;
const COLUMN: f64 = 15.0;
const RIDGE: f64 = 5.0;
/// The share of the device's depth, from its back, that the cage's side walls hold closed.
const CLOSED: f64 = 0.2;
/// Notches in the outlines narrower than twice this are filled, and wobbles smaller than FAIR
/// straightened (see [`clean`]); the nest is only grown by FAIR (see [`nest`]).
const NOTCH: f64 = 0.5;
const FAIR: f64 = 0.05;
/// With air under the device, it rests on ledges this wide along the sides: far enough in to reach
/// past rounded bottom edges to where the underside is flat.
const LEDGE: f64 = 15.0;
/// The longest an opening's top edge may bridge when the cage prints standing on its back.
const BRIDGE: f64 = 30.0;
/// Corners in the layer plane, which every part prints in, are rounded this much where there's room,
/// so the nozzle needn't stop dead at them.
pub(crate) const ROUND: f64 = 1.0;
/// Where a millimeter doesn't fit: round the latch's slits and holes.
const SNUG: f64 = 0.5;
/// The collar's top and bottom are as thick as the walls where the panel leaves room, and no thinner
/// than this with the latch, whose nubs they hold.
const COLLAR: f64 = 1.0;
/// Pillars this thick on the back of a blank faceplate hold the device behind it (see [`stops`]).
const STOP: f64 = 6.0;
/// The struts meet the cage's walls in fillets this big.
const FILLET: f64 = 5.0;
/// The latch: two tongues in the cage's top wall and two in its bottom, each rooted at its inner
/// end and cut free by slits SLIT wide along its outer end and the outer half of its back edge (the
/// wall past that holds it on, and gives enough to let it bend), spring in over a nub
/// on the collar's inner face as the cage goes in, and out again into a hole around the nub once
/// it's home. They reach REACH past the collar's back end: squeezing the cage's top and bottom
/// there lets it go. The cage prints back down, so the tongues bend within their layers, not
/// across them. A tongue is as thick as the wall, so it prints in the same lines, and long enough
/// that bending over the nub strains its root no more than STRAIN; its wall stands proud over it,
/// where it must, so that even bent all the way in it keeps the device's clearance.
const STRAIN: f64 = 0.015;
const SLIT: f64 = 0.8;
const REACH: f64 = 9.0;
/// The tongues run where the device's top or underside is flat to within this.
const STRAIGHT: f64 = 0.2;
/// How long a nub is, along the tongue, and what's left of the tongue beyond its hole.
const NUB: f64 = 5.0;
const EDGE: f64 = 2.0;
/// The nub's back is a ramp this many millimeters long per millimeter it stands proud, which the
/// tongue rides in on. Its front, the catch, runs CATCH millimeters along per millimeter proud: 60
/// degrees to the pull, twice the hold of 45 (about 257 N against 113 N for four tongues, at a
/// friction of 0.3). As the faceplate prints front down, that's under a millimeter of ledge at 30
/// degrees, which printers manage without support. It only locks past about 73 degrees, so pulled
/// hard enough, the catch still pushes the tongue in and lets go.
const RAMP: f64 = 2.0;
const CATCH: f64 = 0.577_350_269_189_625_8; // tan 30°

/// Everything about a device's mount that doesn't depend on where it sits in the rack. Layers
/// are in the device frame across and in the rack frame along z, since the panel front is z = 0
/// wherever the device sits.
pub struct DeviceFit {
    pub size: DVec3,
    /// How far the front opening is inset from the device's outline, sideways and vertically.
    pub retention: [f64; 2],
    /// How far the device's frontmost point sits behind the panel front: its face is flush, or
    /// behind a lip for box-shaped devices, and whatever stands proud of the face sticks out; or
    /// behind a blank faceplate, past the collar.
    pub recess: f64,
    pub front: Front,
    /// How deep its face lies behind its frontmost point (see [`face`]).
    face: f64,
    /// Behind a blank faceplate, the pillars on its back that the device's face rests against.
    stops: Vec<Contour>,
    /// The nest's outline at depths through the plate, front first: each with its z.
    nest: Vec<(f64, Shapes)>,
    /// The cavity's outline: the device's, plus clearance.
    cavity: Shapes,
    /// What it was fitted with: the project's settings, or the device's own where it has them.
    build: Build,
    fit: Fit,
    /// The cage's outside: the cavity and a wall all round.
    outer: Shapes,
    /// The struts in the cage's open back, the gussets in front of them (see [`cavity`]), the z where
    /// the cavity ends in front of them, and the z of the cage's back.
    struts: Shapes,
    gussets: Vec<Gusset>,
    z_struts: f64,
    z_back: f64,
    latch: Latch,
}

/// A gusset in front of a strut: its outline at its tallest, the wall's face it stands on, which
/// way it stands out of it, and how far.
type Gusset = (Contour, f64, f64, f64);

/// Where the latch's tongues go, and what they need, in the device frame, each pair for the bottom
/// wall and then the top.
struct Latch {
    /// The x of each tongue's free end, its root, and where its pad ends towards the middle.
    tongues: [[(f64, f64, f64); 2]; 2],
    /// The y the flat stretch they sit on is measured to, just inside the device's face, which their
    /// cuts reach past.
    levels: [f64; 2],
    /// How far the wall stands proud over them.
    pads: [f64; 2],
    /// How wide a flat stretch they need, how wide one they have, and how far behind the panel's
    /// front they reach.
    need: f64,
    straight: f64,
    deep: f64,
}

/// What the device looks like end-on, which no setting changes: its size, how deep its front and back
/// faces lie (see [`face`]), its outline, the outline of what stands out behind its back face, and
/// of all that's behind each millimeter up to RAIL in front of it.
pub struct Profile {
    pub size: DVec3,
    face: f64,
    back: f64,
    outline: Shapes,
    behind: Shapes,
    near: Vec<Shapes>,
}

impl Profile {
    pub fn new(tris: &[Tri]) -> Profile {
        let (lo, hi) = bounds(tris.iter().flatten().copied());
        let size = hi - lo;
        let back = face(tris, rear_silhouettes);
        // The outline comes from a sweep too: thin slabs at a time are far quicker than all at once.
        // What stands out behind the back face by more than a step: on a flat back, nothing.
        let depths: Vec<f64> =
            std::iter::once(back - 2.0 * FACE_STEP).chain(samples(size.z, SLAB).iter().map(|d| back + d)).collect();
        let mut silhouettes = rear_silhouettes(tris, &depths);
        let outline = silhouettes.last().cloned().unwrap_or_default();
        let near = silhouettes.drain(1..).take(RAIL as usize + 1).collect();
        let behind = silhouettes.pop().unwrap_or_default();
        Profile { size, face: face(tris, front_silhouettes), back, outline, behind, near }
    }
}

impl DeviceFit {
    /// None where there's nothing to fit: no outline, or none left inside the clearance.
    pub fn new(tris: &[Tri], profile: &Profile, p: &Project, room: Option<f64>) -> Option<DeviceFit> {
        let (size, face) = (profile.size, profile.face);
        // How far the front opening is inset from the device's outline: what holds it.
        let margin = |layers: &[(f64, Shapes)]| {
            let opening = layers.first().and_then(|l| poly::bounds(&l.1));
            opening.map_or(DVec2::ZERO, |(a, b)| (size.truncate() - DVec2::new(b[0] - a[0], b[1] - a[1])) / 2.0)
        };
        let (recess, nest, front, stops) = match room {
            // Past the collar and the band of the cage it grips, which are closed all round, so the room is all
            // opening, which the cables bend out through.
            Some(room) => {
                let recess = p.build.plate + p.build.collar_depth + BAND + room.max(0.0);
                (recess, Vec::new(), Front::Blank, stops(tris, face))
            }
            None => {
                let flush = nest(tris, p, -face, face, 0.0);
                if margin(&flush).max_element() < MIN_RETENTION {
                    (LIP_DEPTH - face, nest(tris, p, LIP_DEPTH - face, face, LIP), Front::Lip, Vec::new())
                } else {
                    (-face, flush, Front::Flush, Vec::new())
                }
            }
        };
        let retention = margin(&nest).to_array();
        let (cavity, struts, gussets, z_struts) = cavity(profile, p, -size.z - recess);
        let z_back = z_struts - p.build.back.max(p.build.wall);
        let outer = poly::rounded(&poly::offset(&cavity, p.build.wall), ROUND);
        let latch = latch(&cavity, size.y, p)?;
        Some(DeviceFit {
            size,
            retention,
            recess,
            front,
            face,
            stops,
            nest,
            cavity,
            build: p.build,
            fit: p.fit,
            outer,
            struts,
            gussets,
            z_struts,
            z_back,
            latch,
        })
    }

    /// Half the width the device takes behind the panel, collar included.
    pub fn half_width(&self) -> f64 {
        let (lo, hi) = poly::bounds(&self.outer).expect("a fit has an outline");
        (hi[0] - lo[0]) / 2.0 + self.fit.collar + self.build.wall
    }

    /// How tall the cage is.
    pub fn height(&self) -> f64 {
        let (lo, hi) = poly::bounds(&self.cavity).expect("a fit has an outline");
        hi[1] - lo[1] + 2.0 * self.build.wall
    }

    /// Place the device with its center at `x` and its cage centered on a panel spanning `y0..y1`.
    pub fn module(&self, x: f64, (y0, y1): (f64, f64)) -> Module {
        let (b, fit) = (&self.build, &self.fit);
        let (t, wall, g) = (b.plate, b.wall, fit.collar);
        let (clo, chi) = poly::bounds(&self.cavity).expect("a fit has an outline");
        // The latch, where there's room for it: across, up, with its pads and the collar round them,
        // and behind the panel.
        let l = &self.latch;
        let need = [l.need, self.height() + l.pads[0] + l.pads[1] + 2.0 * (g + COLLAR.min(wall)), l.deep];
        let room = [l.straight, y1 - y0, -self.z_struts];
        let issue = (0..3).any(|i| need[i] > room[i]).then_some(Issue::Unlatched { need, room });
        let latched = issue.is_none();
        let [below, above] = if latched { l.pads } else { [0.0; 2] };
        // Centered on the panel, pads and all.
        let shift = DVec3::new(x, (y0 + y1 - clo[1] - chi[1] + below - above) / 2.0, -self.recess);
        let (olo, ohi) = poly::bounds(&self.outer).expect("a fit has an outline");
        // The front outline runs straight up past the front, so the nest meets the front at it.
        let front = self.nest.first().map(|(_, s)| (PAST, s));
        let nest: Vec<(f64, Vec<Contour>)> = front
            .into_iter()
            .chain(self.nest.iter().map(|(z, s)| (*z, s)))
            .map(|(z, s)| (z, poly::translate(s, x, shift.y)))
            .collect();

        // The cavity's ceiling or floor at each x, where it runs flat enough for the cuts' rounding.
        let cavity = poly::translate(&self.cavity, x, shift.y);
        let face = |top: bool| {
            let cavity = &cavity;
            move |at: f64| {
                let hits = poly::crossings(cavity, 1, at).into_iter();
                let hit =
                    if top { hits.max_by(|a, b| a.0.total_cmp(&b.0)) } else { hits.min_by(|a, b| a.0.total_cmp(&b.0)) };
                hit.filter(|(_, slope)| slope.abs() < 0.25).map(|(at, _)| at)
            }
        };

        // The latch, in each wall: `s` points out of the cage, `d` from a tongue's free end to its
        // root. The tongue's free end reaches REACH past the collar, and its back edge runs from
        // there towards its root at 45 degrees, so that edge rests on the layer below as the cage
        // prints, cut halfway; the slit beyond its free end runs straight up, and ends flat there. Where the wall
        // is too thin for the tongue and the room it bends into, or runs higher than the tongue
        // under a ledge, a pad stands proud of it, which the collar follows, and which steps back
        // to the wall at 45 degrees behind the latch. The pocket behind the tongue opens into the
        // cavity, with a 45-degree root, which a sharp corner would crack at. The cuts reach past
        // the device's face, into the cavity, however the wall runs over them, and round the
        // corners they leave.
        let z_collar = -t - b.collar_depth;
        let (z_free, z_front) = (z_collar - REACH, -t + 1.0);
        // The nub stands EDGE ahead of the collar's back end, unless that's too close to the plate.
        let proud = g + fit.hook;
        let z_catch = (z_collar + EDGE + (CATCH + RAMP) * proud).min(-t - EDGE);
        let shell = Solid::prism_z(poly::translate(&self.outer, x, shift.y), self.z_back, -t).labeled("cage shell");
        let (mut collar, mut shell, mut outline) = (Vec::new(), vec![shell], self.outer.clone());
        let mut cut = vec![Solid::prism_z(cavity.clone(), self.z_back - 1.0, -t + 1.0).labeled("cavity")];
        // Per wall, where the openings beside the tongues step out under them, where the tongues'
        // pads end towards the middle, and what they span across.
        let (mut behind, mut inner, mut pads) = ([z_collar; 2], [Vec::new(), Vec::new()], [Vec::new(), Vec::new()]);
        for (w, (s, inside, pad)) in [(-1.0, clo[1], below), (1.0, chi[1], above)].into_iter().enumerate() {
            let (inside, level) = (inside + shift.y, l.levels[w] + shift.y);
            let out = inside + s * (wall + pad);
            let through = span(level - s, out + s);
            for &(free, root, stop) in l.tongues[w].iter().filter(|_| latched) {
                let (free, root, stop, d) = (free + x, root + x, stop + x, (root - free).signum());
                // Its back edge's cut ends halfway to its root, and the pad and the openings beside it
                // follow from there.
                let (half, z_root) = ((free + root) / 2.0, z_free - (root - free).abs() / 2.0);
                let z_pad = z_root - SLIT - 1.0;
                let (p0, p1) = span(free - d * (SLIT + wall), stop);
                let (n0, n1) = span(free + d * (EDGE + g), free + d * (EDGE + g + NUB));
                // Its fillet included.
                inner[w].push(stop + d * ROUND);
                pads[w].push((p0, p1));
                // More than a snapping step proud of the wall, the pad goes on.
                let (r0, r1) = span(out - shift.y, level - shift.y);
                let rect = poly::rect(p0 - x, r0, p1 - x, r1);
                let k = poly::bounds(&poly::difference(&rect, &self.outer)).map_or(0.0, |(a, b)| b[1] - a[1]);
                if k > 2.0 * poly::RESOLUTION {
                    // Past the slit it's as wide as the wall, and meets the wall where that runs up from
                    // the flat stretch square, as the collar follows it.
                    let base = out - s * k;
                    let ends = if d > 0.0 { [(0.0, 0.0), (ROUND, ROUND)] } else { [(ROUND, ROUND), (0.0, 0.0)] };
                    let pad = bump(p0, p1, base, s, k, ends);
                    outline =
                        poly::union(&outline, &vec![vec![pad.iter().map(|q| [q[0] - x, q[1] - shift.y]).collect()]]);
                    let tips = [(-t, k), (z_pad, k), (z_pad - k, 0.0)];
                    shell.push(rise(&pad, base, s, &tips).labeled("latch pad"));
                }
                behind[w] = behind[w].min(z_pad - k - RAIL);
                // Up, and along the back edge, flat at its ends. Inside, it opens through the tongue and
                // the pocket behind it, or beside them through the wall, wherever that runs flat.
                let under = out - s * wall;
                let depth = s * (under - inside);
                let floor_or_ceiling = face(s > 0.0);
                let (past_free, past_root) = (|u: f64| (u - free) * d, |u: f64| (u - root) * d);
                let tongue = |u: f64| (past_free(u) > -SLIT / 2.0 && past_root(u) < 0.0).then_some(under);
                let beside = |u: f64| if past_free(u) < -SLIT / 2.0 { floor_or_ceiling(u) } else { None };
                let at_out = |_: f64| Some(out);
                let faces: [Face; 3] = [(&tongue, out - under), (&beside, out - under), (&at_out, under - out)];
                let (a, b) = (free - d * SLIT, half - d * SLIT);
                let slit =
                    vec![[a, z_front], [a, z_free], [b, z_root], [half, z_root], [free, z_free], [free, z_front]];
                cut.push(Solid::through(vec![slit], through, &faces, SNUG).labeled("tongue slits"));
                // Its root runs on along the wall's face, just inside the cavity, to round the corner it
                // makes there.
                let mut pocket = vec![
                    [free - d * SLIT / 2.0, under],
                    [root - d * depth, under],
                    [root + d * poly::RESOLUTION, inside - s * poly::RESOLUTION],
                    [root + d * (poly::RESOLUTION + 1.0), inside - s * poly::RESOLUTION],
                    [root + d * (poly::RESOLUTION + 1.0), level - s * (EPS + 1.0)],
                    [free - d * SLIT / 2.0, level - s * (EPS + 1.0)],
                ];
                if pocket.area() < 0.0 {
                    pocket.reverse();
                }
                let pocket = poly::fillet(&pocket, ROUND);
                cut.push(Solid::prism_z(vec![pocket], z_root - SLIT / 2.0, z_front).labeled("tongue pocket"));
                // The nub, on the collar's face, reaches `hook` into the tongue: from its catch's foot
                // to its tip, then down the ramp. Its fillets stay out of the hole: `g` from the nub,
                // where the hole ends, they're less than `g` high.
                let foot = out + s * g;
                let nub = bump(n0, n1, foot, -s, proud, [(ROUND, g / (1.0 - std::f64::consts::FRAC_1_SQRT_2)); 2]);
                let tips =
                    [(z_catch, 0.0), (z_catch - CATCH * proud, proud), (z_catch - CATCH * proud - RAMP * proud, 0.0)];
                collar.push(rise(&nub, foot, -s, &tips).labeled("latch nub"));
                // Its hole clears it by `play` front and back where the tongue's outer face meets it,
                // and by the collar's gap either side. From the cage's back its top is a short bridge.
                let (zb, zf) = (z_catch - CATCH * proud - RAMP * fit.hook - fit.play, z_catch - CATCH * g + fit.play);
                let hole = vec![[n0 - g, zb], [n1 + g, zb], [n1 + g, zf], [n0 - g, zf]];
                let at_under = |_: f64| Some(under);
                let tongue: [Face; 2] = [(&at_under, out - under), (&at_out, under - out)];
                cut.push(Solid::through(vec![hole], through, &tongue, SNUG).labeled("latch hole"));
            }
        }

        // Collar: hollowed on its own with the cut running past both ends, then set exactly on
        // the plate. It follows the cage's outside, filleted where the pads stand out of it. On a
        // tight panel its top and bottom thin to what the panel leaves: with the latch, at least
        // COLLAR.
        let outline = poly::closed(&outline, ROUND);
        let reach = g + wall + 1.0;
        let panel = poly::rect(olo[0] - reach, y0 - shift.y, ohi[0] + reach, y1 - shift.y);
        let outer = poly::rounded(&poly::intersection(&poly::offset(&outline, g + wall), &panel), ROUND);
        let inside = poly::offset(&outline, g);
        let inside_x = poly::bounds(&inside).map_or([x, x], |(lo, hi)| [lo[0] + x, hi[0] + x]);
        let hollow =
            vec![Solid::prism_z(poly::translate(&inside, x, shift.y), z_collar - 1.0, -t + 1.0).labeled("collar")];
        let outside = poly::union_polygons(poly::translate(&outer, x, shift.y));
        let ring = Solid::prism_z(poly::translate(&outer, x, shift.y), z_collar, -t).labeled("collar");
        collar.push(Solid::difference(ring, hollow));
        // Behind a blank faceplate, pillars from halfway into the plate to the device's face hold it.
        let z_face = -self.recess - self.face;
        let stops = self.stops.iter().map(|s| poly::translate(&vec![vec![s.clone()]], x, shift.y));
        collar.extend(stops.map(|s| Solid::prism_z(s, z_face, -t / 2.0).labeled("stops")));

        // Cage: the cavity, right through the back, where struts hold the device.
        let end = self.z_struts;
        let (ilo, ihi) = (clo[1] + shift.y, chi[1] + shift.y);
        let mut vents = Vec::new();
        if b.vents {
            // Openings between the band at the back and the band the collar grips, through each wall
            // only, planned on its inside face unrolled: how far along it, and z. The floor's keep to
            // its flat stretch: beyond it the device rests on ledges. The sides' run from just above
            // those round whatever bevels or curves the device has to its top corners, and the top's
            // on round to those. Where a side's meet the top's or the floor's, a COLUMN apart, those give
            // way: the sides' are small enough. Between the latch's tongues the top and bottom ones reach
            // the collar's band, and further back the outermost step out beside the tongues at 45
            // degrees, a rail short of them, to the wall's ends. Where the tongues leave no room between
            // them, the openings stop a rail short of them. The sides stay closed further from the
            // back, where they hold up the device's back corners.
            let (v0, front, closed) = (end + RAIL, z_collar - BAND, end + RAIL.max(CLOSED * self.size.z));
            let lean = (90.0 - b.overhang.clamp(10.0, 80.0)).to_radians().tan();
            for ring in &cavity {
                let corners = unrolled(ring);
                let m = corners.len() - 1;
                let at = |i: usize| DVec2::from(corners[i].1);
                let level = |i: usize| (at(i + 1).y - at(i).y).abs() <= STRAIGHT;
                let longest = |i: &usize, j: &usize| {
                    (corners[*i + 1].0 - corners[*i].0).total_cmp(&(corners[*j + 1].0 - corners[*j].0))
                };
                // The ceiling's flat stretch, and the run of stretches facing up round it.
                let up = |i: usize| at(i).x - at(i + 1).x >= (at(i + 1).y - at(i).y).abs();
                let Some(ceiling) = (0..m).filter(|&i| level(i) && (at(i).y - ihi).abs() <= STRAIGHT).max_by(longest)
                else {
                    continue;
                };
                let (mut t0, mut t1) = (ceiling, ceiling + 1);
                while t0 > 0 && up(t0 - 1) {
                    t0 -= 1;
                }
                while t1 < m && up(t1) {
                    t1 += 1;
                }
                // Where the outline climbs past the ledges, going up the right side and coming down the
                // left: far enough past their rounded corners with the sides that the openings' own
                // rounding stays on the flat, rather than cutting a groove into the ledge.
                let ledges = ilo + b.air + 2.0 * ROUND;
                let cross = |i: usize| {
                    let (a, b) = (at(i), at(i + 1));
                    corners[i].0 + (corners[i + 1].0 - corners[i].0) * ((ledges - a.y) / (b.y - a.y)).clamp(0.0, 1.0)
                };
                let rise = (0..t0).find(|&i| at(i).y < ledges && at(i + 1).y >= ledges).map(cross);
                let fall = (t1..m).rev().find(|&i| at(i).y >= ledges && at(i + 1).y < ledges).map(cross);
                let s = |i: usize| corners[i].0;
                // Each wall: its name, where its openings may go, which of the latch's walls it is,
                // and from where they open.
                let floor = level(0) && (at(0).y - ilo).abs() <= STRAIGHT;
                let bottom = [
                    (s(0) + ROUND).max(fall.map_or(f64::MIN, |f| f - s(m) + COLUMN)),
                    (s(1) - ROUND).min(rise.map_or(f64::MAX, |r| r - COLUMN)),
                ];
                let top = [s(t0) + COLUMN - RAIL / 2.0, s(t1) - COLUMN + RAIL / 2.0];
                let walls = [
                    floor.then_some(("bottom openings", bottom, Some((0, 0)), v0)),
                    Some(("top openings", top, Some((1, ceiling)), v0)),
                    rise.map(|r| ("side openings", [r, s(t0) - RAIL / 2.0], None, closed)),
                    fall.map(|f| ("side openings", [s(t1) + RAIL / 2.0, f], None, closed)),
                ];
                for (name, [lo, hi], latch, v0) in walls.into_iter().flatten() {
                    // The bridge goes on the longest straight stretch: the latch's flat one in its walls.
                    let stretch = match latch {
                        Some((_, i)) => Some(i),
                        None => (0..m).filter(|&i| s(i + 1) > lo && s(i) < hi).max_by(longest),
                    };
                    let Some(stretch) = stretch else { continue };
                    let bridge = [s(stretch).max(lo), s(stretch + 1).min(hi)];
                    let shapes = match latch {
                        Some((w, i)) => {
                            // How far along the outline an x is, on the latch's stretch.
                            let along = |x: f64| s(i) + (x - at(i).x) * (at(i + 1).x - at(i).x).signum();
                            let (a, b, v1) = match inner[w][..] {
                                [a, b] if b - a > 3.0 * RAIL => {
                                    let (a, b) = span(along(a + RAIL), along(b - RAIL));
                                    (a, b, front)
                                }
                                [_, _] => (lo, hi, behind[w]),
                                _ => (lo, hi, front),
                            };
                            openings(a, b, v0, v1, [lo, hi], behind[w], bridge, lean)
                        }
                        None => openings(lo, hi, v0, front, [lo, hi], front, bridge, lean),
                    };
                    if !shapes.is_empty() {
                        let vent = Vent { name, corners: corners.clone(), bridge, openings: shapes };
                        cut.push(vent.cut(wall).labeled(name));
                        vents.push(vent);
                    }
                }
            }
        }
        let struts = Solid::prism_z(poly::translate(&self.struts, x, shift.y), self.z_back, end).labeled("back struts");
        let mut cage = vec![struts, Solid::difference(Solid::union(shell), cut)];
        // The floor's columns' ridges run from the cage's back to its front, meeting the floor in
        // fillets: under a tongue, which they'd keep from bending in, only to where the openings
        // beside it stop.
        let ridge = b.air / 2.0;
        for vent in vents.iter().filter(|v| v.name == "bottom openings" && ridge > 0.0) {
            // The floor is the first stretch round.
            let [(s0, from), (_, to)] = [vent.corners[0], vent.corners[1]];
            let across = |u: f64| from[0] + (u - s0) * (to[0] - from[0]).signum();
            let mut spans: Vec<(P2, P2)> =
                vent.openings.iter().filter_map(|o| poly::bounds(&vec![vec![o.clone()]])).collect();
            spans.sort_by(|p, q| p.0[0].total_cmp(&q.0[0]));
            for pair in spans.windows(2) {
                let [(_, hi), (next_lo, _)] = pair else { unreachable!() };
                let (mid, half) = (across((hi[0] + next_lo[0]) / 2.0), RIDGE / 2.0 + ROUND);
                let rib = poly::rounded_rect(
                    mid - RIDGE / 2.0,
                    ilo - EPS,
                    mid + RIDGE / 2.0,
                    ilo + ridge,
                    ROUND.min(ridge / 2.0),
                );
                // The fillets end on the floor's face, along a line: none of their faces lies on it.
                let foot = poly::rect(mid - half - ROUND, ilo - EPS, mid + half + ROUND, ilo);
                let rib = poly::closed(&poly::union(&vec![vec![rib]], &foot), ROUND);
                let rib = poly::intersection(&rib, &poly::rect(mid - half, ilo - EPS, mid + half, ilo + ridge + 1.0));
                let under = pads[0].iter().any(|&(a, b)| a < mid + half + RAIL && b > mid - half - RAIL);
                let z1 = if under { behind[0] } else { -t };
                cage.extend(rib.into_iter().map(|r| Solid::prism_z(r, self.z_back, z1).labeled("floor ridges")));
            }
        }
        for (shape, face, s, h) in &self.gussets {
            let shape: Contour = shape.iter().map(|q| [q[0] + x, q[1] + shift.y]).collect();
            let tips = [(end - EPS, *h), (end, *h), (end + h, 0.0)];
            cage.push(rise(&shape, face + shift.y, *s, &tips).labeled("strut gussets"));
        }

        let (collar, nest) = (Solid::union(collar), terrain(&nest, -t - 1.0).labeled("nest"));
        Module { collar, inside: inside_x, outside, nest, cage: Solid::union(cage), shift, issue, vents }
    }
}

/// A device placed in a row.
pub struct Module {
    /// Added to the faceplate, with the latch's nubs.
    pub collar: Solid,
    /// Where the collar's inside, which the cage slides into, ends left and right: what stands
    /// next to the collar on the faceplate runs on into it up to there.
    pub inside: [f64; 2],
    /// The collar's outside, which what stands next to it meets in fillets.
    pub outside: Shapes,
    /// Cut from the faceplate.
    pub nest: Solid,
    pub cage: Solid,
    /// Device frame to rack frame.
    pub shift: DVec3,
    /// Why the cage isn't latched, if it isn't.
    pub issue: Option<Issue>,
    /// What the cage's openings are cut from.
    pub vents: Vec<Vent>,
}

/// A wall's openings, unrolled on the cavity's outline: how far along it, and z.
pub struct Vent {
    pub name: &'static str,
    /// The outline's corners, each with how far along it it is (see [`unrolled`]).
    pub corners: Vec<(f64, P2)>,
    /// Where the openings may bridge: the wall's straight stretch.
    pub bridge: [f64; 2],
    pub openings: Vec<Contour>,
}

impl Vent {
    /// The openings cut through a wall `wall` thick, each in one piece round the wall's corners:
    /// square through each straight stretch, mitred where two meet, which both put a point the same
    /// depth into the wall at the same place.
    fn cut(&self, wall: f64) -> Solid {
        let (s, at) = (|i: usize| self.corners[i].0, |i: usize| DVec2::from(self.corners[i].1));
        let m = self.corners.len() - 1;
        let place = |t: f64, u: f64, z: f64| {
            let i = (1..m).take_while(|&k| s(k) <= u).last().unwrap_or(0);
            let (a, d, length) = (at(i), (at(i + 1) - at(i)).normalize(), s(i + 1) - s(i));
            let [ta, tb] = [self.mitre(i), self.mitre(i + 1)].map(|m| m.dot(d));
            let (out, u) = (DVec2::new(d.y, -d.x), u - s(i));
            (a + d * u + out * t + d * t * (ta + (tb - ta) * u / length)).extend(z).to_array()
        };
        // A point wherever an opening's edge passes a corner, so it bends there with the wall.
        let contours: Vec<Contour> = self
            .openings
            .iter()
            .map(|o| {
                let n = o.len();
                (0..n)
                    .flat_map(|k| {
                        let (p, q) = (o[k], o[(k + 1) % n]);
                        let mut passed: Vec<P2> = (1..m)
                            .map(s)
                            .filter(|&c| (p[0] < c && c < q[0]) || (q[0] < c && c < p[0]))
                            .map(|c| [c, p[1] + (q[1] - p[1]) * (c - p[0]) / (q[0] - p[0])])
                            .collect();
                        passed.sort_by(|a, b| ((a[0] - p[0]).abs()).total_cmp(&(b[0] - p[0]).abs()));
                        std::iter::once(p).chain(passed)
                    })
                    .collect()
            })
            .collect();
        let (inside, outside) = (|_: f64| Some(0.0), |_: f64| Some(wall));
        let faces: [Face; 2] = [(&inside, wall), (&outside, -wall)];
        Solid::bent(&contours, (-1.0, wall + 1.0), &faces, ROUND, &place, &(1..m).map(s).collect::<Vec<_>>())
    }

    /// Which way, per millimeter into the wall, the cuts square to the stretches either side of corner
    /// `i` meet: along the line that halves the turn.
    fn mitre(&self, i: usize) -> DVec2 {
        let m = self.corners.len() - 1;
        let out = |k: usize| {
            let d = (DVec2::from(self.corners[(k + 1) % m].1) - DVec2::from(self.corners[k % m].1)).normalize();
            DVec2::new(d.y, -d.x)
        };
        let (a, b) = (out(i + m - 1), out(i));
        (a + b) / (1.0 + a.dot(b))
    }
}

/// The corners of an outline's straight stretches (see [`stretches`]) from where its floor starts, or
/// its lowest corner, round to there again, each with how far along the outline it is.
fn unrolled(ring: &Contour) -> Vec<(f64, P2)> {
    let corners = stretches(ring);
    let n = corners.len();
    let ylo = corners.iter().map(|c| c[1]).fold(f64::INFINITY, f64::min);
    let length = |i: usize| DVec2::from(corners[i]).distance(DVec2::from(corners[(i + 1) % n]));
    let floor = (0..n)
        .filter(|&i| (corners[i][1] - ylo).abs() <= STRAIGHT && (corners[(i + 1) % n][1] - ylo).abs() <= STRAIGHT);
    let start = floor.max_by(|&a, &b| length(a).total_cmp(&length(b))).unwrap_or(0);
    let mut s = 0.0;
    (0..=n)
        .map(|k| {
            let i = (start + k) % n;
            let here = (s, corners[i]);
            s += length(i);
            here
        })
        .collect()
}

/// How deep the device's face is behind its frontmost point: where its silhouette first covers
/// half of what it does further back. LEDs and buttons standing proud of the face don't count.
/// With [`rear_silhouettes`], the same for its back face, behind which ports and plugs stand.
fn face(tris: &[Tri], sweep: fn(&[Tri], &[f64]) -> Vec<Shapes>) -> f64 {
    let depths: Vec<f64> = (1..=200).map(|i| i as f64 * FACE_STEP).collect();
    let areas: Vec<f64> = sweep(tris, &depths).iter().map(|s| s.area()).collect();
    let full = areas.last().copied().unwrap_or(0.0);
    let Some(i) = areas.iter().position(|a| *a >= full / 2.0) else { return 0.0 };
    // Then onto the face itself: a step past it, the plane already cuts into the rounding of the
    // device's edges, where it runs so nearly flat that the mesh's facets kink the outline.
    let (mut lo, mut hi) = (if i == 0 { 0.0 } else { depths[i - 1] }, depths[i]);
    while hi - lo > 1e-4 {
        let mid = (lo + hi) / 2.0;
        if sweep(tris, &[mid])[0].area() >= full / 2.0 { hi = mid } else { lo = mid }
    }
    hi
}

/// The nest: what the plate must clear at each depth as the device slides in from behind, with
/// its frontmost point `recess` behind the plate front and its face `face` behind that. In front of
/// the face the opening is the face's outline less a lip of width `lip`.
fn nest(tris: &[Tri], p: &Project, recess: f64, face: f64, lip: f64) -> Vec<(f64, Shapes)> {
    // Only the part of the device in the plate counts, and each look is far quicker without the rest.
    let deepest = (p.build.plate - recess).max(face);
    let ahead: Vec<Tri> = tris.iter().filter(|t| t.iter().any(|v| v.z >= -deepest)).copied().collect();
    let look = |depth: f64| {
        let s = &front_silhouettes(&ahead, &[(depth - recess).max(face)])[0];
        // Not faired: each look would straighten its own way, and on the shallow slopes at the front
        // outlines that wobble against each other by hundredths of a millimeter crease the surface.
        // A notch whose mouth closes leaves a hole, which would be plate standing in the opening, so
        // we fill that too.
        let hole = poly::offset(&poly::offset(s, p.fit.nest + FAIR + NOTCH), -NOTCH);
        let hole = if depth - recess <= face { poly::offset(&hole, -lip) } else { hole };
        (-depth, poly::fill_holes(&poly::tidy(&hole), 0.0))
    };
    // A stack of looks still to place, the next one on top.
    let mut todo: Vec<(f64, Shapes)> = samples(p.build.plate, NEST_STEP).into_iter().rev().map(look).collect();
    let mut looks: Vec<(f64, Shapes)> = todo.pop().into_iter().collect();
    while let Some(next) = todo.pop() {
        let last = &looks[looks.len() - 1];
        if last.0 - next.0 > FINEST {
            let mid = look(-(last.0 + next.0) / 2.0);
            if !straight(last, &mid, &next) {
                todo.extend([next, mid]);
                continue;
            }
        }
        looks.push(next);
    }
    looks
}

/// Where [`STOP`] pillars meet the device's face: one at each of its corners, a millimeter inside its outline there, so
/// they rest on the face and leave the middle, where plugs go, to the cables.
fn stops(tris: &[Tri], face: f64) -> Vec<Contour> {
    let r = STOP / 2.0;
    let inside: Vec<P2> =
        poly::offset(&front_silhouettes(tris, &[face])[0], -(r + 1.0)).into_iter().flatten().flatten().collect();
    let corner = |(sx, sy): (f64, f64)| {
        inside.iter().copied().max_by(|a, b| (sx * a[0] + sy * a[1]).total_cmp(&(sx * b[0] + sy * b[1])))
    };
    let mut at: Vec<P2> = [(-1.0, -1.0), (1.0, -1.0), (1.0, 1.0), (-1.0, 1.0)].into_iter().filter_map(corner).collect();
    // On a face hardly bigger than a pillar, corners fall together.
    at.dedup_by(|a, b| (a[0] - b[0]).hypot(a[1] - b[1]) < STOP);
    at.iter().map(|c| poly::rounded_rect(c[0] - r, c[1] - r, c[0] + r, c[1] + r, r)).collect()
}

/// The cavity: the device's outline plus clearance, from the plate to its back face. The
/// device's rearmost point is at z = `rear`. Below it, air under all but the device's sides.
/// Returns the outline, where the struts in the open back stand, clear of whatever stands out
/// behind the device's back face, the gussets in front of them, and the z where the cavity ends.
fn cavity(profile: &Profile, p: &Project, rear: f64) -> (Shapes, Shapes, Vec<Gusset>, f64) {
    let c = p.fit.cage;
    let mut outline = clean(&profile.outline, c);
    // With air under the device, the floor drops away between ledges along its sides, which it
    // rests on, their inner edges chamfered at 45 degrees. The channel reaches up into the outline,
    // so that wherever the underside lies, nothing is left between it and the air.
    let air = p.build.air;
    if let Some((lo, hi)) = poly::bounds(&outline).filter(|(lo, hi)| air > 0.0 && hi[0] - lo[0] > 2.0 * (LEDGE + air)) {
        let (x0, x1, floor, mid) = (lo[0] + LEDGE, hi[0] - LEDGE, lo[1] - air, (lo[1] + hi[1]) / 2.0);
        let channel = vec![[x0 + air, floor], [x1 - air, floor], [x1, lo[1]], [x1, mid], [x0, mid], [x0, lo[1]]];
        outline = poly::union(&outline, &poly::rounded(&vec![vec![channel]], ROUND));
    }
    // Its inside corners rounded too, the only way that leaves the device all its room: by growing.
    let outline = poly::tidy(&poly::closed(&outline, ROUND));
    // Struts evenly across the width, in the middle of equal shares of it, from floor to ceiling,
    // meeting both in fillets FILLET big. Where a port or plug stands out behind the device, a strut
    // moves aside, a millimeter at a time and at most a quarter of its share, to where it's least in the
    // way, and is cut back around it, clear by at least ROUND, so the corners that leaves are round.
    let ports = poly::offset(&profile.behind, c.max(ROUND));
    let (mut struts, mut gussets) = (Shapes::new(), Vec::new());
    if let Some((lo, hi)) = poly::bounds(&outline) {
        let width = hi[0] - lo[0];
        let bar = |x: f64| poly::rect(x - STRUT / 2.0, lo[1] - EPS, x + STRUT / 2.0, hi[1] + EPS);
        let blocked = |x: f64| poly::intersection(&bar(x), &ports).area();
        let n = (width / SPACING).round().max(2.0);
        for i in 0..n as usize {
            let mid = lo[0] + (i as f64 + 0.5) * width / n;
            let tries = (0..=(width / n / 4.0) as i32).flat_map(|d| [d, -d]).map(|d| mid + d as f64);
            let x = tries.min_by(|a, b| blocked(*a).total_cmp(&blocked(*b))).unwrap_or(mid);
            let (a, b) = (x - STRUT / 2.0, x + STRUT / 2.0);
            let around = |d: f64| poly::rect(a - d, lo[1] - d, b + d, hi[1] + d);
            // Kept as far as a fillet can fill between it and the walls, where they rise, as by a ledge.
            let walls = poly::difference(&around(3.0 * FILLET), &outline);
            let strut =
                poly::rounded(&poly::difference(&poly::closed(&poly::union(&bar(x), &walls), FILLET), &ports), ROUND);
            let keep = poly::intersection(&poly::offset(&outline, EPS), &around(2.0 * FILLET + 1.0));
            struts = poly::union(&struts, &poly::intersection(&strut, &keep));
            // From its front, a gusset up each wall at 45 degrees, as tall where it starts as the
            // device leaves room: from the wall's face under the strut to the device, with its
            // clearance, behind each millimeter in front of its back face. Under the device that's the
            // air and however much its back edge rounds off. It stays on the rail along the back,
            // clear of the openings.
            let Some((slo, shi)) = poly::bounds(&poly::intersection(&outline, &bar(x))) else { continue };
            for (s, face) in [(1.0, slo[1]), (-1.0, shi[1])] {
                let room = |k: usize| {
                    let device =
                        poly::intersection(&profile.near[k.min(profile.near.len() - 1)], &around(FILLET + 1.0));
                    poly::bounds(&poly::offset(&device, c))
                        .map_or(RAIL, |(dlo, dhi)| s * ([dlo, dhi][(s < 0.0) as usize][1] - face))
                };
                let mut h = RAIL.min(room(0));
                for k in 1..profile.near.len() {
                    if (k - 1) as f64 >= h {
                        break;
                    }
                    h = h.min(room(k) + (k - 1) as f64);
                }
                if h >= 0.5 {
                    gussets.push((bump(a, b, face, s, h, [(ROUND, FILLET); 2]), face, s, h));
                }
            }
        }
    }
    (outline, struts, gussets, rear + profile.back - c)
}

/// The latch's tongues: as short as their root's strain allows, two in the bottom wall and two in
/// the top, each at an end of the stretch where the device's underside or top runs flat, with its
/// free end outwards. Between them the wall is free for openings. Each wall's pad gives its
/// tongues room to bend in by the nub and the play without coming nearer the device than its
/// clearance; under the device, its air gives some of that room. Under the ledges the device rests
/// on, the wall stands higher than over the air, and the pad fills it down to the tongue, reaching
/// in to where the wall itself runs low, so that the tongue's root holds.
fn latch(cavity: &Shapes, top: f64, p: &Project) -> Option<Latch> {
    let (lo, hi) = poly::bounds(cavity)?;
    let flat = |y0: f64, y1: f64| {
        let strip = poly::rect(lo[0] - 1.0, y0, hi[0] + 1.0, y1);
        poly::bounds(&poly::intersection(cavity, &strip)).map_or((0.0, 0.0), |(a, b)| (a[0], b[0]))
    };
    let c = p.fit.cage;
    let levels = [-c + STRAIGHT, top + c - STRAIGHT];
    let walls = [flat(lo[1] - 1.0, lo[1] + STRAIGHT), flat(hi[1] - STRAIGHT, hi[1] + 1.0)];
    let stretches = [flat(lo[1] - 1.0, levels[0]), flat(levels[1], hi[1] + 1.0)];
    // From the root to the middle of the nub, which bends the tongue `hook` and at most `play` more.
    let bend = p.fit.hook + p.fit.play;
    let length = EDGE + p.fit.collar + NUB / 2.0 + (1.5 * bend * p.build.wall / STRAIN).sqrt().ceil();
    // Past the slit, the wall stands as thick as it is to the stretch's end. Each tongue's pad runs
    // from its slit to a millimeter past its root, and on to where the wall itself runs at its
    // extreme, so that the root holds.
    let tongues = [0, 1].map(|w| {
        let ((x0, x1), (w0, w1)) = (stretches[w], walls[w]);
        let (left, right) = (x0 + p.build.wall + SLIT, x1 - p.build.wall - SLIT);
        [
            (left, left + length, (left + length + 1.0).max(w0 + 1.0)),
            (right, right - length, (right - length - 1.0).min(w1 - 1.0)),
        ]
    });
    let pads = [-c - lo[1], hi[1] - top - c].map(|room| (bend - room.max(0.0)).max(0.0));
    let deep = p.build.plate + p.build.collar_depth + REACH + length + SLIT + 1.0 + pads[0].max(pads[1]);
    let straight = (stretches[0].1 - stretches[0].0).min(stretches[1].1 - stretches[1].0).max(0.0);
    Some(Latch { tongues, levels, pads, need: 2.0 * (p.build.wall + SLIT + length + 1.0), straight, deep })
}

/// Depths from 0 to `depth`, `step` apart, both ends included.
fn samples(depth: f64, step: f64) -> Vec<f64> {
    let n = (depth / step).ceil().max(1.0) as usize;
    (0..=n).map(|i| (i as f64 * step).min(depth)).collect()
}

/// Whether outline `o` lies within FOLLOW of the surface running straight between `a` and `b`,
/// the outlines either side of it: at `o`'s depth, that surface as we build it runs within FOLLOW
/// of `o` all round. Blending their distances instead, a side moving out next to an edge that stays
/// put looked bent, being always near that edge, and drafted sides took up to four times the looks
/// they need.
fn straight((za, a): &(f64, Shapes), (zo, o): &(f64, Shapes), (zb, b): &(f64, Shapes)) -> bool {
    let s = crate::terrain::section(&terrain(&[(*za, a.concat()), (*zb, b.concat())], zb - 1.0), *zo);
    let near = |from: &Shapes, to: &Shapes| from.iter().flatten().flatten().all(|&p| poly::distance(to, p) <= FOLLOW);
    near(o, &s) && near(&s, o)
}

/// The outline grown by `clearance` and a little further and shrunk back, which fills notches
/// narrower than 2 NOTCH, then faired: it runs clean, and only ever gets roomier.
fn clean(s: &Shapes, clearance: f64) -> Shapes {
    poly::fair(&poly::offset(&poly::offset(s, clearance + NOTCH), -NOTCH), FAIR)
}

/// From the lower of two numbers to the higher.
fn span(a: f64, b: f64) -> (f64, f64) {
    (a.min(b), a.max(b))
}

/// What stands `height` out of a wall's face at y = `face`, towards `s`, across `x0` to `x1`, in
/// the layer plane, each of its `ends` with its corners rounded and meeting the face in a fillet,
/// by the radii given. It reaches EPS into the wall, short of its face, so no edge runs along it.
fn bump(x0: f64, x1: f64, face: f64, s: f64, height: f64, ends: [(f64, f64); 2]) -> Contour {
    let (y0, y1) = span(face - s * 2.0 * EPS, face + s * height);
    let (w0, w1) = span(face - s * 2.0 * EPS, face - s * EPS / 2.0);
    let (b0, b1) = span(face - s * EPS, face + s * (height + 1.0));
    let [(left, l), (right, r)] = ends;
    let (u0, u1, fillet) = (x0 - l, x1 + r, l.max(r));
    let body = poly::sided_rect(x0, y0, x1, y1, left, right);
    let body = poly::union(&vec![vec![body]], &poly::rect(u0, w0, u1, w1));
    let shape = poly::tidy(&poly::intersection(&poly::closed(&body, fillet), &poly::rect(u0, b0, u1, b1)));
    shape.into_iter().map(|mut s| s.swap_remove(0)).max_by(|a, b| a.area().total_cmp(&b.area())).unwrap_or_default()
}

/// A [`bump`] through z: at each z of `tips` as tall as given, squashed towards the
/// wall, so its outline in every layer stays round. Between them its top runs straight, through
/// rings a fifth of a millimeter apart in height: the faces between two rings squashed by
/// different amounts twist, and slice into zigzags where they're too far apart.
fn rise(shape: &Contour, face: f64, s: f64, tips: &[(f64, f64)]) -> Solid {
    // As tall as it came out of the tidying, which snaps it to the grid.
    let height = shape.iter().map(|q| s * (q[1] - face)).fold(f64::NEG_INFINITY, f64::max);
    let base = face - s * EPS;
    let ring =
        |z: f64, h: f64| shape.iter().map(|q| [q[0], base + (q[1] - base) * (h + EPS) / (height + EPS), z]).collect();
    let mut rings = vec![ring(tips[0].0, tips[0].1)];
    for pair in tips.windows(2) {
        let [(z0, h0), (z1, h1)] = [pair[0], pair[1]];
        let n = ((h1 - h0).abs() / 0.2).ceil().max(1.0) as usize;
        rings.extend((1..=n).map(|i| i as f64 / n as f64).map(|f| ring(z0 + f * (z1 - z0), h0 + f * (h1 - h0))));
    }
    Solid::loft(rings)
}

/// The corners where an outline's straight stretches meet, counterclockwise: as long as each goes while
/// the outline stays within STRAIGHT of the line between its ends. Round a curve they're short.
fn stretches(ring: &Contour) -> Vec<P2> {
    let mut ring = ring.clone();
    if ring.area() < 0.0 {
        ring.reverse();
    }
    let n = ring.len();
    let at = |i: usize| DVec2::from(ring[i % n]);
    // From the sharpest corner, so no stretch runs round the start.
    let turn = |i: usize| (at(i + n) - at(i + n - 1)).normalize_or_zero().dot((at(i + 1) - at(i)).normalize_or_zero());
    let start = (0..n).min_by(|&a, &b| turn(a).total_cmp(&turn(b))).unwrap_or(0);
    let fits = |i: usize, j: usize| {
        let d = (at(j) - at(i)).normalize_or_zero();
        (i + 1..j).all(|k| (at(k) - at(i)).perp_dot(d).abs() <= STRAIGHT)
    };
    let (mut corners, mut i) = (Vec::new(), start);
    while i < start + n {
        let mut j = i + 1;
        while j < start + n && fits(i, j + 1) {
            j += 1;
        }
        corners.push(ring[i % n]);
        i = j;
    }
    corners
}

/// Openings in a wall, `u` across and `v` along the cage, as few and as big as bridges allow.
/// Printed with v up, an opening's top edge is a bridge; 45-degree chamfers at its corners keep that
/// to BRIDGE, so where one opening would need more, several sit side by side. It only bridges within
/// `bridge`, where the wall runs straight: beyond it the top leans down `lean` per millimeter, which
/// the layers below hold up however the wall turns, and a top corner there needs no chamfer. Below `knee` the outermost reach on out to
/// `wide`, their sides stepping out at 45 degrees, which bridges nothing.
#[allow(clippy::too_many_arguments)]
fn openings(
    u0: f64,
    u1: f64,
    v0: f64,
    v1: f64,
    wide: [f64; 2],
    knee: f64,
    bridge: [f64; 2],
    lean: f64,
) -> Vec<Contour> {
    let (width, height) = (u1 - u0, v1 - v0);
    if width < RAIL || height < RAIL {
        return Vec::new();
    }
    // Chamfers can take at most half the height each side, so it takes at least this many; and
    // past the count where they're at their least, more only add struts. Of those, the most open.
    let fewest = ((width + COLUMN) / (BRIDGE + height + COLUMN)).ceil() as usize;
    let most = ((width + COLUMN) / (BRIDGE + 2.0 * COLUMN)).ceil() as usize;
    let shape = |n: usize| {
        let w = (width - (n as f64 - 1.0) * COLUMN) / n as f64;
        let c = ((w - BRIDGE) / 2.0).max(RAIL / 2.0).min(height.min(w) / 2.0 - 0.01);
        (n, w, c, n as f64 * (w * height - 2.0 * c * c))
    };
    let (n, w, c, _) = (fewest..=most.max(fewest)).map(shape).max_by(|a, b| a.3.total_cmp(&b.3)).unwrap();
    let k = knee.clamp(v0 + c, v1 - c);
    let all = (0..n).map(|i| {
        let (a, b) = (u0 + i as f64 * (w + COLUMN), u0 + i as f64 * (w + COLUMN) + w);
        // How far the outermost reach out: no further than their bottom chamfers let them.
        let (l, r) = (
            if i == 0 { (a - wide[0]).min(k - v0 - c) } else { 0.0 },
            if i == n - 1 { (wide[1] - b).min(k - v0 - c) } else { 0.0 },
        );
        let right = [[b + r - c, v0], [b + r, v0 + c], [b + r, k - r], [b, k]];
        let left = [[a, k], [a - l, k - l], [a - l, v0 + c], [a - l + c, v0]];
        let top_right = if b - c >= bridge[1] { vec![[b, v1]] } else { vec![[b, v1 - c], [b - c, v1]] };
        let top_left = if a + c <= bridge[0] { vec![[a, v1]] } else { vec![[a + c, v1], [a, v1 - c]] };
        // Where it doesn't reach out, the two points of its knee are one, give or take rounding.
        let mut o = [right.to_vec(), top_right, top_left, left.to_vec()].concat();
        o.dedup_by(|p, q| (p[0] - q[0]).hypot(p[1] - q[1]) < poly::RESOLUTION);
        o
    });
    // Under a roof flat over the bridge's stretch, a millimeter up, leaning down beyond it.
    let (l, r) = (wide[0].min(u0) - 1.0, wide[1].max(u1) + 1.0);
    let top = |u: f64| v1 - lean * (bridge[0] - u).max(u - bridge[1]).max(0.0);
    let [b0, b1] = bridge.map(|b| b.clamp(l, r));
    let roof = [[l, top(l)], [b0, top(b0)], [b0, v1 + 1.0], [b1, v1 + 1.0], [b1, top(b1)], [r, top(r)]];
    let roof = [roof.to_vec(), vec![[r, v0 - 1.0], [l, v0 - 1.0]]].concat();
    poly::intersection(&poly::union_polygons(all), &vec![vec![roof]]).into_iter().flatten().collect()
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::csg::Axis;
    use crate::mesh;
    use std::f64::consts::TAU;

    /// A device 60 deep with this convex outline, front to back.
    fn extruded(outline: Contour) -> Vec<Tri> {
        mesh::loft(&[(0.0, outline.clone()), (-60.0, outline)])
    }

    /// A device whose bottom edges are chamfered, as most are rounded: with air under it, something
    /// must still be right under its flat underside to rest on.
    #[test]
    fn a_device_with_rounded_edges_rests_on_something() {
        let tris = extruded(vec![[-45.0, 0.0], [45.0, 0.0], [50.0, 5.0], [50.0, 20.0], [-50.0, 20.0], [-50.0, 5.0]]);
        let p = Project::default();
        let fit = crate::Device::from(tris).fit(&p, None).unwrap();
        // Just under the underside, past its clearance: somewhere there must be material.
        let below = (-45..=45).map(|x| x as f64).filter(|&x| {
            let probe = poly::rect(x - 0.05, -p.fit.cage - 0.3, x + 0.05, -p.fit.cage - 0.2);
            poly::difference(&probe, &fit.cavity).area() > 1e-4
        });
        assert!(below.count() > 0, "nothing under the device");
    }

    /// Round a device with rounded corners, the cage's outside and the collar around it bend one way
    /// only: no step a printer would have to stop at.
    #[test]
    fn the_cage_and_collar_are_smooth_round_a_rounded_device() {
        let tris = extruded(poly::rounded_rect(-50.0, 0.0, 50.0, 40.0, 10.0));
        let mut p = Project::default();
        p.build.air = 0.0;
        let fit = crate::Device::from(tris).fit(&p, None).unwrap();
        let collar = poly::offset(&fit.outer, p.fit.collar + p.build.wall);
        for (name, s) in [("cage", &fit.outer), ("collar", &collar)] {
            let ring = &s[0][0];
            let turns = (0..ring.len()).map(|i| {
                let [a, b, c] = [0, 1, 2].map(|k| ring[(i + k) % ring.len()]);
                (b[0] - a[0]) * (c[1] - b[1]) - (b[1] - a[1]) * (c[0] - b[0])
            });
            assert!(turns.clone().all(|t| t > -1e-6), "the {name} steps in: {ring:?}");
        }
    }

    /// What stands out of a wall is round in the layer plane, but where it reaches into the wall.
    #[test]
    fn a_bump_is_round() {
        for (s, fillet) in [(1.0, ROUND), (-1.0, FILLET)] {
            let bump = bump(0.0, 6.0, 0.0, s, 2.0, [(ROUND, fillet); 2]);
            let sharp = poly::sharp_corners(std::slice::from_ref(&bump));
            assert!(sharp.iter().all(|q| s * q[1] < 0.0), "{sharp:?}");
        }
    }

    /// The latch's pads are round where they stand out of the cage, but for their ends past the slits,
    /// which meet the wall square.
    #[test]
    fn the_pads_are_round() {
        let p = Project::default();
        let m = crate::Device::boxed([150.0, 30.0, 100.0]).fit(&p, None).unwrap().module(0.0, (-30.0, 30.0));
        let pads = meshes(&m.cage, "latch pad");
        assert_eq!(pads.len(), 4);
        for pad in pads {
            // The first ring, at the plate, and how far out it stands.
            let z = pad[0][2];
            let ring: Contour = pad.iter().take_while(|v| v[2] == z).map(|v| [v[0], v[1]]).collect();
            let s = if ring[0][1] > m.shift.y { 1.0 } else { -1.0 };
            let out = ring.iter().map(|q| s * q[1]).fold(f64::NEG_INFINITY, f64::max);
            let end = ring.iter().map(|q| q[0].abs()).fold(0.0, f64::max);
            let sharp = poly::sharp_corners(std::slice::from_ref(&ring));
            let round = sharp.iter().filter(|q| end - q[0].abs() > 0.1);
            assert!(
                round.clone().all(|q| out - s * q[1] > SNUG),
                "{:?} on a pad out to {}",
                round.collect::<Vec<_>>(),
                s * out
            );
        }
    }

    /// Where the pads stand out of the cage, the collar's inside follows them in fillets, and is
    /// round all along but where it follows their square ends past the slits.
    #[test]
    fn the_collar_is_round_inside_and_out() {
        let p = Project::default();
        let fit = crate::Device::boxed([150.0, 30.0, 100.0]).fit(&p, None).unwrap();
        let m = fit.module(0.0, (-30.0, 30.0));
        assert!(!m.cage.find("latch pad").is_empty());
        let ends: Vec<f64> = fit
            .latch
            .tongues
            .iter()
            .flatten()
            .map(|&(free, root, _)| free - (root - free).signum() * (SLIT + p.build.wall))
            .collect();
        let ring = m.collar.profiles("collar");
        let sharp = poly::sharp_corners(&ring);
        assert!(sharp.iter().all(|q| ends.iter().any(|e| (q[0] - e).abs() < 0.5)), "{sharp:?}, pads end at {ends:?}");
    }

    /// The device's back corners rest on the cage's side walls: over the back fifth of a 100 mm deep
    /// device, give or take the clearance, the side openings leave them closed.
    #[test]
    fn the_side_walls_hold_the_back_corners() {
        let p = Project::default();
        let fit = crate::Device::boxed([150.0, 30.0, 100.0]).fit(&p, None).unwrap();
        let m = fit.module(0.0, (-44.0, 0.0));
        let zs: Vec<f64> = vents(&m, "side openings").flat_map(|v| v.openings.iter().flatten().map(|q| q[1])).collect();
        let back = -fit.recess - fit.size.z;
        let nearest = zs.iter().copied().fold(f64::INFINITY, f64::min);
        assert!(!zs.is_empty() && nearest > back + 19.0, "open from {} mm off the back", nearest - back);
    }

    /// Seated, each nub sits in its tongue's hole `hook` deep, touching nothing; pulled back by a
    /// little more than `play` the tongue meets its catch, which stands at 60 degrees to the pull,
    /// by a little less it doesn't. Across the nub, in z and y.
    #[test]
    fn the_nubs_sit_in_their_holes_with_play() {
        for hook in [1.0, 2.0] {
            let mut p = Project::default();
            p.fit.hook = hook;
            let t = p.build.plate;
            let fit = crate::Device::boxed([150.0, 30.0, 100.0]).fit(&p, None).unwrap();
            let m = fit.module(0.0, (-21.8, 21.8));
            let (nubs, holes) = (meshes(&m.collar, "latch nub"), m.cage.profiles("latch hole"));
            assert_eq!((nubs.len(), holes.len(), m.issue), (4, 4, None));
            let (lo, hi) = poly::bounds(&fit.cavity).unwrap();
            for (nub, hole) in nubs.into_iter().zip(holes) {
                // Nubs seen along x, in (y, z), holes (z, x).
                let s = if nub.iter().any(|v| v[1] > m.shift.y + 15.0) { 1.0 } else { -1.0 };
                let nub = hull(nub.iter().map(|v| [v[1], v[2]]).collect());
                let (a, b) = poly::bounds(&vec![vec![hole]]).unwrap();
                let tip = *nub.iter().min_by(|p, q| (s * p[0]).total_cmp(&(s * q[0]))).unwrap();
                let z = nub.iter().map(|q| q[1]).fold(f64::NEG_INFINITY, f64::max);
                let foot = *nub.iter().filter(|q| q[1] == z).min_by(|p, q| (s * p[0]).total_cmp(&(s * q[0]))).unwrap();
                let catch = (foot[0] - tip[0]).abs().atan2(foot[1] - tip[1]).to_degrees();
                assert!((catch - 60.0).abs() < 1e-6, "a {catch}-degree catch");
                let wall = p.build.wall + fit.latch.pads[(s > 0.0) as usize];
                let out = m.shift.y + if s > 0.0 { hi[1] + wall } else { lo[1] - wall };
                let (y0, y1) = span(out, out - s * p.build.wall);
                let band = poly::rect(y0, -t - 60.0, y1, -t);
                let tongue = |dz: f64| {
                    poly::translate(&poly::difference(&band, &poly::rect(y0 - 1.0, a[0], y1 + 1.0, b[0])), 0.0, dz)
                };
                let nub = vec![vec![nub]];
                let touch = |dz: f64| poly::intersection(&nub, &vec![tongue(dz)]).area();
                assert!(touch(0.0) < 1e-6, "the nub runs into the tongue");
                let play = p.fit.play;
                assert!(touch(-play + 0.02) < 1e-6 && touch(-play - 0.02) > 1e-4, "not {play} mm of play");
                let (c, d) = poly::bounds(&poly::intersection(&nub, &band)).unwrap();
                assert!((d[0] - c[0] - hook.min(p.build.wall)).abs() < 1e-6, "{} mm into the tongue", d[0] - c[0]);
            }
        }
    }

    /// Bent in all the way, by the nub and the play, each tongue keeps the device's clearance,
    /// however thick the walls and whatever the air under the device. The pads that make the room
    /// stand proud only where the tongues are: the cage's shell stays as tall as the cavity and its
    /// walls.
    #[test]
    fn the_tongues_bend_clear_of_the_device() {
        for (hook, wall, air) in [(0.5, 1.2, 0.0), (1.2, 1.6, 2.0), (2.0, 1.6, 0.5), (1.0, 4.0, 0.0), (1.0, 0.4, 3.0)] {
            let mut p = Project::default();
            (p.fit.hook, p.build.wall, p.build.air) = (hook, wall, air);
            let fit = crate::Device::boxed([150.0, 30.0, 100.0]).fit(&p, None).unwrap();
            let m = fit.module(0.0, (-30.0, 30.0));
            let pockets = m.cage.profiles("tongue pocket");
            assert_eq!((pockets.len(), &m.issue), (4, &None));
            let c = p.fit.cage;
            for pocket in pockets {
                let ys = pocket.iter().map(|q| q[1] - m.shift.y);
                let (face, room) = if pocket.iter().any(|q| q[1] - m.shift.y > 15.0) {
                    let face = ys.fold(f64::NEG_INFINITY, f64::max);
                    (face, face - (30.0 + c))
                } else {
                    let face = ys.fold(f64::INFINITY, f64::min);
                    (face, -c - face)
                };
                assert!(room >= hook + p.fit.play - 1e-6, "{room} mm to bend into at {face}, for a {hook} mm nub");
            }
            let (lo, hi) = poly::bounds(&vec![m.cage.profiles("cage shell")]).unwrap();
            let (clo, chi) = poly::bounds(&fit.cavity).unwrap();
            assert!(
                (hi[1] - lo[1] - (chi[1] - clo[1] + 2.0 * wall)).abs() < 0.05,
                "the shell is {} tall",
                hi[1] - lo[1]
            );
        }
    }

    /// Behind a blank faceplate a device sits its room past the collar and the band the collar grips, and the plate has
    /// no opening. Pillars on the plate's back rest on its face, one just inside each corner, and the cage's openings
    /// span the room in front of it, which its front cables bend out through.
    #[test]
    fn a_device_behind_a_blank_faceplate() {
        let (p, room) = (Project::default(), 30.0);
        let fit = crate::Device::boxed([150.0, 30.0, 100.0]).fit(&p, Some(room)).unwrap();
        let m = fit.module(0.0, (-30.0, 30.0));
        let (t, band) = (p.build.plate, p.build.collar_depth + BAND);
        assert_eq!(fit.front, Front::Blank);
        assert!((fit.recess - (t + band + room)).abs() < 1e-9, "{}", fit.recess);
        assert!(m.nest.find("nest").is_empty());
        let face = -fit.recess - fit.face;
        let stops = m.collar.prisms("stops");
        assert_eq!(stops.len(), 4);
        for (lo, hi, outline) in stops {
            let (a, b) = poly::bounds(&vec![outline.to_vec()]).unwrap();
            let (x, y) = ((a[0] + b[0]) / 2.0, (a[1] + b[1]) / 2.0 - m.shift.y);
            let corner = (75.0 - x.abs()).max(0.0).hypot(if y < 15.0 { y } else { 30.0 - y });
            assert!((lo - face).abs() < 1e-9 && hi > -t && hi < 0.0, "from {lo} to {hi}");
            assert!(a[0] > -75.0 && b[0] < 75.0 && a[1] - m.shift.y > 0.0 && b[1] - m.shift.y < 30.0, "{a:?} {b:?}");
            assert!(corner < 2.0 * STOP, "{corner} mm from a corner");
        }
        let open = vents(&m, "side openings").flat_map(|v| v.openings.iter());
        let spans =
            open.map(|o| o.iter().fold((f64::INFINITY, f64::NEG_INFINITY), |(lo, hi), q| (lo.min(q[1]), hi.max(q[1]))));
        let front = -t - band;
        assert!(spans.into_iter().any(|(lo, hi)| lo < face && hi > front - 1.0), "no way out in front of the device");
    }

    /// On a tight panel the collar's top and bottom thin to what it leaves, so the cage still latches,
    /// but no thinner than COLLAR, which holds the nubs.
    #[test]
    fn a_tight_panel_thins_the_collar() {
        let p = Project::default();
        let fit = crate::Device::boxed([150.0, 30.0, 100.0]).fit(&p, None).unwrap();
        let cage = fit.height() + fit.latch.pads[0] + fit.latch.pads[1] + 2.0 * p.fit.collar;
        for (each, latched) in [(1.2, true), (0.9, false)] {
            let half = cage / 2.0 + each;
            let m = fit.module(0.0, (-half, half));
            assert_eq!(m.issue.is_none(), latched, "{each} mm round the cage");
            let (lo, hi) = poly::bounds(&m.outside).unwrap();
            assert!(!latched || ((lo[1] + half).abs() < 1e-6 && (hi[1] - half).abs() < 1e-6), "{lo:?} {hi:?}");
        }
    }

    /// Each tongue is as short as it can be without straining more than STRAIN at its root as the
    /// nub bends it, to a millimeter.
    #[test]
    fn the_tongues_are_as_short_as_their_strain_allows() {
        for hook in [0.5, 1.0, 1.2, 2.0] {
            let mut p = Project::default();
            p.fit.hook = hook;
            let fit = crate::Device::boxed([150.0, 30.0, 100.0]).fit(&p, None).unwrap();
            let bend = 1.5 * (hook + p.fit.play) * p.build.wall;
            for (free, root, _) in fit.latch.tongues.into_iter().flatten() {
                let reach = (root - free).abs() - (EDGE + p.fit.collar + NUB / 2.0);
                let strain = |reach: f64| bend / (reach * reach);
                assert!(strain(reach) <= STRAIN && strain(reach - 1.0) > STRAIN, "{hook}: {reach} mm");
            }
        }
    }

    /// The tongues' free ends reach about 9 mm past the collar, to squeeze them in there.
    #[test]
    fn the_tongues_reach_past_the_collar() {
        let p = Project::default();
        let fit = crate::Device::boxed([150.0, 30.0, 100.0]).fit(&p, None).unwrap();
        // (z, x): the slit's corner furthest along x from its root, the deepest point.
        let slits = fit.module(0.0, (-21.8, 21.8)).cage.profiles("tongue slits");
        let free = |slit: &Contour| {
            let root = slit.iter().min_by(|a, b| a[0].total_cmp(&b[0])).unwrap()[1];
            let far = slit.iter().map(|q| (q[1] - root).abs()).fold(0.0, f64::max);
            slit.iter().filter(|q| ((q[1] - root).abs() - far).abs() < 1e-9).map(|q| q[0]).fold(f64::INFINITY, f64::min)
        };
        let collar = -p.build.plate - p.build.collar_depth;
        assert!(slits.len() == 4 && slits.iter().all(|s| (8.0..=10.0).contains(&(collar - free(s)))));
    }

    /// A tongue's back edge is cut at 45 degrees only halfway to its root: the wall past that holds it on.
    #[test]
    fn the_tongues_back_edges_are_cut_halfway() {
        let p = Project::default();
        let fit = crate::Device::boxed([150.0, 30.0, 100.0]).fit(&p, None).unwrap();
        // (z, x), as in the wall.
        let slits = fit.module(0.0, (-21.8, 21.8)).cage.profiles("tongue slits");
        let z_free = -p.build.plate - p.build.collar_depth - REACH;
        let tongues = fit.latch.tongues.concat();
        assert_eq!(slits.len(), 4);
        for slit in &slits {
            let deepest = slit.iter().min_by(|a, b| a[0].total_cmp(&b[0])).unwrap();
            let (free, root, _) = tongues.iter().find(|t| slit.iter().any(|q| (q[1] - t.0).abs() < 1e-6)).unwrap();
            let half = (root - free).abs() / 2.0;
            assert!((z_free - deepest[0] - half).abs() < 1e-6, "{slit:?}: {half} deep");
            assert!(((deepest[1] - free).abs() - half).abs() <= SLIT + 1e-6, "{slit:?}: {half} along");
        }
    }

    /// Each tongue sits at an end of its wall, as far out as the device's top or underside runs flat,
    /// whatever air it rests on: here edges rounding over 8 mm leave both flat for 84 mm, a little more
    /// with the clearance.
    #[test]
    fn the_tongues_sit_at_the_ends_of_their_walls() {
        let device = crate::Device::from(extruded(poly::rounded_rect(-50.0, 0.0, 50.0, 30.0, 8.0)));
        for air in [0.0, 2.0, 3.0] {
            let mut p = Project::default();
            p.build.air = air;
            let fit = device.fit(&p, None).unwrap();
            assert_eq!(fit.module(0.0, (-30.0, 30.0)).issue, None);
            for (free, root, _) in fit.latch.tongues.into_iter().flatten() {
                let end = free.abs() + SLIT + p.build.wall;
                assert!((42.0..=44.0).contains(&end) && root.abs() < free.abs(), "{air}: flat to {end}");
            }
        }
    }

    /// Between the tongues, the openings in the top and bottom walls reach the band the collar grips,
    /// and further back the outermost reach on out beside the tongues to a COLUMN short of the sides'
    /// openings, which stop half a rail short of the top, or in the bottom wall to where the floor
    /// stops running flat, short of the ledges, and without them a COLUMN short of the sides' openings,
    /// which start just above the floor: none stop short under the tongues. They take most of that, bridge at most BRIDGE, and keep a
    /// rail, less the overlap, clear of all the latch cuts and adds.
    #[test]
    fn the_openings_pass_beside_the_tongues() {
        for air in [0.0, 2.0] {
            let mut p = Project::default();
            p.build.air = air;
            let fit = crate::Device::boxed([150.0, 30.0, 100.0]).fit(&p, None).unwrap();
            let m = fit.module(0.0, (-30.0, 30.0));
            let (lo, hi) = poly::bounds(&fit.cavity).unwrap();
            let front = -p.build.plate - p.build.collar_depth - BAND;
            // What the latch cuts and adds, as boxes in the walls' (z, x), with the y of their middle.
            let latch = ["tongue slits", "tongue pocket", "latch pad", "latch hole"].map(|n| m.cage.find(n)).concat();
            let latch: Vec<(f64, Shapes)> = latch
                .into_iter()
                .filter_map(|s| match s {
                    Solid::Extrude { axis, contours, lo, hi } => {
                        let (a, b) = poly::bounds(&vec![contours.clone()]).unwrap();
                        let (y, [z0, x0, z1, x1]) = match axis {
                            Axis::Y => ((lo + hi) / 2.0, [a[0], a[1], b[0], b[1]]),
                            Axis::Z => ((a[1] + b[1]) / 2.0, [*lo, a[0], *hi, b[0]]),
                            Axis::X => ((a[0] + b[0]) / 2.0, [a[1], *lo, b[1], *hi]),
                        };
                        Some((y, poly::rect(z0, x0, z1, x1)))
                    }
                    Solid::Mesh { vertices, .. } => {
                        let (lo, hi) = mesh::bounds(vertices.iter().map(|&v| DVec3::from(v)));
                        Some(((lo.y + hi.y) / 2.0, poly::rect(lo.z, lo.x, hi.z, hi.x)))
                    }
                    _ => None,
                })
                .collect();
            for (name, top) in [("top openings", true), ("bottom openings", false)] {
                let vent = vents(&m, name).next().unwrap_or_else(|| panic!("{air}: no {name}"));
                let (a, b) = poly::bounds(&vec![vent.openings.clone()]).unwrap();
                let reach = |o: &Contour| o.iter().map(|q| q[1]).fold(f64::NEG_INFINITY, f64::max);
                let all = vent.openings.iter();
                assert!(all.clone().all(|o| (reach(o) - front).abs() < 1e-6), "{air} {name}: one stops short");
                // In (z, x), as the latch's cuts are.
                let openings: Vec<Contour> =
                    all.map(|o| o.iter().map(|&[s, z]| [z, on(vent, s).x]).collect()).collect();
                let (x0, x1) = span(on(vent, a[0]).x, on(vent, b[0]).x);
                if top {
                    let ends = [x0 - lo[0] - (COLUMN - RAIL / 2.0), hi[0] - (COLUMN - RAIL / 2.0) - x1];
                    assert!(ends.iter().all(|e| (0.0..1.0).contains(e)), "{air} {name}: {ends:?} short of the sides");
                } else if air > 0.0 {
                    let floor = |s: f64| (on(vent, s).y - m.shift.y - lo[1]).abs() <= STRAIGHT;
                    let (s0, s1) = (a[0] - ROUND, b[0] + ROUND);
                    assert!(floor(s0) && floor(s1) && !floor(s0 - 1.0) && !floor(s1 + 1.0), "{air} {name}");
                } else {
                    let round = vent.corners[vent.corners.len() - 1].0;
                    // Up the right side, then down the left, round to the floor's start.
                    let mut sides: Vec<_> =
                        vents(&m, "side openings").map(|v| poly::bounds(&vec![v.openings.clone()]).unwrap()).collect();
                    sides.sort_by(|p, q| p.0[0].total_cmp(&q.0[0]));
                    let [(right, _), (_, left)] = sides[..] else { panic!("{name}: {sides:?}") };
                    let gaps = [right[0] - b[0], a[0] + round - left[0]];
                    assert!(gaps.iter().all(|g| (g - COLUMN).abs() < 1e-6), "{name}: {gaps:?}");
                }
                let wall = (b[0] - a[0]) * (front - fit.z_struts - RAIL);
                let bridge = |o: &Contour| {
                    let top = o.iter().map(|q| q[0]).fold(f64::NEG_INFINITY, f64::max);
                    let xs = o.iter().filter(|q| (q[0] - top).abs() < 1e-9).map(|q| q[1]);
                    xs.clone().fold(f64::NEG_INFINITY, f64::max) - xs.fold(f64::INFINITY, f64::min)
                };
                assert!(openings.iter().all(|o| bridge(o) <= BRIDGE + 1e-6), "{air} {name}: a longer bridge");
                let share = poly::union_polygons(vent.openings.clone()).area() / wall;
                assert!(share > 0.6, "{air} {name}: {share} open");
                let near = poly::offset(&poly::union_polygons(openings), RAIL - EPS - 0.01);
                for (y, part) in latch.iter().filter(|(y, _)| (*y > m.shift.y + 15.0) == top) {
                    let hits = poly::intersection(&near, part).area();
                    assert!(hits < 1e-6, "{air} {name}: within a rail of the latch at y {y}");
                }
            }
        }
    }

    /// Under the ledges the wall runs higher than the bottom tongues, which run with the wall over the
    /// air. However short they are, the pad under each runs on past its root until it meets that wall,
    /// so the root holds on to wall all along.
    #[test]
    fn the_tongues_roots_hold() {
        for hook in [0.5, 1.0, 2.0] {
            let mut p = Project::default();
            p.fit.hook = hook;
            let fit = crate::Device::boxed([150.0, 30.0, 100.0]).fit(&p, None).unwrap();
            let m = fit.module(0.0, (-30.0, 30.0));
            let pads = meshes(&m.cage, "latch pad").into_iter().filter(|v| v[0][1] < m.shift.y);
            let span = |v: Vec<[f64; 3]>| {
                v.iter().fold((f64::INFINITY, f64::NEG_INFINITY), |(a, b), q| (a.min(q[0]), b.max(q[0])))
            };
            let pads: Vec<_> = pads.map(span).collect();
            let (lo, _) = poly::bounds(&fit.cavity).unwrap();
            // The tongue's outermost layer, in the device frame.
            let y = lo[1] - p.build.wall - fit.latch.pads[0] + 0.1;
            let wall = |x: f64| {
                let probe = poly::rect(x - 0.01, y - 0.01, x + 0.01, y + 0.01);
                pads.iter().any(|&(a, b)| (a..=b).contains(&x)) || poly::difference(&probe, &fit.outer).area() < 1e-9
            };
            for (_, root, _) in fit.latch.tongues[0] {
                let d = -root.signum();
                assert!((0..=200).map(|i| root + d * i as f64 * 0.1).all(wall), "{hook}: a root hangs on less");
            }
        }
    }

    /// Under a box, the gussets are as tall as the air, up to the rail along the back.
    #[test]
    fn gussets_grow_with_the_air() {
        let device = crate::Device::boxed([150.0, 30.0, 100.0]);
        for air in [1.0, 3.0, 6.0, 9.0] {
            let mut p = Project::default();
            p.build.air = air;
            let fit = device.fit(&p, None).unwrap();
            let below: Vec<f64> = fit.gussets.iter().filter(|g| g.2 > 0.0).map(|g| g.3).collect();
            let n = fit.struts.len();
            assert!(below.len() == n && below.iter().all(|h| (h - air.min(RAIL)).abs() < 0.05), "{air}: {below:?}");
        }
    }

    /// Air under the device makes its cage that much taller; the device rests on ledges instead.
    #[test]
    fn air_under_the_device_raises_the_cage() {
        let device = crate::Device::boxed([150.0, 30.0, 100.0]);
        let mut p = Project::default();
        let (tall, air) = (device.fit(&p, None).unwrap().height(), p.build.air);
        p.build.air = 0.0;
        assert!((tall - device.fit(&p, None).unwrap().height() - air).abs() < 0.05, "{tall}");
    }

    /// Round a device's rounded edges, or where a bevel meets a side, the walls turn, and a bridge that
    /// turned with them would sag: the openings follow the walls round, but only bridge where a wall
    /// runs straight. Everywhere else, followed along the wall, their tops lean down no further from
    /// vertical than the overhang the user allows.
    #[test]
    fn the_openings_lean_no_further_than_the_overhang() {
        // Beveled underneath, like the UCG-Fiber, its top edges rounded.
        let mut outline = vec![[-85.0, 0.0], [85.0, 0.0], [100.0, 15.0]];
        outline.extend(poly::arc([88.0, 28.0], 12.0, 0.0, 90.0));
        outline.extend(poly::arc([-88.0, 28.0], 12.0, 90.0, 90.0));
        outline.push([-100.0, 15.0]);
        let device = crate::Device::from(extruded(outline));
        for overhang in [45.0, 60.0] {
            let mut p = Project::default();
            p.build.overhang = overhang;
            let m = device.fit(&p, None).unwrap().module(0.0, (-30.0, 30.0));
            let lean = 1.0 / overhang.to_radians().tan();
            let mut round = 0;
            for vent in &m.vents {
                // The bridge's stretch of wall runs straight.
                let [b0, b1] = vent.bridge;
                let (p0, p1) = (on(vent, b0), on(vent, b1));
                let off = (0..=50)
                    .map(|i| on(vent, b0 + (b1 - b0) * i as f64 / 50.0))
                    .map(|q| (q - p0).perp_dot((p1 - p0).normalize()).abs());
                assert!(off.fold(0.0, f64::max) <= STRAIGHT + 1e-6, "{}: its bridge's wall bends", vent.name);
                for o in &vent.openings {
                    round += o.iter().any(|q| q[0] < b0 - RAIL || q[0] > b1 + RAIL) as usize;
                    // Counterclockwise, with the opening to their left: a top edge runs back along the wall.
                    let ccw = o.area() > 0.0;
                    for (a, b) in o.iter().zip(o.iter().cycle().skip(1)) {
                        let (ds, dv) = (b[0] - a[0], b[1] - a[1]);
                        if (ds < 0.0) != ccw || ds.abs() < 1e-9 {
                            continue;
                        }
                        let flat = dv.abs() / ds.abs() < lean - 1e-6;
                        let bridged = a[0].min(b[0]) >= b0 - 1e-6 && a[0].max(b[0]) <= b1 + 1e-6 && dv.abs() < 1e-9;
                        assert!(
                            !flat || bridged,
                            "{} at {overhang}°: from {a:?} to {b:?}, off the bridge {:?}",
                            vent.name,
                            vent.bridge
                        );
                    }
                }
            }
            assert!(round >= 2, "at {overhang}°, {round} openings follow the walls round");
        }
    }

    /// Next to the tongues, what's left of a wall can be a hair wider or taller than a rail: still
    /// an opening, with chamfers that fit.
    #[test]
    fn a_wall_just_wider_than_a_rail_opens() {
        for (u1, v1) in [(RAIL + 0.01, 50.0), (50.0, RAIL + 0.01)] {
            assert!(!openings(0.0, u1, 0.0, v1, [0.0, u1], v1, [0.0, u1], 1.0).is_empty());
        }
    }

    /// However a device's edges round, each opening is cut in one closed piece round the wall's
    /// corners: every edge of its surface has a twin running the other way.
    #[test]
    fn the_openings_are_closed_round_any_corner() {
        for (r, air, overhang) in [(2.0, 0.0, 55.0), (5.0, 3.0, 55.0), (8.0, 2.0, 60.0), (12.0, 3.0, 45.0)] {
            let mut outline = vec![[-90.0 + r, 0.0], [90.0 - r, 0.0]];
            outline.extend(poly::arc([90.0 - r, r], r, 270.0, 90.0));
            outline.extend(poly::arc([90.0 - r, 32.0 - r], r, 0.0, 90.0));
            outline.extend(poly::arc([-90.0 + r, 32.0 - r], r, 90.0, 90.0));
            outline.extend(poly::arc([-90.0 + r, r], r, 180.0, 90.0));
            let mut p = Project::default();
            (p.build.air, p.build.overhang) = (air, overhang);
            let m = crate::Device::from(extruded(outline)).fit(&p, None).unwrap().module(0.0, (-40.0, 40.0));
            for name in ["top openings", "bottom openings", "side openings"] {
                for piece in m.cage.find(name) {
                    let Solid::Mesh { vertices, triangles } = piece else { continue };
                    let mut directed = std::collections::HashMap::new();
                    for t in triangles {
                        assert!(t.iter().all(|&i| (i as usize) < vertices.len()), "{r} {name}: {t:?}");
                        for k in 0..3 {
                            *directed.entry((t[k], t[(k + 1) % 3])).or_insert(0) += 1;
                        }
                    }
                    let open =
                        directed.iter().filter(|&(&(a, b), &n)| n != 1 || directed.get(&(b, a)) != Some(&1)).count();
                    let bad: Vec<_> = directed
                        .iter()
                        .filter(|&(&(a, b), &n)| n != 1 || directed.get(&(b, a)) != Some(&1))
                        .map(|(&(a, b), &n)| {
                            (
                                n,
                                vertices[a as usize].map(|v| (v * 100.0).round() / 100.0),
                                vertices[b as usize].map(|v| (v * 100.0).round() / 100.0),
                            )
                        })
                        .collect();
                    assert_eq!(open, 0, "{r} {air} {overhang} {name}: {open} open edges {bad:?}");
                }
            }
        }
    }

    /// Past the bridge an opening's top leans down at the overhang, all the way to its side: no
    /// 45-degree chamfer meets the lean in a kink.
    #[test]
    fn past_the_bridge_the_tops_lean_without_a_kink() {
        let (v1, lean) = (60.0, 0.7);
        for (u1, bridge) in [(60.0, [0.0, 44.0]), (60.0, [0.0, 20.0]), (120.0, [30.0, 80.0]), (90.0, [0.0, 90.0])] {
            for o in openings(0.0, u1, 0.0, v1, [0.0, u1], v1, bridge, lean) {
                for k in 0..o.len() {
                    let (p, q) = (o[k], o[(k + 1) % o.len()]);
                    let past = [p, q].iter().all(|e| e[0] < bridge[0] - 1e-6 || e[0] > bridge[1] + 1e-6);
                    let high = p[1].min(q[1]) > v1 / 2.0;
                    let slope = (q[1] - p[1]).abs() / (q[0] - p[0]).abs();
                    assert!(!(past && high && (slope - 1.0).abs() < 1e-6), "{u1} {bridge:?}: {p:?} {q:?} in {o:?}");
                }
            }
        }
    }

    /// Where an opening doesn't reach out, the two points of its knee are one, though rounding can
    /// leave them a hair apart: an edge that short means nothing, and only gives the kernel trouble.
    #[test]
    fn openings_have_no_points_a_hair_apart() {
        for i in 0..2000 {
            let (u0, u1) = (0.1 * i as f64, 40.0 + 0.37 * i as f64);
            for o in openings(u0, u1, 0.0, 50.0, [u0, u1], 20.0, [u0, u1], 1.0) {
                let gap = |k: usize| (o[k][0] - o[(k + 1) % o.len()][0]).hypot(o[k][1] - o[(k + 1) % o.len()][1]);
                assert!((0..o.len()).all(|k| gap(k) >= poly::RESOLUTION), "{u0} {u1}: {o:?}");
            }
        }
    }

    /// Openings side by side in the top and bottom walls stand a COLUMN apart. With air under the
    /// device, each column in the floor stands a ridge RIDGE wide and half as tall into it, from the
    /// cage's back to its front, meeting the floor in fillets.
    #[test]
    fn the_columns_between_openings_are_wide_and_ridged() {
        for air in [0.0, 2.0, 3.0] {
            let mut p = Project::default();
            p.build.air = air;
            let fit = crate::Device::boxed([300.0, 30.0, 100.0]).fit(&p, None).unwrap();
            let m = fit.module(0.0, (-30.0, 30.0));
            // Each ridge: its outline's bounds, from where to where along the cage, and how wide it is
            // a quarter of a fillet above the floor.
            let floor = poly::bounds(&fit.cavity).unwrap().0[1] + m.shift.y;
            let ridges: Vec<_> = m
                .cage
                .find("floor ridges")
                .into_iter()
                .map(|r| match r {
                    Solid::Extrude { axis: Axis::Z, contours, lo, hi } => {
                        let (a, b) = poly::bounds(&vec![contours.clone()]).unwrap();
                        let line = poly::rect(a[0], floor + ROUND / 4.0 - 0.01, b[0], floor + ROUND / 4.0 + 0.01);
                        ((a, b), *lo, *hi, poly::intersection(&vec![contours.clone()], &line).area() / 0.02)
                    }
                    _ => panic!("a ridge is a prism"),
                })
                .collect();
            for name in ["top openings", "bottom openings"] {
                let vent = vents(&m, name).next().unwrap();
                let mut spans: Vec<(P2, P2)> =
                    vent.openings.iter().map(|o| poly::bounds(&vec![vec![o.clone()]]).unwrap()).collect();
                spans.sort_by(|p, q| p.0[0].total_cmp(&q.0[0]));
                assert!(spans.len() > 1, "{air} {name}: one opening");
                for w in spans.windows(2) {
                    let [(_, hi), (next_lo, _)] = w else { unreachable!() };
                    assert!((next_lo[0] - hi[0] - COLUMN).abs() < 1e-6, "{air} {name}: {w:?}");
                    if name == "top openings" || air == 0.0 {
                        continue;
                    }
                    let mid = on(vent, (hi[0] + next_lo[0]) / 2.0).x;
                    let ridge = ridges.iter().find(|((a, b), ..)| a[0] < mid && b[0] > mid);
                    let Some(((a, b), z0, z1, foot)) = ridge else { panic!("{air}: no ridge at {mid}") };
                    let fillet = ROUND - (ROUND * ROUND - (0.75 * ROUND).powi(2)).sqrt();
                    assert!((foot - RIDGE - 2.0 * fillet).abs() < 0.2, "{air}: {foot} wide at its foot");
                    assert!((b[1] - floor - air / 2.0).abs() < 1e-6 && a[1] < floor, "{air}: {a:?} {b:?}");
                    assert!(
                        (b[0] - a[0] - RIDGE - 2.0 * ROUND).abs() < 1e-6 && (a[0] + b[0] - 2.0 * mid).abs() < 0.01,
                        "{air}: {a:?} {b:?} round {mid}"
                    );
                    assert!(*z0 == fit.z_back && *z1 == -p.build.plate, "{air}: {z0} {z1}");
                }
                if name == "bottom openings" {
                    assert_eq!(ridges.len(), if air > 0.0 { spans.len() - 1 } else { 0 }, "{air}");
                }
            }
        }
    }

    /// A flat back has nothing standing out behind it: struts as near SPACING apart as the width
    /// allows, each floor to ceiling, meeting both in fillets FILLET big.
    #[test]
    fn a_flat_back_gets_evenly_spaced_struts_in_fillets() {
        let widths = [(60.0, 2), (110.0, 2), (150.0, 3), (215.0, 4), (230.0, 5)];
        for ((width, n), air) in widths.into_iter().flat_map(|w| [0.0, 2.0, 3.0].map(|a| (w, a))) {
            let mut p = Project::default();
            p.build.air = air;
            let fit = crate::Device::boxed([width, 30.0, 100.0]).fit(&p, None).unwrap();
            assert_eq!(fit.struts.len(), n, "{width}");
            // Evenly, in the middle of equal shares of the width. On a narrow device a strut's fillets
            // also fill the cavity's corners nearby, so we measure each across a line through it: the
            // pieces of the strut there, and how wide the one round `mid` is.
            let (lo, hi) = poly::bounds(&fit.cavity).unwrap();
            let line =
                |strut: &Shapes, y: f64| poly::intersection(strut, &poly::rect(lo[0], y - 0.01, hi[0], y + 0.01));
            let wide = |strut: &Shapes, y: f64, mid: f64| {
                let pieces = line(strut, y).into_iter().map(|p| vec![p]);
                let round = pieces.filter(|p| poly::bounds(p).is_some_and(|(a, b)| a[0] < mid && b[0] > mid));
                round.map(|p| p.area()).sum::<f64>() / 0.02
            };
            let middle = |strut: &Shapes| {
                let (a, b) = poly::bounds(&line(strut, (lo[1] + hi[1]) / 2.0)).unwrap();
                (a[0] + b[0]) / 2.0
            };
            let mut mids: Vec<f64> = fit.struts.iter().map(|s| middle(&vec![s.clone()])).collect();
            mids.sort_by(f64::total_cmp);
            let share = (hi[0] - lo[0]) / n as f64;
            assert!(
                mids.iter().enumerate().all(|(i, m)| (m - lo[0] - (i as f64 + 0.5) * share).abs() < 0.01),
                "{width}: {mids:?}"
            );
            for strut in fit.struts.iter().map(|s| vec![s.clone()]) {
                // From the walls where the strut stands: fairing leaves them a few hundredths off level.
                let mid = middle(&strut);
                // With air, the outermost run into the ledges' chamfers along the floor instead.
                let outer = air > 0.0 && [mids[0], mids[n - 1]].iter().any(|m| (m - mid).abs() < 0.01);
                let walls = poly::crossings(&fit.cavity.concat(), 1, mid);
                let [floor, ceiling] = [f64::min, f64::max].map(|m| walls.iter().map(|w| w.0).reduce(m).unwrap());
                for h in [0.5, FILLET / 2.0, FILLET, (ceiling - floor) / 2.0] {
                    let fillet =
                        if h < FILLET { FILLET - (FILLET * FILLET - (FILLET - h).powi(2)).sqrt() } else { 0.0 };
                    for y in [floor + h, ceiling - h].into_iter().skip(outer as usize) {
                        let width = wide(&strut, y, mid);
                        assert!(
                            (width - STRUT - 2.0 * fillet).abs() < 0.1,
                            "{n} struts, {air}: {width} mm wide {h} mm from a wall"
                        );
                    }
                }
            }
        }
    }

    /// However close a strut stands to a ledge's chamfer, it meets the walls round it smoothly: no
    /// notch sharper than its fillets within their reach, where its fillet and the chamfer meet.
    #[test]
    fn the_struts_meet_the_walls_smoothly() {
        for (width, air) in [(212.8, 3.0), (150.0, 2.0), (110.0, 3.0), (230.0, 2.0)] {
            let mut p = Project::default();
            p.build.air = air;
            let fit = crate::Device::boxed([width, 30.0, 100.0]).fit(&p, None).unwrap();
            let (lo, hi) = poly::bounds(&fit.cavity).unwrap();
            let walls =
                poly::difference(&poly::rect(lo[0] - 10.0, lo[1] - 10.0, hi[0] + 10.0, hi[1] + 10.0), &fit.cavity);
            let solid = poly::union(&walls, &fit.struts);
            let notches = poly::difference(&poly::closed(&solid, 0.9 * FILLET), &solid);
            for strut in &fit.struts {
                let (a, b) = poly::bounds(&vec![strut.clone()]).unwrap();
                let mid = (a[0] + b[0]) / 2.0;
                let reach =
                    poly::rect(mid - STRUT / 2.0 - 2.0 * FILLET, lo[1], mid + STRUT / 2.0 + 2.0 * FILLET, hi[1]);
                // Arcs, drawn in straight pieces, leave slivers of a few hundredths.
                let pieces = poly::intersection(&notches, &reach).into_iter().map(|n| n.area());
                let area = pieces.fold(0.0, f64::max);
                assert!(area < 0.1, "{width} {air}: a notch of {area} mm² by the strut at {mid}");
            }
        }
    }

    /// From the struts' fronts gussets run up the walls, at 45 degrees, as far as the device leaves
    /// room and never nearer it than its clearance: under a box, as tall as the air under it, over it
    /// none, as its back edges are square. Over a device whose back edges round over 6 mm, some. They
    /// stay on the rail along the back.
    #[test]
    fn gussets_run_up_the_walls_where_the_device_leaves_room() {
        let round = |d: f64| 6.0 - (36.0 - (6.0 - d).powi(2)).sqrt();
        let mut rings = vec![(0.0, 75.0, 15.0)];
        rings.extend((0..=12).map(|i| 6.0 - i as f64 / 2.0).map(|e| (e - 60.0, 75.0 - round(e), 15.0 - round(e))));
        for (tris, top) in [(crate::Device::boxed([150.0, 30.0, 100.0]).tris, false), (lofted(&rings), true)] {
            let p = Project::default();
            let fit = crate::Device::from(tris.clone()).fit(&p, None).unwrap();
            let (below, above): (Vec<_>, Vec<_>) = fit.gussets.iter().partition(|g| g.2 > 0.0);
            let n = fit.struts.len();
            assert!(below.len() == n && below.iter().all(|g| g.3 >= p.build.air - 0.01), "{:?}", fit.gussets);
            assert!(fit.gussets.iter().all(|g| g.3 <= RAIL + 1e-9), "{:?}", fit.gussets);
            assert_eq!(above.len(), if top { n } else { 0 }, "{:?}", fit.gussets);
            let rear = -fit.size.z - fit.recess;
            for (shape, face, s, h) in &fit.gussets {
                for u in (0..=(h / 0.05) as usize).map(|i| i as f64 * 0.05) {
                    let base = face - s * EPS;
                    let k = (h - u + EPS) / (h + EPS);
                    let section = vec![vec![shape.iter().map(|q| [q[0], base + (q[1] - base) * k]).collect()]];
                    let device = poly::offset(&rear_silhouettes(&tris, &[fit.z_struts + u - rear])[0], p.fit.cage);
                    let hits = poly::intersection(&section, &device).area();
                    assert!(hits < 1e-3, "{top}: {hits} mm² of a gusset {h} tall in the device {u} mm in");
                }
            }
        }
    }

    /// A face just off the grid we look for it on, behind an LED, with edges rounding back as gently
    /// as a big radius does at first, 5 mm in the first half millimeter: the opening at the front is
    /// the face's own outline, not a cut through the rounding a step behind it.
    #[test]
    fn the_opening_at_the_front_is_the_face() {
        let mut tris = lofted(&[(-0.23, 40.0, 10.0), (-0.73, 45.0, 10.0), (-60.0, 45.0, 10.0)]);
        let led = crate::Device::boxed([2.0, 2.0, 0.23]).tris.into_iter().map(|t| t.map(|p| p + DVec3::Y * 4.0));
        tris.extend(led);
        let p = Project::default();
        let fit = crate::Device::from(tris).fit(&p, None).unwrap();
        let (lo, hi) = poly::bounds(&fit.nest[0].1).unwrap();
        let extra = (hi[0] - lo[0] - 80.0) / 2.0 - p.fit.nest;
        assert!((0.0..=FAIR + 1e-6).contains(&extra), "{extra} mm wider than the face and its clearance");
    }

    /// A device whose front edges round over, as most do, here with a 6 mm radius. Running straight
    /// between looks half a millimeter apart, the surface cut a millimeter into the rounding near
    /// the front.
    #[test]
    fn the_nest_follows_a_rounded_front() {
        let round = |d: f64| (36.0 - (6.0 - d.min(6.0)).powi(2)).sqrt();
        let depths = [0.0, 0.01, 0.03, 0.06, 0.1, 0.16, 0.25, 0.4, 0.6, 0.9, 1.3, 1.8, 2.4, 3.2, 4.2, 6.0, 60.0];
        follows(&lofted(&depths.map(|d| (-d, 44.0 + round(d), 9.0 + round(d)))));
    }

    /// A device that runs straight, then steps out: the surface runs straight along it, so the
    /// nest only slopes where the device does.
    #[test]
    fn the_nest_follows_a_step() {
        follows(&lofted(&[(0.0, 40.0, 10.0), (-1.3, 40.0, 10.0), (-1.35, 42.0, 10.0), (-60.0, 42.0, 10.0)]));
    }

    /// A device whose faceted sides are drafted, as the fronts of many are, between a flat top and
    /// bottom. Each look lies straight between its neighbors, so the nest needs no more than the
    /// regular ones; and each is the device's outline grown evenly, not simplified its own way,
    /// which on such a slope left the surface creased every few tenths of a millimeter.
    #[test]
    fn the_nest_runs_straight_along_a_drafted_side() {
        let oval = |w: f64| {
            (0..48).map(|i| i as f64 * TAU / 48.0).map(|a| [w * a.cos(), 20.0 * a.sin().clamp(-0.5, 0.5)]).collect()
        };
        let tris = mesh::loft(&[(0.0, oval(40.0)), (-5.0, oval(54.0)), (-60.0, oval(54.0))]);
        follows(&tris);
        let p = Project::default();
        let fit = crate::Device::from(tris.clone()).fit(&p, None).unwrap();
        assert_eq!(fit.nest.len(), samples(p.build.plate, NEST_STEP).len());
        for (z, look) in &fit.nest {
            let even = poly::offset(&front_silhouettes(&tris, &[-z - fit.recess])[0], p.fit.nest + FAIR);
            let off = |a: &Shapes, b: &Shapes| {
                a.iter().flatten().flatten().map(|&q| poly::distance(b, q)).fold(0.0, f64::max)
            };
            let off = off(look, &even).max(off(&even, look));
            assert!(off < 1.5 * poly::RESOLUTION, "the look {z} mm in is {off} mm off the outline grown evenly");
        }
    }

    /// All through the plate, the nest clears the device by most of its clearance, and by little
    /// more than that.
    fn follows(tris: &[Tri]) {
        let p = Project::default();
        let fit = crate::Device::from(tris.to_vec()).fit(&p, None).unwrap();
        let m = fit.module(0.0, (-30.0, 30.0));
        let depths: Vec<f64> = (0..=(p.build.plate / 0.02).round() as usize).map(|i| i as f64 * 0.02).collect();
        let silhouettes = front_silhouettes(tris, &depths.iter().map(|d| d - fit.recess).collect::<Vec<_>>());
        for (d, s) in depths.iter().zip(&silhouettes) {
            let device = vec![poly::translate(s, m.shift.x, m.shift.y)];
            let nest = crate::terrain::section(&m.nest, -d - 1e-9);
            let hits = poly::difference(&poly::offset(&device, p.fit.nest - FOLLOW), &nest).area();
            assert!(hits < 1e-3, "{hits} mm² of the device and most of its clearance outside the nest {d} mm in");
            let slack = poly::difference(&nest, &poly::offset(&device, p.fit.nest + FAIR + FOLLOW)).area();
            assert!(slack < 1e-3, "{slack} mm² of the nest {d} mm in further from the device than it need be");
        }
    }

    /// A module's openings named `name`.
    fn vents<'a>(m: &'a Module, name: &'a str) -> impl Iterator<Item = &'a Vent> {
        m.vents.iter().filter(move |v| v.name == name)
    }

    /// Where on the cavity's outline `s` along it is, round and round, in the rack frame.
    fn on(vent: &Vent, s: f64) -> DVec2 {
        let c = &vent.corners;
        let s = s.rem_euclid(c[c.len() - 1].0);
        let i = c.windows(2).position(|w| s <= w[1].0).unwrap_or(c.len() - 2);
        let (a, b) = (DVec2::from(c[i].1), DVec2::from(c[i + 1].1));
        a.lerp(b, (s - c[i].0) / (c[i + 1].0 - c[i].0))
    }

    /// The vertices of the meshes labeled `name`.
    fn meshes(s: &Solid, name: &str) -> Vec<Vec<[f64; 3]>> {
        let meshes = s.find(name).into_iter().filter_map(|m| match m {
            Solid::Mesh { vertices, .. } => Some(vertices.clone()),
            _ => None,
        });
        meshes.collect()
    }

    /// The convex hull of points, counterclockwise.
    fn hull(mut points: Vec<poly::P2>) -> Contour {
        points.sort_by(|a, b| a[0].total_cmp(&b[0]).then(a[1].total_cmp(&b[1])));
        let turn =
            |a: poly::P2, b: poly::P2, c: poly::P2| (b[0] - a[0]) * (c[1] - a[1]) - (b[1] - a[1]) * (c[0] - a[0]);
        let chain = |points: &mut dyn Iterator<Item = poly::P2>| {
            let mut h: Contour = Vec::new();
            for p in points {
                while h.len() >= 2 && turn(h[h.len() - 2], h[h.len() - 1], p) <= 1e-12 {
                    h.pop();
                }
                h.push(p);
            }
            h.pop();
            h
        };
        let lower = chain(&mut points.clone().into_iter());
        [lower, chain(&mut points.into_iter().rev())].concat()
    }

    /// A device lofted through rectangles, each `(z, half its width, half its height)`, front first.
    fn lofted(rings: &[(f64, f64, f64)]) -> Vec<Tri> {
        let rings: Vec<_> = rings.iter().map(|&(z, w, h)| (z, poly::rounded_rect(-w, -h, w, h, 0.0))).collect();
        mesh::loft(&rings)
    }
}
