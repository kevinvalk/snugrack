//! The rack: rows from the top down, each a faceplate with its devices side by side, and a cage
//! per device. Split, each device module and each ear prints as its own piece, bolted together on
//! the back.

use std::collections::HashMap;
use std::rc::Rc;

use glam::{DMat4, DVec3};
use serde::Serialize;
use tsify::Tsify;

use crate::Device;
use crate::csg::Solid;
use crate::mount::{DeviceFit, EPS, Module, ROUND};
use crate::poly;
use crate::project::{Align, Panel, Project, RackWidth, Side, U};

const PANEL_GAP: f64 = 0.8;
/// How much the faceplate's corners are rounded at the row's ends.
const CORNER: f64 = 1.5;
/// How close anything behind the panel comes to a rail.
const RAIL_GAP: f64 = 2.0;
/// EIA-310 holes we use, from the bottom of each unit (the middle one at 22.225 is optional).
const EAR_HOLES: [f64; 2] = [6.35, 38.1];
const EAR_HOLE: f64 = 6.6;
const EAR_SLOT: f64 = 3.0;
/// Neighboring pieces bolt together with M3 screws, 8 to 12 mm long, and M3 nuts, square or hex.
/// The screws go in from the right-hand piece, through a rib with room for a hex key behind it. On
/// the left-hand piece the rib is a solid block that runs on into the device's collar, with a trap
/// for each nut: a slot down from the block's back end, which the nut drops into and a thin hex key
/// pushes down until it sits on the screw's axis, where it can't turn. Three screws: one in the
/// middle close to the plate, to pull the seam shut, and two high and low at the back end, where
/// they best keep the row straight. A tongue along the seam keeps the faces flush and the crack
/// closed.
const RIB: f64 = 4.0;
const RIB_DEPTH: f64 = 40.0;
/// A wedge where a rib meets the plate, on its inside: that corner is where it would crack. It
/// reaches GUSSET across the plate and RISE up the rib, 60 degrees, which takes the rib's bending to
/// where the back screws' lever is shorter. The middle screw goes through it, in a hole wide enough
/// for a flanged button head (ISO 7380-2, 6.9 mm) and the key, with its mouth chamfered.
const GUSSET: f64 = 10.0;
const RISE: f64 = GUSSET * 1.732;
const CLEAR: f64 = 8.0;
const CHAMFER: f64 = 1.0;
/// Where the screws go, in holes BOLT wide: the middle one this far behind the plate, the two at
/// the back end this far in front of it and in from the panel's top and bottom edges.
const SCREW_FROM_PLATE: f64 = 6.0;
const SCREW_FROM_BACK: f64 = 6.0;
const SCREW_FROM_EDGE: f64 = 9.0;
const BOLT: f64 = 3.4;
/// The seam's tongue: how far it reaches past the joint face, and the gap its groove leaves.
const TONGUE: f64 = 1.5;
const TONGUE_GAP: f64 = 0.2;
/// The longest screw's length under its head. The nuts: across their flats, a hex nut's across its
/// corners, the thicker one's thickness, and the gap their trap leaves them all around.
const SCREW: f64 = 12.0;
const NUT: f64 = 5.5;
const NUT_CORNERS: f64 = 6.35;
const NUT_T: f64 = 2.4;
const NUT_GAP: f64 = 0.2;
/// The nut's trap stands this far in from the joint face, which the nut pulls against: thin enough
/// that the shortest screw's tip gets through a hex nut, and three lines thick.
const NUT_WALL: f64 = 1.2;
/// A screw's hole runs this far past its tip, for a screw a little long.
const HOLE_PAST: f64 = 0.4;
/// From the joint face to the collar's inside, where the cage slides: past the longest screw's hole,
/// 0.8 mm of wall.
const BLOCK: f64 = SCREW - RIB + HOLE_PAST + 0.8;
/// Room between a collar and the next one in a row printed whole, or the end of a plain row.
const MARGIN: f64 = 8.0;
/// Room between a collar and the rib a screw goes in through: an M3x12 with its head is 15 mm
/// long, and a standard 2.5 mm L-key's short leg about 18 (ISO 2936).
const SCREW_ROOM: f64 = 20.0;
/// How much taller than its units a device with its own ears may be: models have feet, and makers round.
const UNIT_SLACK: f64 = 1.0;

#[derive(Clone, Debug, Serialize, Tsify)]
#[serde(rename_all = "camelCase")]
pub struct Assembly {
    pub parts: Vec<Part>,
    pub warnings: Vec<Warning>,
    pub rows: Vec<RowReport>,
    pub panel: Panel,
}

#[derive(Clone, Debug, Serialize, Tsify)]
pub struct Part {
    pub row: usize,
    pub kind: PartKind,
    /// The device a cage or a split faceplate piece belongs to, by its index in the row.
    pub device: Option<usize>,
    /// Which face goes on the printer's bed.
    pub bed: Face,
    pub solid: Solid,
}

#[derive(Clone, Copy, Debug, Serialize, PartialEq, Eq, Tsify)]
#[serde(rename_all = "camelCase")]
pub enum Face {
    Front,
    Back,
}

#[derive(Clone, Copy, Debug, Serialize, PartialEq, Eq, Tsify)]
#[serde(rename_all = "camelCase")]
pub enum PartKind {
    Faceplate,
    LeftEar,
    RightEar,
    Cage,
}

#[derive(Clone, Debug, Serialize, PartialEq, Tsify)]
pub struct Warning {
    pub row: usize,
    pub device: Option<usize>,
    pub issue: Issue,
}

/// Things the user should know about the design, with the numbers to explain them.
#[derive(Clone, Debug, Serialize, PartialEq, Tsify)]
#[serde(tag = "kind", rename_all = "camelCase", rename_all_fields = "camelCase")]
pub enum Issue {
    MissingModel {
        id: String,
    },
    /// The device with its cage is taller than its row, so it's left out.
    TooTall {
        height: f64,
        row: f64,
    },
    /// The device, with the room around its collar, is wider than what's left of its row between the ears and the
    /// rails, or with its own ears, wider than the panel, so it's left out.
    TooWide {
        width: f64,
        room: f64,
    },
    /// The device has no outline to fit, as a model scaled to nothing hasn't, so it's left out.
    NoOutline,
    /// There's no room for the latch, so it's left out and the cage only slides into its collar:
    /// what it needs and what there is across, up and deep. Across, the device's top and underside
    /// must run flat for long enough for two tongues each, which bend without cracking; up, the row must
    /// hold the cage, the pads over its tongues and the collar; deep, the tongues must end in
    /// front of the cage's back.
    Unlatched {
        need: [f64; 3],
        room: [f64; 3],
    },
}

#[derive(Clone, Debug, Serialize, Tsify)]
pub struct RowReport {
    /// By index in the row; none where the device is left out.
    pub devices: Vec<Option<DeviceReport>>,
    /// The width its devices take, room for collars and joints included, and the width it has
    /// between the ears or the rails; with a device with its own ears, its width and the panel's.
    pub width: f64,
    pub room: f64,
    /// What's left of the room, from left to right: before the first device, between each two, and
    /// after the last. Empty for an empty row, and one with a device with its own ears.
    pub gaps: Vec<f64>,
    /// The units it takes.
    pub units: u32,
}

#[derive(Clone, Debug, Serialize, Tsify)]
#[serde(rename_all = "camelCase")]
pub struct DeviceReport {
    /// Source model to rack frame, column-major 4x4.
    pub matrix: [f64; 16],
    /// Where in the rack the device's own frame starts, which its holes are in: the middle of its bottom front edge.
    pub origin: [f64; 3],
    /// Width, height and depth.
    pub size: [f64; 3],
    /// How far the front opening is inset from the device's outline, sideways and vertically.
    pub retention: [f64; 2],
    /// Set back behind a lip because the device is box-shaped at the front.
    pub lip: bool,
}

/// What an end of a row keeps clear of its rail, behind the panel: a bolted-on ear's rib, or a gap.
fn inset(side: Side) -> f64 {
    match side {
        Side::BoltedEar => RIB,
        Side::Ear => RAIL_GAP,
        Side::Joiner | Side::Plain => 0.0,
    }
}

pub fn generate(p: &Project, models: &HashMap<String, Device>) -> Assembly {
    let layout = |rack| layout(p, models, rack);
    if p.rack != RackWidth::Fit {
        return layout(p.rack);
    }
    // Devices with their own ears fit only the racks they're made for: the smallest standard one
    // that holds the rest too.
    if p.rows.iter().any(|r| r.racked) {
        let ten = layout(RackWidth::Inch10);
        let wide = ten.warnings.iter().any(|w| matches!(w.issue, Issue::TooWide { .. }));
        return if wide { layout(RackWidth::Inch19) } else { ten };
    }
    // What each row needs, laid out in a rack too wide to matter, with its ends' room each side; the
    // ears' holes and edges sit as on a 19" rack.
    let inch19 = RackWidth::Inch19.dimensions();
    let opening = |rail_opening: f64| RackWidth::Custom {
        panel_width: rail_opening + inch19.width - inch19.rail_opening,
        hole_spacing: rail_opening + inch19.hole_spacing - inch19.rail_opening,
        rail_opening,
    };
    let roomy = layout(opening(1e5));
    if roomy.rows.iter().all(|r| r.width == 0.0) {
        return layout(RackWidth::Inch10);
    }
    let need = roomy.rows.iter().zip(&p.rows).map(|(r, row)| r.width + inset(row.left) + inset(row.right));
    layout(opening(need.fold(0.0, f64::max) + 0.01))
}

/// The design in a rack this wide, from the models by id.
fn layout(p: &Project, models: &HashMap<String, Device>, rack: RackWidth) -> Assembly {
    let panel = rack.dimensions();
    let (half, hole_x, rails) = (panel.width / 2.0, panel.hole_spacing / 2.0, panel.rail_opening);
    let (mut parts, mut warnings, mut rows) = (Vec::new(), Vec::new(), Vec::new());
    let mut top = 0.0;
    for (r, row) in p.rows.iter().enumerate() {
        let mut warn = |device, issue| warnings.push(Warning { row: r, device, issue });
        let report = |device: &Device, size: DVec3, retention, lip, shift: DVec3| DeviceReport {
            matrix: (DMat4::from_translation(shift) * DMat4::from(device.to_device)).to_cols_array(),
            origin: shift.to_array(),
            size: size.to_array(),
            retention,
            lip,
        };
        // A device with its own ears takes as many whole units as it's tall: centered, on its units'
        // bottom, with its front at the panel's. Nothing is printed for it.
        if row.racked {
            let device = row.devices.first().map(|id| (id, models.get(id)));
            let size = device.and_then(|(_, d)| d).map_or(DVec3::ZERO, |d| d.profile.size);
            let units = if size.y > 0.0 { ((size.y - UNIT_SLACK) / U).ceil().max(1.0) as u32 } else { 1 };
            top -= units as f64 * U;
            let mut devices = vec![None; row.devices.len()];
            match device {
                Some((id, None)) => warn(Some(0), Issue::MissingModel { id: id.clone() }),
                Some(_) if size.x > panel.width => warn(Some(0), Issue::TooWide { width: size.x, room: panel.width }),
                Some((_, Some(d))) => devices[0] = Some(report(d, size, [0.0; 2], false, DVec3::new(0.0, top, 0.0))),
                None => {}
            }
            rows.push(RowReport { devices, width: size.x, room: panel.width, gaps: Vec::new(), units });
            continue;
        }
        let units = row.units.max(1) as f64;
        let (y0, y1) = (top - units * U + PANEL_GAP / 2.0, top - PANEL_GAP / 2.0);
        top -= units * U;

        // Lay the devices out side by side, each with its room either side of its collar.
        // A device we have no model for, nothing to fit, or a cage that doesn't fit the row, is left
        // out.
        let placed: Vec<_> = row
            .devices
            .iter()
            .enumerate()
            .filter_map(|(i, id)| match models.get(id).map(|d| (d, d.fit(p))) {
                None => {
                    warn(Some(i), Issue::MissingModel { id: id.clone() });
                    None
                }
                Some((_, None)) => {
                    warn(Some(i), Issue::NoOutline);
                    None
                }
                Some((d, Some(f))) => {
                    if f.height(p) > y1 - y0 {
                        warn(Some(i), Issue::TooTall { height: f.height(p), row: y1 - y0 });
                        return None;
                    }
                    Some((i, d, f))
                }
            })
            .collect();
        let ear = |side: Side| matches!(side, Side::Ear | Side::BoltedEar);
        let ribbed = |side: Side| matches!(side, Side::BoltedEar | Side::Joiner);
        // Which ends of the k-th of n pieces have a joint: each between split pieces, and the row's
        // ends that bolt on.
        let rib_l = |k: usize| (p.build.split && k > 0) || (k == 0 && ribbed(row.left));
        let rib_r = |k: usize, n: usize| (p.build.split && k + 1 < n) || (k + 1 == n && ribbed(row.right));
        // Where devices may go: up to 2 mm short of the ear's slots on an ear's side, to the panel's
        // edge otherwise, and with everything behind the panel between the rails, a bolted-on ear's
        // rib included.
        let reach = |side: Side| {
            let panel = if ear(side) { hole_x - EAR_HOLE / 2.0 - EAR_SLOT / 2.0 - 2.0 } else { half };
            panel.min(rails / 2.0 - inset(side))
        };
        let (lo, hi) = (-reach(row.left), reach(row.right));
        // Each device's room left and right of its collar. A screw goes in through a piece's left
        // rib, so that side needs room for it and a hex key; the nuts sit in a block on its right,
        // which the collar's own wall is part of.
        let nut = (BLOCK - p.build.wall).max(0.0);
        let rooms = |n: usize| -> Vec<(f64, f64)> {
            (0..n)
                .map(|k| {
                    // Next to a bolted-on left ear the screws come from the ear's side, so only a
                    // nut needs room here.
                    let screwed = rib_l(k) && !(k == 0 && row.left == Side::BoltedEar);
                    // An ear that's part of the piece needs no room at all: the collar may come right
                    // up to the rail.
                    let end = |side: Side, last: bool| if last && side == Side::Ear { 0.0 } else { MARGIN };
                    let left = if screwed {
                        SCREW_ROOM + RIB
                    } else if rib_l(k) {
                        nut
                    } else {
                        end(row.left, k == 0)
                    };
                    (left, if rib_r(k, n) { nut } else { end(row.right, k + 1 == n) })
                })
                .collect()
        };
        let width = |fitted: &[(usize, &Device, Rc<DeviceFit>)]| -> f64 {
            fitted.iter().zip(rooms(fitted.len())).map(|((_, _, f), (l, r))| l + 2.0 * f.half_width(p) + r).sum()
        };
        // Devices go in in order, as long as they fit. A device's width counts the room for its
        // collar's screws and nuts, not just its cage.
        let mut fitted = Vec::new();
        for device in placed {
            let (i, before) = (device.0, width(&fitted));
            fitted.push(device);
            let after = width(&fitted);
            if after > hi - lo {
                fitted.pop();
                warn(Some(i), Issue::TooWide { width: after - before, room: (hi - lo - before).max(0.0) });
            }
        }
        let (n, total) = (fitted.len(), width(&fitted));
        // Spread out, each device after the first starts its share of the spare width early: it
        // widens the room between a joint's rib and the collar, where the screws and the key go.
        // The ends' shares widen the ears, moving their joints in from the rails, or are bare plate.
        let spread = if row.align == Align::Spread { (hi - lo - total).max(0.0) / (n + 1) as f64 } else { 0.0 };
        let span = total + spread * n.saturating_sub(1) as f64;
        let start = match row.align {
            Align::Left => lo,
            Align::Right => hi - span,
            Align::Center | Align::Spread => (-span / 2.0).clamp(lo, (hi - span).max(lo)),
        };
        let mut edges = vec![start];
        let mut modules: Vec<(usize, Module)> = Vec::new();
        for (k, ((i, _, f), (l, r))) in fitted.iter().zip(rooms(n)).enumerate() {
            let x0 = *edges.last().unwrap() + if k > 0 { spread } else { 0.0 };
            let m = f.module(x0 + l + f.half_width(p), (y0, y1), p);
            if let Some(issue) = m.issue.clone() {
                warn(Some(*i), issue);
            }
            modules.push((*i, m));
            edges.push(x0 + l + 2.0 * f.half_width(p) + r);
        }

        let (first, last) = (edges[0], *edges.last().unwrap());
        let gaps = if n == 0 { Vec::new() } else { [vec![first - lo], vec![spread; n - 1], vec![hi - last]].concat() };

        // Faceplate pieces: (kind, device, x0, x1, rib left, rib right). An ear that's part of the
        // row takes the end piece to the panel's edge; one that's bolted on is a piece of its own.
        let mut pieces = Vec::new();
        if modules.is_empty() {
            pieces.push((PartKind::Faceplate, None, -half, half, false, false));
        } else {
            let to_edge = |side: Side, x: f64, edge: f64| if side == Side::Ear { edge } else { x };
            if p.build.split {
                for (k, ((i, _), e)) in modules.iter().zip(edges.windows(2)).enumerate() {
                    let x0 = if k == 0 { to_edge(row.left, e[0], -half) } else { e[0] };
                    let x1 = if k + 1 == n { to_edge(row.right, e[1], half) } else { e[1] };
                    pieces.push((PartKind::Faceplate, Some(*i), x0, x1, rib_l(k), rib_r(k, n)));
                }
            } else {
                let (x0, x1) = (to_edge(row.left, first, -half), to_edge(row.right, last, half));
                pieces.push((PartKind::Faceplate, None, x0, x1, rib_l(0), rib_r(n - 1, n)));
            }
            if row.left == Side::BoltedEar {
                pieces.push((PartKind::LeftEar, None, -half, first, false, true));
            }
            if row.right == Side::BoltedEar {
                pieces.push((PartKind::RightEar, None, last, half, true, false));
            }
        }

        let mut devices = vec![None; row.devices.len()];
        for ((i, device, f), (_, m)) in fitted.iter().zip(&modules) {
            devices[*i] = Some(report(device, f.size, f.retention, f.recess > 0.0, m.shift));
        }
        rows.push(RowReport { devices, width: total, room: hi - lo, gaps, units: units as u32 });

        // Each device's cable holes, where it sits: their centers, across, and outlines. A full turn ends where it
        // starts: once will do.
        let cables: Vec<(f64, f64, poly::Contour)> = modules
            .iter()
            .flat_map(|(i, m)| {
                p.holes.iter().filter(|h| h.model == row.devices[*i] && h.diameter > 0.0).map(|h| (m.shift, h))
            })
            .map(|(at, h)| {
                let (x, y, r) = (at.x + h.x, at.y + h.y, h.diameter / 2.0);
                (x - r, x + r, poly::arc([x, y], r, 0.0, 360.0)[1..].to_vec())
            })
            .collect();
        let t = p.build.plate;
        for (kind, device, x0, x1, rib_l, rib_r) in pieces {
            // Square where pieces meet, so the seam is a clean line; rounded at the row's ends.
            let corner = |rib: bool| if rib { 0.0 } else { CORNER };
            let plate = poly::sided_rect(x0, y0, x1, y1, corner(rib_l), corner(rib_r));
            let mut add = vec![Solid::prism_z(vec![plate], -t, 0.0).labeled("panel")];
            let mut cut = Vec::new();
            let mine: Vec<&Module> = modules
                .iter()
                .filter(|(i, m)| device.is_none_or(|d| d == *i) && m.shift.x > x0 && m.shift.x < x1)
                .map(|(_, m)| m)
                .collect();
            // Nuts sit in device pieces: on the right, and on the left next to a bolted-on ear, whose screws come from
            // the ear's side. There the block runs on into the collar next to it, up to its inside.
            let device_piece = kind == PartKind::Faceplate;
            let nut_l = device_piece && row.left == Side::BoltedEar && x0 == first;
            let blocks = [
                mine.first().filter(|_| nut_l).map(|m| (m.inside[0], &m.outside)),
                mine.last().filter(|_| device_piece).map(|m| (m.inside[1], &m.outside)),
            ];
            for ((on, edge, right), block) in [(rib_l, x0, false), (rib_r, x1, true)].into_iter().zip(blocks) {
                if on {
                    let (a, c) = joint(p, (y0, y1), edge, right, block);
                    add.extend(a);
                    cut.extend(c);
                }
            }
            for (x, side) in [(-hole_x, row.left), (hole_x, row.right)] {
                if ear(side) && x > x0 && x < x1 {
                    let unit_bottom = y0 - PANEL_GAP / 2.0;
                    let ys = (0..row.units.max(1)).flat_map(|u| EAR_HOLES.map(|h| unit_bottom + u as f64 * U + h));
                    let slots = ys.map(|y| poly::stadium(x - EAR_SLOT / 2.0, x + EAR_SLOT / 2.0, y, EAR_HOLE / 2.0));
                    cut.push(Solid::prism_z(slots.collect(), -t - 1.0, 1.0).labeled("ear slots"));
                }
            }
            for m in mine {
                add.push(m.collar.clone());
                cut.push(m.nest.clone());
            }
            // Holes go straight through all of the piece, collar, ribs and a tongue reaching past its edge too, cut
            // last: moving one leaves the rest of the piece as it was, which the mesh kernel then reuses.
            let mut solid = Solid::difference(Solid::union(add), cut);
            let (lo, hi) = (x0 - TONGUE, x1 + TONGUE);
            let holes: Vec<_> = cables.iter().filter(|(a, b, _)| *b > lo && *a < hi).map(|(.., c)| c.clone()).collect();
            if !holes.is_empty() {
                let deep = -t - p.build.collar_depth.max(RIB_DEPTH) - 1.0;
                solid = Solid::difference(solid, vec![Solid::prism_z(holes, deep, 1.0).labeled("cable holes")]);
            }
            parts.push(Part { row: r, kind, device, bed: Face::Front, solid });
        }
        for (i, m) in modules {
            parts.push(Part { row: r, kind: PartKind::Cage, device: Some(i), bed: Face::Back, solid: m.cage });
        }
    }
    Assembly { parts, warnings, rows, panel }
}

/// What a joint at a piece's left or right edge adds and cuts, in a row from y0 to y1: with a `block`, the inside and
/// outside of the collar it runs on into, the block the nuts sit in, otherwise the rib and gusset the screws go in
/// through; and on the right the tongue, on the left the groove.
fn joint(
    p: &Project,
    (y0, y1): (f64, f64),
    edge: f64,
    right: bool,
    block: Option<(f64, &poly::Shapes)>,
) -> (Vec<Solid>, Vec<Solid>) {
    let t = p.build.plate;
    let (mut add, mut cut) = (Vec::new(), Vec::new());
    let back = -t - RIB_DEPTH + SCREW_FROM_BACK;
    let middle = ((y0 + y1) / 2.0, -t - SCREW_FROM_PLATE);
    let screws = [middle, (y0 + SCREW_FROM_EDGE, back), (y1 - SCREW_FROM_EDGE, back)];
    let holes = screws.map(|(y, z)| poly::teardrop(y, z, BOLT / 2.0)).to_vec();
    // Clear of the plate's rounded corners, so the rib's edges don't meet theirs.
    let (ry0, ry1) = (y0 + CORNER + 1.0, y1 - CORNER - 1.0);
    // From the joint face into the piece.
    let into = if right { -1.0 } else { 1.0 };
    let at = |z: f64, c: poly::Contour| -> Vec<[f64; 3]> { c.into_iter().map(|[x, y]| [x, y, z]).collect() };
    if let Some((inside, collar)) = block {
        let outline = vec![poly::rounded_rect(edge.min(inside), ry0, edge.max(inside), ry1, ROUND)];
        // It meets the collar in fillets, less the specks the closing leaves where its offsets land a hair off what
        // they closed, reaching into both so they don't just touch.
        let both = poly::union(&vec![outline.clone()], collar);
        let fillets = poly::fill_holes(&poly::difference(&poly::closed(&both, ROUND), &both), 0.01);
        let fillets = poly::union(&fillets, &poly::intersection(&poly::offset(&fillets, EPS / 2.0), &both)).concat();
        add.push(Solid::prism_z(fillets, -t - p.build.collar_depth, -t + EPS).labeled("block fillets"));
        add.push(Solid::prism_z(outline, -t - RIB_DEPTH, -t + EPS).labeled("joint block"));
        let end = edge + into * (SCREW - RIB + HOLE_PAST);
        cut.push(Solid::prism_x(holes.clone(), edge - into, end).labeled("screw holes"));
        // A trap's floor is a shallow V: a hex nut lands on a corner, a square one on two, and both end up centered on
        // the screw's axis. Its corners stay square, to hold the nut.
        let (w, deep, apex) = (NUT / 2.0 + NUT_GAP, -t - RIB_DEPTH - 1.0, NUT_CORNERS / 2.0);
        let side = apex - w * (NUT_CORNERS - NUT) / NUT;
        let traps = screws
            .map(|(y, z)| vec![[y - w, deep], [y + w, deep], [y + w, z + side], [y, z + apex], [y - w, z + side]])
            .to_vec();
        let (near, far) = (edge + into * NUT_WALL, edge + into * (NUT_WALL + NUT_T + 2.0 * NUT_GAP));
        cut.push(Solid::prism_x(traps, near, far).labeled("nut traps"));
    } else {
        // The rib and its gusset in one, from inside the plate, the slope's foot included.
        let face = edge + into * RIB;
        let slope = |z: f64| face + into * GUSSET * (1.0 + (z + t) / RISE);
        let ring = |z: f64, x: f64| at(z, poly::rounded_rect(edge.min(x), ry0, edge.max(x), ry1, ROUND));
        let rib = vec![ring(-t + EPS, slope(-t + EPS)), ring(-t - RISE, face), ring(-t - RIB_DEPTH, face)];
        add.push(Solid::loft(rib).labeled("joint rib"));
        cut.push(Solid::prism_x(holes.clone(), edge - into, face + into).labeled("screw holes"));
        // Through the gusset, room for the middle screw's head and the tool, its mouth chamfered CHAMFER wide where it
        // meets the slope.
        let clear = vec![poly::teardrop(middle.0, middle.1, CLEAR / 2.0)];
        // The hole's outline scaled, so the rings pair up point for point.
        let mouth = |k: f64, off: f64| -> Vec<[f64; 3]> {
            let at = |[y, z]: poly::P2| [middle.0 + k * (y - middle.0), middle.1 + k * (z - middle.1)];
            clear[0].iter().map(|&q| at(q)).map(|[y, z]| [slope(z) + into * off, y, z]).collect()
        };
        let rings = vec![mouth(1.0, -CHAMFER), mouth(1.0 + 3.0 * CHAMFER / CLEAR, CHAMFER / 2.0)];
        cut.push(Solid::prism_x(clear, face, face + into * (GUSSET + 0.5)).labeled("middle screw clearance"));
        cut.push(Solid::loft(rings).labeled("middle screw chamfer"));
    }
    // The tongue, in the middle of the plate's thickness, with its front chamfered at 45 degrees, on into the plate:
    // the plate prints front down, and so the tongue needs no support.
    let (zb, zf) = (-0.75 * t, -0.25 * t);
    let reach = TONGUE.min(zf - zb);
    // How far past the joint face, at a height: the groove is the tongue grown by the gap.
    let (g, slope) = if right { (0.0, 0.0) } else { (TONGUE_GAP, TONGUE_GAP * std::f64::consts::SQRT_2) };
    let mut steps =
        vec![(zb - g, reach + g), (zf + slope - reach - g, reach + g), (zf / 2.0 + slope + g, zf / 2.0 - g)];
    steps.dedup_by(|a, b| (a.0 - b.0).abs() < 1e-9);
    let rings = steps
        .into_iter()
        .map(|(z, out)| at(z, poly::sided_rect(edge - 4.0, ry0 - g, edge + out, ry1 + g, 0.0, ROUND + g)));
    if right {
        add.push(Solid::loft(rings.collect()).labeled("tongue"));
    } else {
        cut.push(Solid::loft(rings.collect()).labeled("groove"));
    }
    (add, cut)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::mesh;
    use crate::project::{Hole, Row};

    /// A 150 x 30 x 100 box, one 50 tall, and one 100 x 20 x 60 with its upright edges chamfered
    /// 5 mm at 45 degrees, front and back.
    fn models() -> HashMap<String, Device> {
        let ring = |z: f64, w: f64| (z, poly::rounded_rect(-w, 0.0, w, 20.0, 0.0));
        let chamfered = mesh::loft(&[ring(0.0, 45.0), ring(-5.0, 50.0), ring(-55.0, 50.0), ring(-60.0, 45.0)]);
        HashMap::from([
            ("box".to_string(), Device::boxed([150.0, 30.0, 100.0])),
            ("tall".to_string(), Device::boxed([150.0, 50.0, 100.0])),
            ("chamfered".to_string(), Device::from(chamfered)),
        ])
    }

    fn rack(rows: &[&[&str]], split: bool) -> Assembly {
        let models = models();
        let mut p = Project::default();
        p.build.split = split;
        p.rows = rows
            .iter()
            .map(|ids| Row { devices: ids.iter().map(|s| s.to_string()).collect(), ..Row::default() })
            .collect();
        generate(&p, &models)
    }

    #[test]
    fn a_proud_led_does_not_close_the_opening() {
        // A box with a tiny LED standing 0.2 mm proud of its face: the face, not the LED, is the front.
        // In the device frame the frontmost point is at z = 0, so that's the LED; the face is behind it.
        let shift = |by: glam::DVec3| move |t: mesh::Tri| t.map(|p| p + by);
        let mut tris: Vec<mesh::Tri> =
            Device::boxed([150.0, 30.0, 100.0]).tris.into_iter().map(shift(glam::DVec3::new(0.0, 0.0, -0.2))).collect();
        tris.extend(Device::boxed([2.0, 2.0, 0.2]).tris.into_iter().map(shift(glam::DVec3::new(0.0, 14.0, 0.0))));
        let models = HashMap::from([("led".to_string(), Device::from(tris))]);
        let p = Project { rows: vec![Row { devices: vec!["led".into()], ..Row::default() }], ..Project::default() };
        let device = generate(&p, &models).rows[0].devices[0].clone().unwrap();
        // Boxed at the front, so behind a lip that overlaps it by the lip less the clearance.
        assert!(device.lip && (device.retention[0] - (1.5 - 0.25)).abs() < 0.05, "{device:?}");
    }

    fn kinds(a: &Assembly) -> Vec<(PartKind, Option<usize>)> {
        a.parts.iter().map(|p| (p.kind, p.device)).collect()
    }

    #[test]
    fn rounded_front_sits_flush_and_box_gets_a_lip() {
        let a = rack(&[&["chamfered", "box"]], true);
        assert_eq!(a.warnings, []);
        let [round, square] = [0, 1].map(|i| a.rows[0].devices[i].clone().unwrap());
        // The opening at the plate's front is the face, 90 mm, plus 0.25 mm of clearance per side.
        assert!(!round.lip && (round.retention[0] - (100.0 - 90.5) / 2.0).abs() < 0.05, "{round:?}");
        assert!(square.lip && (square.retention[0] - (1.5 - 0.25)).abs() < 0.05, "{square:?}");
    }

    #[test]
    fn split_rows_give_a_piece_per_device_and_ear() {
        use PartKind::*;
        let a = rack(&[&["box", "box"]], true);
        let expect = [
            (Faceplate, Some(0)),
            (Faceplate, Some(1)),
            (LeftEar, None),
            (RightEar, None),
            (Cage, Some(0)),
            (Cage, Some(1)),
        ];
        assert_eq!(kinds(&a), expect);
        let whole = [(Faceplate, None), (LeftEar, None), (RightEar, None), (Cage, Some(0)), (Cage, Some(1))];
        assert_eq!(kinds(&rack(&[&["box", "box"]], false)), whole);
    }

    /// The prisms labeled `name`: from, to, their outlines and the outlines' bounds.
    fn prisms(s: &Solid, name: &str) -> Vec<(f64, f64, poly::Contour, (poly::P2, poly::P2))> {
        let each = s.prisms(name).into_iter().flat_map(|(lo, hi, cs)| cs.iter().map(move |c| (lo, hi, c.clone())));
        each.map(|(lo, hi, c)| (lo, hi, c.clone(), poly::bounds(&vec![vec![c]]).unwrap())).collect()
    }

    /// The highest point of an outline at `u`, if it reaches there.
    fn top(c: &[poly::P2], u: f64) -> Option<f64> {
        let edges = c.iter().zip(c.iter().cycle().skip(1));
        edges
            .filter_map(|(a, b)| {
                let s = (u - a[0]) / (b[0] - a[0]);
                (0.0..=1.0).contains(&s).then(|| a[1] + s * (b[1] - a[1]))
            })
            .reduce(f64::max)
    }

    /// Nuts sit in device pieces: on their right, and on their left next to a bolted-on ear. Each
    /// side is one block from the joint face on into the collar, with a trap per nut that opens at
    /// the block's back end. Square nuts (DIN 562) and hex nuts (DIN 934) alike land on a screw's
    /// axis there, and M3x8, M3x10 and M3x12 screws all reach through the nut and stop short of
    /// where the cage slides.
    #[test]
    fn nuts_sit_in_traps_in_blocks_that_run_into_the_collar() {
        // Thickness and outline across the trap, flats against its walls: a hex nut stands on a corner.
        let (f, c) = (5.5 / 2.0, 6.35 / 2.0);
        let nuts = [
            (1.8, vec![[-f, -f], [f, -f], [f, f], [-f, f]]),
            (2.4, vec![[0.0, -c], [f, -c / 2.0], [f, c / 2.0], [0.0, c], [-f, c / 2.0], [-f, -c / 2.0]]),
        ];
        let a = rack(&[&["box", "box"]], true);
        let plates: Vec<&Part> = a.parts.iter().filter(|p| p.kind != PartKind::Cage).collect();
        let found: Vec<(PartKind, usize)> =
            plates.iter().map(|p| (p.kind, p.solid.find("joint block").len())).collect();
        use PartKind::*;
        assert_eq!(found, [(Faceplate, 2), (Faceplate, 1), (LeftEar, 0), (RightEar, 0)]);
        let t = Project::default().build.plate;
        for part in plates {
            let insides: Vec<f64> =
                prisms(&part.solid, "collar").iter().flat_map(|(.., (lo, hi))| [lo[0], hi[0]]).collect();
            let (traps, holes) = (prisms(&part.solid, "nut traps"), prisms(&part.solid, "screw holes"));
            for (.., (lo, hi)) in prisms(&part.solid, "joint block") {
                // The block runs from the joint face to the collar's inside, with nothing between.
                let (inside, face) = if insides.contains(&lo[0]) { (lo[0], hi[0]) } else { (hi[0], lo[0]) };
                assert!(insides.contains(&inside), "the block stops short of the collar");
                assert!(((face - inside).abs() - BLOCK).abs() < 1e-4, "{} mm thick", (face - inside).abs());
                let within = |(x0, x1, ..): &&(f64, f64, _, _)| x0.max(lo[0]) < x1.min(hi[0]);
                let (traps, holes): (Vec<_>, Vec<_>) =
                    (traps.iter().filter(within).collect(), holes.iter().filter(within).collect());
                assert_eq!((traps.len(), holes.len()), (3, 3));
                for &&(x0, x1, ref trap, (tlo, thi)) in &traps {
                    assert!(tlo[1] < -t - RIB_DEPTH, "a trap stops at z = {}, short of the back end", tlo[1]);
                    let y = (tlo[0] + thi[0]) / 2.0;
                    let screw = holes.iter().find(|(.., (hlo, hhi))| ((hlo[0] + hhi[0]) / 2.0 - y).abs() < 1e-9);
                    let &&(h0, h1, _, (_, hhi)) = screw.unwrap_or_else(|| panic!("no screw through the trap at {y}"));
                    let axis = hhi[1] - BOLT / 2.0;
                    // Pushed down to the trap's floor, either nut ends up on the screw's axis.
                    for (_, nut) in &nuts {
                        let gaps = (-300..=300).filter_map(|i| {
                            let u = i as f64 / 100.0;
                            Some(top(trap, y + u)? - top(nut, u)?)
                        });
                        let center = gaps.fold(f64::INFINITY, f64::min);
                        assert!((center - axis).abs() < 0.01, "a nut lands {} mm off the axis", center - axis);
                    }
                    // The wall toward the joint face takes the clamp; either nut fits behind it.
                    let (near, far) =
                        ((x0 - face).abs().min((x1 - face).abs()), (x0 - face).abs().max((x1 - face).abs()));
                    assert!(near > 1.2 - 1e-6 && far - near > 2.4, "a trap {near} to {far} mm from the joint face");
                    // Past the head side's rib, each screw's tip reaches through either nut, and its
                    // hole runs on past it, but stops short of where the cage slides.
                    let end = if inside < face { h0 } else { h1 };
                    let (depth, wall) = ((end - face).abs(), (inside - end).abs());
                    assert!(wall > 0.8 - 1e-6, "a hole {wall} mm from the cage");
                    for screw in [8.0, 10.0, 12.0] {
                        let tip = screw - RIB;
                        for (thick, _) in &nuts {
                            assert!(tip >= near + thick, "an M3x{screw} stops short of a nut's far face");
                        }
                        assert!(tip < depth, "an M3x{screw} in a hole {depth} mm deep");
                    }
                }
            }
        }
    }

    /// The middle screw goes in through the gusset: a flanged button head (ISO 7380-2, 6.9 mm
    /// across) passes its hole with half a millimeter to spare all round, and the hole is a tunnel
    /// with its point 2 mm under the gusset's top, so it prints as an arch and leaves the gusset
    /// whole above it, and 1 mm clear of the plate below.
    #[test]
    fn the_middle_screw_gets_through_its_gusset() {
        let a = rack(&[&["box", "box"]], true);
        let part = a.parts.iter().find(|p| !p.solid.find("middle screw clearance").is_empty()).unwrap();
        let [(.., (lo, hi))] = prisms(&part.solid, "middle screw clearance")[..] else { panic!("one hole") };
        let [Solid::Mesh { vertices, .. }] = part.solid.find("joint rib")[..] else { panic!("one rib") };
        let mut levels: Vec<f64> = vertices.iter().map(|v| v[2]).collect();
        levels.sort_by(f64::total_cmp);
        levels.dedup();
        let (plate, top) = (-Project::default().build.plate, levels[1]);
        assert!(hi[0] - lo[0] >= 6.9 + 1.0, "{} mm across", hi[0] - lo[0]);
        assert!(
            lo[1] - top >= 2.0 && plate - hi[1] >= 1.0,
            "a hole from {} to {} mm in a gusset {} mm high",
            plate - hi[1],
            plate - lo[1],
            plate - top
        );
    }

    /// Where a joint's block runs into the collar, they meet in fillets: seen from the front, at
    /// the collar's depth, what they make together has no sharp corners but the nut traps'.
    #[test]
    fn the_joint_blocks_meet_the_collars_in_fillets() {
        let a = rack(&[&["box", "box"]], true);
        for part in a.parts.iter().filter(|p| p.kind == PartKind::Faceplate) {
            let shapes: Vec<poly::Shapes> = ["collar", "joint block", "block fillets"]
                .map(|n| {
                    part.solid.prisms(n).into_iter().filter(|(_, hi, _)| *hi > -Project::default().build.plate - 1.0)
                })
                .into_iter()
                .flatten()
                .map(|(.., c)| poly::union_polygons(c.to_vec()))
                .collect();
            let together = shapes.iter().fold(poly::Shapes::new(), |all, s| poly::union(&all, s));
            // Faired by the resolution, of the hairlines where the pieces' edges land a hair apart.
            let sharp = poly::sharp_corners(&poly::fill_holes(&poly::fair(&together, poly::RESOLUTION), 0.0).concat());
            assert!(sharp.is_empty() && !part.solid.find("block fillets").is_empty(), "{sharp:?}");
        }
    }

    /// Where the nuts sit, a joint takes only its block, which the collar's wall is part of: a
    /// split row of two with bolted-on ears is its collars, three blocks less their collars' walls,
    /// and a rib with 20 mm of room for an M3x12 with its head, or a 2.5 mm L-key's short leg.
    #[test]
    fn a_row_is_its_collars_and_joints() {
        let a = rack(&[&["box", "box"]], true);
        let collars: f64 = a
            .parts
            .iter()
            .filter(|p| p.kind == PartKind::Faceplate)
            .map(|p| prisms(&p.solid, "collar").iter().map(|(.., (lo, hi))| hi[0] - lo[0]).fold(0.0, f64::max))
            .sum();
        let joints = 3.0 * (BLOCK - Project::default().build.wall) + 20.0 + RIB;
        assert!((a.rows[0].width - collars - joints).abs() < 1e-6, "{} mm", a.rows[0].width - collars);
    }

    #[test]
    fn rows_stack_downwards_and_problems_are_reported_where_they_are() {
        let a = rack(&[&["box"], &["box", "tall", "gone"]], true);
        let y = |r: usize| a.rows[r].devices[0].as_ref().unwrap().matrix[13];
        assert!(y(1) < y(0) - 30.0);
        assert_eq!(a.warnings.len(), 2, "{:?}", a.warnings);
        assert!(matches!(a.warnings[0], Warning { row: 1, device: Some(1), issue: Issue::TooTall { .. } }));
        assert_eq!(
            a.warnings[1],
            Warning { row: 1, device: Some(2), issue: Issue::MissingModel { id: "gone".into() } }
        );
        // What doesn't fit is left out: no report, no parts.
        assert!(a.rows[1].devices[1].is_none());
        assert!(!a.parts.iter().any(|p| p.row == 1 && p.device == Some(1)));
    }

    /// Three 150 mm devices don't fit a 19" row: the third is left out, and says so.
    #[test]
    fn a_device_that_does_not_fit_is_left_out() {
        let a = rack(&[&["box", "box", "box"]], true);
        assert!(
            matches!(a.warnings[..], [Warning { device: Some(2), issue: Issue::TooWide { .. }, .. }]),
            "{:?}",
            a.warnings
        );
        assert!(a.rows[0].devices[2].is_none() && a.parts.iter().all(|p| p.device != Some(2)));
    }

    /// A device with no room for the latch, across, up or deep: it stays, without a latch, and says
    /// where it falls short. Too narrow for the tongues, too tall for them in a 1U row, too shallow.
    /// A narrow one, whose ledges leave too little air between them for the bottom tongues, still
    /// has room under the ledges.
    #[test]
    fn a_device_without_room_for_the_latch_is_left_unlatched() {
        let parts = |a: &Assembly, name: &str| a.parts.iter().map(|p| p.solid.find(name).len()).sum::<usize>();
        let design = |size: [f64; 3]| {
            let models = HashMap::from([("it".to_string(), Device::boxed(size))]);
            let row = Row { devices: vec!["it".into()], ..Row::default() };
            generate(&Project { rows: vec![row], ..Project::default() }, &models)
        };
        for (short, size) in [(0, [30.0, 30.0, 100.0]), (1, [150.0, 36.0, 100.0]), (2, [150.0, 30.0, 30.0])] {
            let a = design(size);
            let [Warning { device: Some(0), issue: Issue::Unlatched { need, room }, .. }] = a.warnings[..] else {
                panic!("{size:?}: {:?}", a.warnings);
            };
            assert!((0..3).all(|i| (need[i] > room[i]) == (i == short)), "{size:?}: {need:?} {room:?}");
            assert!(a.rows[0].devices[0].is_some());
            assert_eq!(["latch nub", "latch pad", "tongue slits"].map(|n| parts(&a, n)), [0; 3], "{size:?}");
        }
        for a in [rack(&[&["box"]], true), design([45.0, 30.0, 100.0])] {
            assert!(a.warnings.is_empty() && parts(&a, "latch nub") == 4, "{:?}", a.warnings);
        }
    }

    /// A rack that fits its devices: narrower than 19" for one, wider for four, nothing left out.
    /// Its rails are as far apart as its widest row needs, that row's ends included, whatever the
    /// other rows' ends need.
    #[test]
    fn a_rack_that_fits_its_devices() {
        let (models, box_row) = (models(), |n: usize| Row { devices: vec!["box".into(); n], ..Row::default() });
        let fit = |rows: Vec<Row>| generate(&Project { rack: RackWidth::Fit, rows, ..Project::default() }, &models);
        let [one, four] = [fit(vec![box_row(1)]), fit(vec![box_row(4)])];
        assert!(one.warnings.is_empty() && four.warnings.is_empty(), "{:?} {:?}", one.warnings, four.warnings);
        assert!(one.panel.width < 254.0 && four.panel.width > 482.6, "{} {}", one.panel.width, four.panel.width);
        assert!(four.rows[0].devices.iter().all(Option::is_some));
        let rows = vec![Row { left: Side::Plain, right: Side::Plain, ..box_row(2) }, box_row(1)];
        let a = fit(rows.clone());
        let need = a.rows.iter().zip(&rows).map(|(r, row)| r.width + inset(row.left) + inset(row.right));
        let need = need.fold(0.0, f64::max) + 0.01;
        assert!((a.panel.rail_opening - need).abs() < 1e-6, "{} for {need}", a.panel.rail_opening);
    }

    /// A device with its own ears takes as many whole units as it's tall, give or take UNIT_SLACK, so a switch on
    /// its feet is 1U still: centered, on the bottom of its units, its front at the panel's, and nothing is printed
    /// for it. It needs the panel's width. Fitted, it gets the standard rack it's made for.
    #[test]
    fn a_device_with_its_own_ears_takes_its_units() {
        let mut models = models();
        models.insert("switch".into(), Device::boxed([442.4, 44.9, 285.6]));
        models.insert("2u".into(), Device::boxed([442.4, 88.0, 285.6]));
        let row = |id: &str, racked| Row { devices: vec![id.into()], racked, ..Row::default() };
        let mut p = Project {
            rows: vec![row("box", false), row("2u", true), row("switch", true), row("box", false)],
            ..Project::default()
        };
        let a = generate(&p, &models);
        assert_eq!(a.warnings, []);
        // Each device sits on its row's bottom, or a little above it in a cage.
        let device = |r: &RowReport| r.devices[0].clone().unwrap();
        let stack: Vec<_> = a.rows.iter().map(|r| (r.units, (device(r).origin[1] / U).round())).collect();
        assert_eq!(stack, [(1, -1.0), (2, -3.0), (1, -4.0), (1, -5.0)]);
        let switch = device(&a.rows[2]);
        assert_eq!(switch.matrix[12..15], [0.0, switch.origin[1], 0.0]);
        assert!((switch.origin[1] / U + 4.0).abs() < 1e-9, "{:?}", switch.origin);
        assert!(a.parts.iter().all(|p| p.row != 1 && p.row != 2), "{:?}", kinds(&a));
        p.rows = vec![row("switch", true)];
        p.rack = RackWidth::Inch10;
        let wide = Issue::TooWide { width: 442.4, room: RackWidth::Inch10.dimensions().width };
        assert_eq!(generate(&p, &models).warnings, [Warning { row: 0, device: Some(0), issue: wide }]);
        p.rack = RackWidth::Fit;
        assert_eq!(generate(&p, &models).panel.width, RackWidth::Inch19.dimensions().width);
        // With its model gone, it takes a unit.
        p.rows = vec![Row { units: 3, ..row("gone", true) }];
        assert_eq!(generate(&p, &models).rows[0].units, 1);
    }

    /// A hole goes where it's put on its device, through every piece of the row it reaches and all of each, collar and
    /// ribs included: here in the middle of a split row's device, across the seam between its piece and the left ear,
    /// and just past it, where the ear's tongue reaches. It moves with its device, and moving it leaves the rest of
    /// the piece as it was, for the mesh kernel to reuse.
    #[test]
    fn holes_cut_straight_through() {
        let mut p = Project { rows: vec![Row { devices: vec!["box".into()], ..Row::default() }], ..Project::default() };
        let a = generate(&p, &models());
        let ear = a.parts.iter().find(|p| p.kind == PartKind::LeftEar).unwrap();
        let seam = prisms(&ear.solid, "panel")[0].3.1[0];
        let o = a.rows[0].devices[0].as_ref().unwrap().origin;
        let hole = |x: f64, y: f64, diameter| Hole { model: "box".into(), x: x - o[0], y: y - U - o[1], diameter };
        p.holes = vec![hole(0.0, 20.0, 10.0), hole(seam, 10.0, 6.0), hole(seam + 5.5, 25.0, 10.0)];
        let a = generate(&p, &models());
        let cut: Vec<_> = a.parts.iter().map(|part| (part.kind, prisms(&part.solid, "cable holes"))).collect();
        let [(PartKind::Faceplate, ref mine), (PartKind::LeftEar, ref ear), (PartKind::RightEar, ref none), _] =
            cut[..]
        else {
            panic!("{:?}", kinds(&a))
        };
        assert_eq!((mine.len(), ear.len(), none.len()), (3, 2, 0));
        let near = |a: f64, b: f64| (a - b).abs() < 0.01;
        let (z0, z1, _, (lo, hi)) = &mine[0];
        assert!(
            near(lo[0], -5.0) && near(hi[0], 5.0) && near(lo[1], 15.0 - U) && near(hi[1], 25.0 - U),
            "{lo:?} {hi:?}"
        );
        assert!(*z0 < -Project::default().build.plate - RIB_DEPTH && *z1 > 0.0, "{z0} to {z1}");
        for part in &a.parts {
            well_formed(&part.solid).and(traced(&part.solid, false)).unwrap();
        }
        // With the device on the left, its holes go along.
        p.rows[0].align = Align::Left;
        let left = generate(&p, &models());
        let shift = left.rows[0].devices[0].as_ref().unwrap().origin[0] - o[0];
        let (.., (moved, _)) = prisms(&left.parts[0].solid, "cable holes")[0];
        assert!(shift < -10.0 && near(moved[0], lo[0] + shift), "{} for {shift}", moved[0]);
        // Under the cut on top, the piece is as it is without holes.
        let Solid::Difference { base, .. } = &left.parts[0].solid else { panic!("no cut on top") };
        p.holes.clear();
        assert_eq!(format!("{base:?}"), format!("{:?}", generate(&p, &models()).parts[0].solid));
    }

    /// A model with nothing to fit, scaled to nothing or a box with no width, is left out, and says
    /// so, rather than bringing the design down.
    #[test]
    fn a_device_with_no_outline_is_left_out() {
        let nothing = mesh::Orientation { front: mesh::Direction::PosZ, up: mesh::Direction::PosY, scale: 0.0 };
        let flat = Device::new(&[0.0, 0.0, 0.0, 0.1, 0.0, 0.0, 0.0, 0.1, 0.0], &nothing);
        let thin = Device::boxed([0.0, 30.0, 100.0]);
        let models = HashMap::from([("flat".to_string(), flat), ("thin".to_string(), thin)]);
        let row = Row { devices: vec!["flat".into(), "thin".into()], ..Row::default() };
        let a = generate(&Project { rows: vec![row], ..Project::default() }, &models);
        let left_out = |i| Warning { row: 0, device: Some(i), issue: Issue::NoOutline };
        assert_eq!(a.warnings, [left_out(0), left_out(1)]);
        assert!(a.rows[0].devices.iter().all(Option::is_none) && a.parts.iter().all(|p| p.device.is_none()));
    }

    /// Next to an ear that's part of the piece, the collar comes right up to the rail, short of it
    /// by RAIL_GAP.
    #[test]
    fn a_collar_sits_right_by_the_rail() {
        let device = Device::boxed([150.0, 30.0, 100.0]);
        let p = Project {
            rows: vec![Row { devices: vec!["box".into()], left: Side::Ear, align: Align::Left, ..Row::default() }],
            ..Project::default()
        };
        let edge = device.fit(&p).unwrap().half_width(&p);
        let models = HashMap::from([("box".to_string(), device)]);
        let center = generate(&p, &models).rows[0].devices[0].as_ref().unwrap().matrix[12];
        let rails = RackWidth::Inch19.dimensions().rail_opening;
        assert!((center - edge - (-rails / 2.0 + RAIL_GAP)).abs() < 1e-6, "{}", center - edge);
    }

    /// An ear that's part of the row makes the end pieces reach the panel's edge, with no joint.
    #[test]
    fn ears_join_the_row_or_bolt_on() {
        use PartKind::*;
        let mut p = Project { rows: vec![Row { devices: vec!["box".into()], ..Row::default() }], ..Project::default() };
        p.rows[0].left = Side::Ear;
        assert_eq!(kinds(&generate(&p, &models())), [(Faceplate, Some(0)), (RightEar, None), (Cage, Some(0))]);
    }

    /// Spread out, the spare width goes equally into every gap, the ends included; one device sits
    /// as if centered on the panel. Aligned to a side or centered, the gaps are at the ends. Either
    /// way they add up to what the devices leave of the row, and each device starts where its gap ends.
    #[test]
    fn gaps_share_the_spare_width() {
        let models = HashMap::from([("box".to_string(), Device::boxed([100.0, 30.0, 100.0]))]);
        let row = |n: usize, align: Align| {
            let row = Row { devices: vec!["box".into(); n], align, ..Row::default() };
            generate(&Project { rows: vec![row], ..Project::default() }, &models).rows.remove(0)
        };
        // With an ear that's part of the row on the left, the row has more room on that side, but
        // it's centered on the panel still.
        let lopsided = |align: Align| {
            let row = Row { devices: vec!["box".into()], align, left: Side::Ear, ..Row::default() };
            generate(&Project { rows: vec![row], ..Project::default() }, &models).rows.remove(0)
        };
        let near = |a: f64, b: f64| (a - b).abs() < 1e-6;
        for n in 1..=3 {
            for align in [Align::Spread, Align::Left, Align::Center, Align::Right] {
                let r = row(n, align);
                assert_eq!(r.gaps.len(), n + 1, "{align:?}");
                assert!(near(r.gaps.iter().sum(), r.room - r.width), "{align:?}: {:?}", r.gaps);
                let (ends, between) = ([r.gaps[0], r.gaps[n]], &r.gaps[1..n]);
                match align {
                    Align::Spread => assert!(r.gaps.iter().all(|&g| near(g, r.gaps[0]) && g > 10.0), "{:?}", r.gaps),
                    _ => assert!(between.iter().all(|&g| g == 0.0), "{align:?}: {:?}", r.gaps),
                }
                match align {
                    Align::Left => assert!(near(ends[0], 0.0), "{ends:?}"),
                    Align::Right => assert!(near(ends[1], 0.0), "{ends:?}"),
                    _ => assert!(near(ends[0], ends[1]), "{align:?}: {ends:?}"),
                }
            }
        }
        let x = |r: &RowReport, i: usize| r.devices[i].as_ref().unwrap().matrix[12];
        assert!(near(x(&row(1, Align::Spread), 0), x(&row(1, Align::Center), 0)));
        assert!(near(x(&lopsided(Align::Spread), 0), x(&lopsided(Align::Center), 0)));
        // The gap between two devices moves the second one over by as much.
        let (spread, packed) = (row(2, Align::Spread), row(2, Align::Center));
        let apart = |r: &RowReport| x(r, 1) - x(r, 0);
        assert!(near(apart(&spread) - apart(&packed), spread.gaps[1]), "{} {}", apart(&spread), apart(&packed));
    }

    /// Random racks, including silly ones: designing must never panic, and every part must have
    /// something in it. Seeded, so a failure always comes back the same.
    #[test]
    fn random_racks_always_design() {
        let mut seed = 0x2545_f491_4f6c_dd1du64;
        let mut rand = move |lo: f64, hi: f64| {
            seed ^= seed << 13;
            seed ^= seed >> 7;
            seed ^= seed << 17;
            lo + (hi - lo) * (seed % 10_000) as f64 / 10_000.0
        };
        for case in 0..60 {
            let models: HashMap<String, Device> = (0..3)
                .map(|i| (format!("m{i}"), Device::boxed([rand(5.0, 400.0), rand(5.0, 120.0), rand(5.0, 300.0)])))
                .collect();
            let side = |x: f64| [Side::Ear, Side::BoltedEar, Side::Joiner, Side::Plain][(x * 4.0) as usize % 4];
            use crate::project::{Build, Fit};
            let p = Project {
                rack: [RackWidth::Inch19, RackWidth::Inch10, RackWidth::Fit][rand(0.0, 3.0) as usize % 3],
                build: Build {
                    split: rand(0.0, 1.0) < 0.5,
                    vents: rand(0.0, 1.0) < 0.7,
                    plate: rand(0.5, 8.0),
                    wall: rand(0.4, 4.0),
                    collar_depth: rand(2.0, 30.0),
                    back: rand(0.0, 10.0),
                    air: rand(0.0, 5.0),
                },
                fit: Fit { nest: rand(0.0, 1.0), cage: rand(0.0, 1.0), hook: rand(0.2, 2.0), ..Fit::default() },
                rows: (0..rand(1.0, 4.0) as usize)
                    .map(|_| Row {
                        units: rand(1.0, 4.0) as u32,
                        left: side(rand(0.0, 1.0)),
                        right: side(rand(0.0, 1.0)),
                        align: [Align::Center, Align::Left, Align::Right, Align::Spread][rand(0.0, 4.0) as usize % 4],
                        devices: (0..rand(0.0, 4.0) as usize)
                            .map(|_| format!("m{}", rand(0.0, 3.0) as usize))
                            .collect(),
                        racked: rand(0.0, 1.0) < 0.2,
                    })
                    .collect(),
                holes: (0..rand(0.0, 6.0) as usize)
                    .map(|_| Hole {
                        model: format!("m{}", rand(0.0, 3.0) as usize),
                        x: rand(-300.0, 300.0),
                        y: rand(-10.0, 100.0),
                        diameter: rand(0.0, 40.0),
                    })
                    .collect(),
            };
            let a = generate(&p, &models);
            for part in &a.parts {
                well_formed(&part.solid)
                    .and(traced(&part.solid, false))
                    .unwrap_or_else(|e| panic!("case {case}, {:?} {:?}: {e}", part.kind, part.device));
            }
        }
    }

    /// Every prism has a positive size and real outlines, so the kernel gets no surprises.
    fn well_formed(s: &Solid) -> Result<(), String> {
        match s {
            Solid::Extrude { lo, hi, .. } if lo >= hi => Err(format!("prism from {lo} to {hi}")),
            // A lone outline running clockwise is a box drawn from right to left: a piece with no room.
            Solid::Extrude { axis: crate::csg::Axis::Z, contours, .. }
                if contours.len() == 1
                    && i_overlay::i_shape::float::area::Area::area(contours[0].as_slice()) <= 0.0 =>
            {
                Err("inside-out outline".into())
            }
            Solid::Extrude { contours, .. }
                if contours.is_empty()
                    || contours.iter().any(|c| c.len() < 3 || c.iter().flatten().any(|v| !v.is_finite())) =>
            {
                Err("broken outline".into())
            }
            Solid::Mesh { vertices, triangles }
                if triangles.iter().flatten().any(|&i| i as usize >= vertices.len()) =>
            {
                Err("mesh refers to a missing vertex".into())
            }
            Solid::Union { children } => children.iter().try_for_each(well_formed),
            Solid::Difference { base, cut } => well_formed(base).and(cut.iter().try_for_each(well_formed)),
            Solid::Label { solid, .. } => well_formed(solid),
            _ => Ok(()),
        }
    }

    /// Every piece of geometry sits under a label, so any face of a part traces back to its code,
    /// and labels don't nest, since an outer one would take the inner one's place.
    fn traced(s: &Solid, labeled: bool) -> Result<(), String> {
        match s {
            Solid::Extrude { .. } | Solid::Mesh { .. } if !labeled => Err("geometry without a label".into()),
            Solid::Label { name, .. } if labeled => Err(format!("label {name} inside another")),
            Solid::Label { solid, .. } => traced(solid, true),
            Solid::Union { children } => children.iter().try_for_each(|c| traced(c, labeled)),
            Solid::Difference { base, cut } => {
                traced(base, labeled).and(cut.iter().try_for_each(|c| traced(c, labeled)))
            }
            _ => Ok(()),
        }
    }
}
