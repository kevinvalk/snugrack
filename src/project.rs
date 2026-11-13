//! What the user designs: a rack of rows, each holding devices side by side, or one device with its own rack
//! ears. Millimeters throughout.

use serde::{Deserialize, Serialize};
use tsify::Tsify;

/// One rack unit (EIA-310).
pub const U: f64 = 44.45;

#[derive(Clone, Debug, Serialize, Deserialize, PartialEq, Tsify)]
#[serde(rename_all = "camelCase")]
pub struct Project {
    pub rack: RackWidth,
    /// Top to bottom.
    pub rows: Vec<Row>,
    pub fit: Fit,
    pub build: Build,
    /// Round holes for cables, straight through the faceplate, each with the device it's for.
    pub holes: Vec<Hole>,
}

#[derive(Clone, Copy, Debug, Serialize, Deserialize, PartialEq, Tsify)]
#[serde(tag = "kind", rename_all = "camelCase", rename_all_fields = "camelCase")]
pub enum RackWidth {
    Inch19,
    /// 10" racks vary by vendor; these are the common values.
    Inch10,
    /// As wide as the widest row needs, narrower than 10" or wider than 19", with ears like a 19"
    /// rack's either side.
    Fit,
    Custom {
        /// Overall panel width including the ears.
        panel_width: f64,
        /// Between the left and right mounting hole centers.
        hole_spacing: f64,
        /// Clear opening between the rails; everything behind the panel must fit.
        rail_opening: f64,
    },
}

/// A rack's panel width, hole spacing and rail opening, as for [`RackWidth::Custom`].
#[derive(Clone, Copy, Debug, Serialize, Tsify)]
#[serde(rename_all = "camelCase")]
pub struct Panel {
    pub width: f64,
    pub hole_spacing: f64,
    pub rail_opening: f64,
}

impl RackWidth {
    pub fn dimensions(self) -> Panel {
        let [width, hole_spacing, rail_opening] = match self {
            RackWidth::Inch19 | RackWidth::Fit => [482.6, 465.1, 450.85],
            RackWidth::Inch10 => [254.0, 236.525, 222.25],
            RackWidth::Custom { panel_width, hole_spacing, rail_opening } => [panel_width, hole_spacing, rail_opening],
        };
        Panel { width, hole_spacing, rail_opening }
    }
}

#[derive(Clone, Debug, Serialize, Deserialize, PartialEq, Tsify)]
#[serde(rename_all = "camelCase")]
pub struct Row {
    pub units: u32,
    pub left: Side,
    pub right: Side,
    /// Where the devices sit when they leave room over.
    pub align: Align,
    /// Model ids, left to right.
    pub devices: Vec<String>,
    /// Its one device comes with its own rack ears: it takes as many units as it's tall, and nothing is printed. The
    /// row's other settings mean nothing then, and stay as they were.
    pub racked: bool,
}

#[derive(Clone, Debug, Serialize, Deserialize, PartialEq, Tsify)]
#[serde(rename_all = "camelCase")]
pub struct Hole {
    /// The device's model, by id: the hole goes wherever it does.
    pub model: String,
    /// Its center in the device's frame: across from the device's middle, to the right, and up from its bottom.
    pub x: f64,
    pub y: f64,
    pub diameter: f64,
}

#[derive(Clone, Copy, Debug, Serialize, Deserialize, PartialEq, Eq, Tsify)]
#[serde(rename_all = "camelCase")]
pub enum Align {
    Center,
    Left,
    Right,
    /// The same room in every gap, the ends included: the most room to work in at each joint.
    Spread,
}

#[derive(Clone, Copy, Debug, Serialize, Deserialize, PartialEq, Eq, Tsify)]
#[serde(rename_all = "camelCase")]
pub enum Side {
    /// Extend to the rail with mounting holes, as part of the piece at that end.
    Ear,
    /// The same, as a piece of its own bolted on: smaller pieces print on smaller beds, and a long
    /// ear can make way for a device later.
    BoltedEar,
    /// End at the last device with a joint to bolt a neighboring piece to.
    Joiner,
    /// End at the last device.
    Plain,
}

/// Gaps that decide how parts fit together. Tune these after a test print.
#[derive(Clone, Copy, Debug, Serialize, Deserialize, PartialEq, Tsify)]
#[serde(rename_all = "camelCase")]
pub struct Fit {
    /// Between the device and the faceplate nest, per side.
    pub nest: f64,
    /// Between the device and the cage cavity, sideways and behind.
    pub cage: f64,
    /// Between the cage and the faceplate collar, per side.
    pub collar: f64,
    /// How far the latch's nubs reach into the holes in the cage's tongues.
    pub hook: f64,
    /// Axial play between the nub and its hole once latched.
    pub play: f64,
}

#[derive(Clone, Copy, Debug, Serialize, Deserialize, PartialEq, Tsify)]
#[serde(rename_all = "camelCase")]
pub struct Build {
    pub plate: f64,
    pub wall: f64,
    /// How deep the cage plugs into the faceplate collar.
    pub collar_depth: f64,
    /// How deep the cage's back is: the struts the device's back rests on, and the walls round them.
    pub back: f64,
    pub vents: bool,
    /// Air under the device, for vents in its bottom: it rests on ledges along the cage's sides.
    pub air: f64,
    /// How far from vertical, in degrees, an opening's top may lean where it follows a wall round a
    /// bend, as the cage prints.
    pub overhang: f64,
    /// Print each device's share of a row as its own piece, bolted to its neighbors.
    pub split: bool,
}

impl Default for Project {
    fn default() -> Self {
        Project {
            rack: RackWidth::Inch19,
            rows: vec![Row::default()],
            fit: Fit::default(),
            build: Build::default(),
            holes: Vec::new(),
        }
    }
}

impl Default for Row {
    fn default() -> Self {
        Row {
            units: 1,
            left: Side::BoltedEar,
            right: Side::BoltedEar,
            align: Align::Spread,
            devices: Vec::new(),
            racked: false,
        }
    }
}

impl Default for Fit {
    fn default() -> Self {
        Fit { nest: 0.25, cage: 0.25, collar: 0.2, hook: 1.2, play: 0.1 }
    }
}

impl Default for Build {
    fn default() -> Self {
        Build {
            plate: 4.0,
            wall: 1.6,
            collar_depth: 16.0,
            back: 5.0,
            vents: true,
            air: 2.0,
            overhang: 60.0,
            split: true,
        }
    }
}
