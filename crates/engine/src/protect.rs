//! Layer protection enforced by the engine: objects on a locked or frozen layer are not modified.
//!
//! Many editing commands already skip protected objects themselves, but each does it on its own.
//! This is the one check every undoable command goes through afterwards, so a command that forgets
//! (or one added later) cannot change a protected object: if the command replaced or removed an
//! object that was on a locked or frozen layer, or put a new object on one, the drawing is put back
//! and the command fails with a message that names the layer. The command is refused as a whole;
//! it is never half applied.
//!
//! What counts: the layer's flags *before* the command, so a command that unlocks a layer and then
//! edits it is still refused. Layer table changes (lock, unlock, freeze, thaw, switching a drawing
//! space) are not object edits and pass. Selecting a protected object is not an edit and is not
//! checked here. Objects inside block definitions are not checked (a known gap, see
//! docs/ACTIVE_DRAWING_SPACE.md).

use std::sync::Arc;

use aad_doc::{Drawing, EntityStore, Handle};

use crate::{EngineError, Session};

/// Why an object may not change.
#[derive(Debug, PartialEq, Eq)]
pub struct Protected {
    pub handle: Handle,
    pub layer: String,
    pub frozen: bool,
    pub added: bool,
}

impl Protected {
    pub fn message(&self) -> String {
        let why = if self.frozen { "frozen" } else { "locked" };
        let what = if self.added { "an object cannot be added on" } else { "object cannot be changed on" };
        format!(
            "layer `{}` is {why}: {what} it ({}). Nothing was changed. Unlock or thaw the layer, or switch the drawing space",
            self.layer,
            self.handle.hex()
        )
    }
}

fn protection(d: &Drawing, layer: &str) -> Option<bool> {
    d.layer(layer).and_then(|l| {
        if l.frozen {
            Some(true)
        } else if l.locked {
            Some(false)
        } else {
            None
        }
    })
}

fn check_store(before: &Drawing, b: &EntityStore, a: &EntityStore, after: &Drawing) -> Option<Protected> {
    if b.same_as(a) {
        return None;
    }
    // Replaced or added in `after`.
    for e in a.changed_since(b) {
        match b.get(e.handle) {
            Some(old) => {
                if let Some(frozen) = protection(before, &old.common.layer) {
                    return Some(Protected { handle: e.handle, layer: old.common.layer.clone(), frozen, added: false });
                }
            }
            None => {
                if let Some(frozen) = protection(after, &e.common.layer).or_else(|| protection(before, &e.common.layer)) {
                    return Some(Protected { handle: e.handle, layer: e.common.layer.clone(), frozen, added: true });
                }
            }
        }
    }
    // Removed from `before`.
    for e in b.changed_since(a) {
        if a.get(e.handle).is_none()
            && let Some(frozen) = protection(before, &e.common.layer)
        {
            return Some(Protected { handle: e.handle, layer: e.common.layer.clone(), frozen, added: false });
        }
    }
    None
}

/// The first object of model space or a layout that `after` changed although it was protected in `before`.
pub fn first_protected_change(before: &Drawing, after: &Drawing) -> Option<Protected> {
    if let Some(p) = check_store(before, &before.model, &after.model, after) {
        return Some(p);
    }
    for l in &after.layouts {
        let old = before.layouts.iter().find(|o| o.name == l.name);
        match old {
            Some(o) => {
                if let Some(p) = check_store(before, &o.entities, &l.entities, after) {
                    return Some(p);
                }
            }
            None => {
                if let Some(p) = check_store(before, &EntityStore::default(), &l.entities, after) {
                    return Some(p);
                }
            }
        }
    }
    None
}

impl Session {
    /// After command `cmd` changed the drawing from `before`: when it changed a protected object,
    /// put the drawing and selection back and return the error to report.
    pub(crate) fn refuse_protected(&mut self, uid: Option<u64>, before: &Arc<Drawing>, selection: &[Handle], cmd: &str) -> crate::Result<()> {
        // Layer table commands: renaming a layer re-points its objects, which is not an edit of them.
        // External reference and floor/project commands manage the locked overlay layers of other
        // drawings (attaching, reloading and detaching xrefs adds and removes the objects on them).
        if matches!(cmd, "layer" | "layer.set") || cmd.starts_with("xref.") || cmd.starts_with("floor.") || cmd.starts_with("project.") {
            return Ok(());
        }
        let st = match uid {
            Some(uid) => self.docs.iter_mut().find(|d| d.uid == uid),
            None => self.state_mut().ok(),
        };
        let Some(st) = st.filter(|st| !Arc::ptr_eq(before, &st.doc)) else { return Ok(()) };
        let Some(p) = first_protected_change(before, &st.doc) else { return Ok(()) };
        st.doc = before.clone();
        st.selection = selection.to_vec();
        Err(EngineError::BadParams { cmd: cmd.into(), msg: p.message() })
    }
}
