//! Flattening of imported drawings.
//!
//! A drawing read from a DXF or DWG file may carry 3D data: elevations, thickness, entities with
//! z coordinates. AAD is a 2D drafting engine, so [`flatten_drawing`] projects every entity
//! of model space, layouts and block definitions onto the XY plane (the same operation the
//! FLATTEN command applies to a selection) and reports what it could not flatten.
//!
//! It works on the `Drawing` value it is given; the importer hands it a working copy and keeps
//! the original, so nothing about the file on disk changes and the flattening can be undone.

use std::collections::BTreeMap;
use std::sync::Arc;

use aad_doc::{Drawing, EntityKind, EntityStore};
use serde_json::{Value, json};

use crate::cmd::modify2::flatten_kind;

/// Object types that are 3D by nature. They are kept as read (AAD doesn't model them, and
/// writes them back unchanged only where the file format allows) but cannot be flattened.
const SOLID_TYPES: &[&str] = &[
    "3DSOLID",
    "BODY",
    "SURFACE",
    "PLANESURFACE",
    "EXTRUDEDSURFACE",
    "LOFTEDSURFACE",
    "REVOLVEDSURFACE",
    "SWEPTSURFACE",
    "NURBSURFACE",
    "MESH",
    "LIGHT",
    "CAMERA",
    "SECTION",
];

/// What flattening an imported drawing did.
#[derive(Clone, Debug, Default, PartialEq)]
pub struct FlattenReport {
    /// Entities examined.
    pub examined: usize,
    /// Entities whose coordinates, elevation or thickness changed.
    pub flattened: usize,
    /// 3D-only objects left as read, by DXF type.
    pub unsupported_3d: BTreeMap<String, usize>,
    /// Other object types AAD doesn't model, left as read, by DXF type.
    pub unmodelled: BTreeMap<String, usize>,
    /// Entities drawn on a plane that isn't parallel to the XY plane (their extrusion direction
    /// is not ±Z). Their coordinates are still zeroed; the plane itself is not projected.
    pub tilted_planes: usize,
}

impl FlattenReport {
    /// Whether anything needs the user's attention.
    pub fn has_findings(&self) -> bool {
        self.flattened > 0 || !self.unsupported_3d.is_empty() || !self.unmodelled.is_empty() || self.tilted_planes > 0
    }

    /// The command-line text.
    pub fn message(&self) -> String {
        let mut lines = vec![format!("Flatten on import: {} of {} objects flattened to 2D.", self.flattened, self.examined)];
        let list = |m: &BTreeMap<String, usize>| m.iter().map(|(t, n)| format!("{n} {t}")).collect::<Vec<_>>().join(", ");
        if !self.unsupported_3d.is_empty() {
            lines.push(format!("Could not flatten (3D objects, kept as read): {}.", list(&self.unsupported_3d)));
        }
        if !self.unmodelled.is_empty() {
            lines.push(format!("Not modelled by AAD (kept as read): {}.", list(&self.unmodelled)));
        }
        if self.tilted_planes > 0 {
            lines.push(format!("{} object(s) lie on a plane that is not parallel to XY; their z values were zeroed.", self.tilted_planes));
        }
        lines.join("\n")
    }

    pub fn to_json(&self) -> Value {
        json!({
            "examined": self.examined,
            "flattened": self.flattened,
            "unsupported3d": self.unsupported_3d,
            "unmodelled": self.unmodelled,
            "tiltedPlanes": self.tilted_planes,
            "message": self.message(),
        })
    }
}

/// Project every entity of `d` onto the XY plane and report what could not be flattened.
pub fn flatten_drawing(d: &mut Drawing) -> FlattenReport {
    let mut r = FlattenReport::default();
    flatten_store(&mut d.model, &mut r);
    for l in &mut d.layouts {
        flatten_store(&mut l.entities, &mut r);
    }
    for b in d.blocks.values_mut() {
        flatten_store(&mut Arc::make_mut(b).entities, &mut r);
    }
    r
}

fn flatten_store(store: &mut EntityStore, r: &mut FlattenReport) {
    for h in store.handles() {
        r.examined += 1;
        let mut tilted = false;
        let mut changed = false;
        let mut unknown_type = None;
        store.modify(h, |e| {
            if let EntityKind::Unknown(u) = &e.kind {
                unknown_type = Some(u.dxf_type.clone());
                return;
            }
            let ex = e.common.extrusion;
            tilted = ex.x.abs() > 1e-9 || ex.y.abs() > 1e-9;
            let before = (e.kind.clone(), e.common.thickness);
            flatten_kind(&mut e.kind);
            e.common.thickness = 0.0;
            changed = before != (e.kind.clone(), e.common.thickness);
        });
        if let Some(t) = unknown_type {
            let map = if SOLID_TYPES.contains(&t.as_str()) { &mut r.unsupported_3d } else { &mut r.unmodelled };
            *map.entry(t).or_insert(0) += 1;
            continue;
        }
        if changed {
            r.flattened += 1;
        }
        if tilted {
            r.tilted_planes += 1;
        }
    }
}
