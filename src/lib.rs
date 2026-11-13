//! SnugRack designs 3D-printable rack mounts from devices' 3D models. The output is a CSG tree
//! per part (see [`csg`]); a mesh kernel turns it into triangles. JavaScript uses it through
//! [`Models`], with TypeScript types tsify writes into wasm-pack's output.

pub mod csg;
pub mod mesh;
pub mod mount;
pub mod poly;
pub mod project;
pub mod rack;
pub mod silhouette;
pub mod terrain;

use std::cell::RefCell;
use std::collections::HashMap;
use std::rc::Rc;

use glam::DAffine3;
use tsify::Ts;
use wasm_bindgen::prelude::*;

use mesh::Orientation;
use mount::{DeviceFit, Profile};
use project::{Build, Fit, Project};
use rack::Assembly;

/// A device model in its own frame (see [`mesh`]), ready to design mounts for.
pub struct Device {
    tris: Vec<mesh::Tri>,
    /// Source model to device frame.
    to_device: DAffine3,
    /// The slow part of a fit, which no setting changes, so we do it once.
    profile: Profile,
    /// The last fit, for the settings it was for: most edits, a hole moved or a row reordered, change none of them.
    fitted: RefCell<Option<Fitted>>,
}

type Fitted = ((Fit, Build), Option<Rc<DeviceFit>>);

impl Device {
    /// `positions`: flat triangle list in meters, glTF's unit.
    pub fn new(positions: &[f32], orientation: &Orientation) -> Self {
        let (tris, to_device) = mesh::normalize(positions, orientation);
        Device { to_device, ..Device::from(tris) }
    }

    /// A plain box, for devices without a 3D model. Its source frame is the device frame.
    pub fn boxed([w, h, d]: [f64; 3]) -> Self {
        let face = poly::rounded_rect(-w / 2.0, 0.0, w / 2.0, h, 0.0);
        Device::from(mesh::loft(&[(0.0, face.clone()), (-d, face)]))
    }

    /// None for a device with nothing to fit, as a model scaled to nothing has.
    fn fit(&self, p: &Project) -> Option<Rc<DeviceFit>> {
        let settings = (p.fit, p.build);
        let mut last = self.fitted.borrow_mut();
        match &*last {
            Some((made, fit)) if *made == settings => fit.clone(),
            _ => last.insert((settings, DeviceFit::new(&self.tris, &self.profile, p).map(Rc::new))).1.clone(),
        }
    }
}

/// A device from its mesh in the device frame.
impl From<Vec<mesh::Tri>> for Device {
    fn from(tris: Vec<mesh::Tri>) -> Self {
        let profile = Profile::new(&tris);
        Device { tris, to_device: DAffine3::IDENTITY, profile, fitted: RefCell::default() }
    }
}

/// The models a project can use, by id.
#[wasm_bindgen]
#[derive(Default)]
pub struct Models(HashMap<String, Device>);

#[wasm_bindgen]
impl Models {
    #[wasm_bindgen(constructor)]
    pub fn new() -> Models {
        Models::default()
    }

    /// `positions`: flat triangle list in meters, glTF's unit.
    pub fn mesh(&mut self, id: String, positions: &[f32], orientation: Ts<Orientation>) -> Result<(), JsError> {
        self.0.insert(id, Device::new(positions, &orientation.to_rust()?));
        Ok(())
    }

    /// A box of `width` x `height` x `depth` mm.
    #[wasm_bindgen(js_name = "box")]
    pub fn boxed(&mut self, id: String, width: f64, height: f64, depth: f64) {
        self.0.insert(id, Device::boxed([width, height, depth]));
    }

    pub fn generate(&self, project: Ts<Project>) -> Result<Ts<Assembly>, JsError> {
        Ok(Ts::from_rust(&rack::generate(&project.to_rust()?, &self.0))?)
    }
}

#[wasm_bindgen(js_name = defaultProject)]
pub fn default_project() -> Result<Ts<Project>, JsError> {
    Ok(Ts::from_rust(&Project::default())?)
}
