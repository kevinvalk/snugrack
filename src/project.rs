//! What the user designs: a rack of rows, each holding devices side by side, or one device with its own rack
//! ears. Millimeters throughout.

use std::borrow::Cow;

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
    /// Devices behind a blank faceplate, such as power supplies, whose cables come out of their front.
    pub blanks: Vec<Blank>,
    /// Devices' own settings, over the project's.
    pub overrides: Vec<Override>,
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

/// A device behind a blank faceplate, set back past the collar and its band by `room`, where its front cables bend
/// out through the cage's openings, and its spare cable goes. Pillars on the faceplate's back hold it there.
#[derive(Clone, Debug, Serialize, Deserialize, PartialEq, Tsify)]
#[serde(rename_all = "camelCase")]
pub struct Blank {
    /// The device's model, by id.
    pub model: String,
    pub room: f64,
}

/// A device's own settings, for those that suit one device and not the next: each it has is used
/// instead of the project's. Those of the panel, which its devices share, it can't have.
#[derive(Clone, Debug, Default, Serialize, Deserialize, PartialEq, Tsify)]
#[serde(rename_all = "camelCase")]
pub struct Override {
    /// The device's model, by id.
    pub model: String,
    /// As [`Build`]'s, and [`Fit::cage`].
    pub air: Option<f64>,
    pub wall: Option<f64>,
    pub vents: Option<bool>,
    pub cage: Option<f64>,
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
    /// Round the corners the openings, slits and latch holes leave where they cut through a wall; without, they stay
    /// sharp, easier to see what made what.
    pub round: bool,
}

impl Project {
    /// The project as the device of model `id` sees it: its own build settings over the project's.
    pub fn own(&self, id: &str) -> Cow<'_, Project> {
        match self.overrides.iter().find(|o| o.model == id) {
            Some(o) => {
                let b = self.build;
                let build = Build {
                    air: o.air.unwrap_or(b.air),
                    wall: o.wall.unwrap_or(b.wall),
                    vents: o.vents.unwrap_or(b.vents),
                    ..b
                };
                let fit = Fit { cage: o.cage.unwrap_or(self.fit.cage), ..self.fit };
                Cow::Owned(Project { build, fit, ..self.clone() })
            }
            None => Cow::Borrowed(self),
        }
    }
}

impl Default for Project {
    fn default() -> Self {
        Project {
            rack: RackWidth::Inch19,
            rows: vec![Row::default()],
            fit: Fit::default(),
            build: Build::default(),
            holes: Vec::new(),
            blanks: Vec::new(),
            overrides: Vec::new(),
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
            wall: 2.51,
            collar_depth: 16.0,
            back: 4.0,
            vents: true,
            air: 3.0,
            overhang: 55.0,
            split: true,
            round: true,
        }
    }
}
