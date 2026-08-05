//! # aadt-object — the Object Database (AI-native Phase 1, ADR-0012)
//!
//! Typed **intelligent engineering objects** layered over the document: a Wall,
//! Room, Door, Column, … each with a stable id, the entity ids that carry its
//! geometry, engineering properties, an AI-confidence score, and a revision
//! stamp. Geometry stays in the [`Document`](aadt_model::Document) — the
//! source of truth — objects *reference* entities, never replace them.
//!
//! The crate also provides the **append-only audit log** of the vision's
//! security model: every object mutation records who / when / what / prev / new
//! / AI-suggested / approval / reason. Nothing is deleted; everything is
//! versioned. M14.3 adds an optional HMAC-SHA256 hash chain over each entry.
//! M14.2 adds principals, capability sets, and per-object ACL props.

pub mod hmac;
pub mod perms;

use std::collections::BTreeMap;

use aadt_model::EntityId;

pub use perms::{
    allows_object, clear_object_acl, command_capability, list_object_acl, object_acl, set_object_acl,
    Capability, CapabilitySet, ObjectAccess, Principal, Role,
};

/// Stable id of an engineering object within a drawing's object store.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, PartialOrd, Ord, Default)]
pub struct ObjectId(pub u64);

/// The engineering type of an object.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord)]
pub enum ObjectKind {
    Wall,
    Room,
    Door,
    Window,
    Column,
    Beam,
    Furniture,
}

impl ObjectKind {
    /// All kinds, in stable order.
    pub const ALL: [ObjectKind; 7] = [
        ObjectKind::Wall,
        ObjectKind::Room,
        ObjectKind::Door,
        ObjectKind::Window,
        ObjectKind::Column,
        ObjectKind::Beam,
        ObjectKind::Furniture,
    ];

    /// Stable token used for persistence and display.
    pub fn as_str(self) -> &'static str {
        match self {
            ObjectKind::Wall => "Wall",
            ObjectKind::Room => "Room",
            ObjectKind::Door => "Door",
            ObjectKind::Window => "Window",
            ObjectKind::Column => "Column",
            ObjectKind::Beam => "Beam",
            ObjectKind::Furniture => "Furniture",
        }
    }

    /// Parse a persisted token (case-insensitive).
    pub fn parse(s: &str) -> Option<ObjectKind> {
        ObjectKind::ALL
            .into_iter()
            .find(|k| k.as_str().eq_ignore_ascii_case(s.trim()))
    }
}

impl std::fmt::Display for ObjectKind {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str(self.as_str())
    }
}

/// One intelligent engineering object.
#[derive(Debug, Clone, PartialEq)]
pub struct EngObject {
    pub id: ObjectId,
    pub kind: ObjectKind,
    /// Document entities carrying this object's geometry (source of truth).
    pub entities: Vec<EntityId>,
    /// Engineering properties (material, thickness, area, …). `BTreeMap` keeps
    /// iteration — and thus persistence — deterministic.
    pub props: BTreeMap<String, String>,
    /// Confidence of the process that produced the object (`1.0` for
    /// deterministic detection; model probability for AI classification).
    pub confidence: f32,
    /// `Document::revision` at the time the object was created/updated.
    pub revision: u64,
}

impl EngObject {
    /// Property accessor.
    pub fn prop(&self, key: &str) -> Option<&str> {
        self.props.get(key).map(String::as_str)
    }
}

/// The object database: all engineering objects of one drawing.
#[derive(Debug, Clone, Default)]
pub struct ObjectStore {
    objects: BTreeMap<ObjectId, EngObject>,
    next_id: u64,
}

impl ObjectStore {
    pub fn new() -> Self {
        Self::default()
    }

    /// Insert a new object, assigning the next id. Returns the assigned id.
    pub fn insert(
        &mut self,
        kind: ObjectKind,
        entities: Vec<EntityId>,
        props: BTreeMap<String, String>,
        confidence: f32,
        revision: u64,
    ) -> ObjectId {
        self.next_id += 1;
        let id = ObjectId(self.next_id);
        self.objects.insert(
            id,
            EngObject {
                id,
                kind,
                entities,
                props,
                confidence,
                revision,
            },
        );
        id
    }

    /// Restore a persisted object keeping its id (advances the id sequence).
    pub fn restore(&mut self, object: EngObject) {
        self.next_id = self.next_id.max(object.id.0);
        self.objects.insert(object.id, object);
    }

    pub fn get(&self, id: ObjectId) -> Option<&EngObject> {
        self.objects.get(&id)
    }

    pub fn get_mut(&mut self, id: ObjectId) -> Option<&mut EngObject> {
        self.objects.get_mut(&id)
    }

    pub fn len(&self) -> usize {
        self.objects.len()
    }

    pub fn is_empty(&self) -> bool {
        self.objects.is_empty()
    }

    /// All objects, ascending by id (deterministic).
    pub fn iter(&self) -> impl Iterator<Item = &EngObject> {
        self.objects.values()
    }

    /// Objects of one kind, ascending by id.
    pub fn by_kind(&self, kind: ObjectKind) -> Vec<&EngObject> {
        self.objects.values().filter(|o| o.kind == kind).collect()
    }

    /// The objects that reference `entity`, ascending by id.
    pub fn find_by_entity(&self, entity: EntityId) -> Vec<&EngObject> {
        self.objects
            .values()
            .filter(|o| o.entities.contains(&entity))
            .collect()
    }

    /// Count per kind, in [`ObjectKind::ALL`] order (zero counts omitted).
    pub fn kind_counts(&self) -> Vec<(ObjectKind, usize)> {
        ObjectKind::ALL
            .into_iter()
            .filter_map(|k| {
                let n = self.objects.values().filter(|o| o.kind == k).count();
                (n > 0).then_some((k, n))
            })
            .collect()
    }
}

// ---------------------------------------------------------------------------
// Audit
// ---------------------------------------------------------------------------

/// One append-only audit record for an object mutation.
#[derive(Debug, Clone, PartialEq)]
pub struct AuditEntry {
    /// Monotonic sequence number within the log.
    pub seq: u64,
    /// Unix timestamp (seconds).
    pub at_unix: u64,
    /// OS user that performed the action.
    pub user: String,
    /// Action verb (`promote`, `update`, `classify`, …).
    pub action: String,
    /// The object acted on.
    pub object: ObjectId,
    /// Serialized previous state summary (empty for creation).
    pub prev: String,
    /// Serialized new state summary.
    pub new: String,
    /// Whether the change originated from an AI suggestion.
    pub ai_suggested: bool,
    /// Whether a human approved it (deterministic pipeline actions are `true`).
    pub approved: bool,
    /// Free-form reason.
    pub reason: String,
    /// Hex HMAC of the previous entry (`""` = genesis or unsigned legacy).
    pub prev_hash: String,
    /// Hex HMAC-SHA256 of this entry's canonical payload (`""` = unsigned).
    pub hash: String,
}

impl AuditEntry {
    /// Canonical bytes hashed for [`AuditEntry::hash`] (excludes `hash` itself).
    pub fn canonical_payload(&self) -> Vec<u8> {
        let mut buf = Vec::new();
        buf.extend_from_slice(&self.seq.to_le_bytes());
        buf.extend_from_slice(&self.at_unix.to_le_bytes());
        buf.extend_from_slice(self.user.as_bytes());
        buf.push(0);
        buf.extend_from_slice(self.action.as_bytes());
        buf.push(0);
        buf.extend_from_slice(&self.object.0.to_le_bytes());
        buf.extend_from_slice(self.prev.as_bytes());
        buf.push(0);
        buf.extend_from_slice(self.new.as_bytes());
        buf.push(0);
        buf.push(u8::from(self.ai_suggested));
        buf.push(u8::from(self.approved));
        buf.extend_from_slice(self.reason.as_bytes());
        buf.push(0);
        buf.extend_from_slice(self.prev_hash.as_bytes());
        buf
    }
}

/// Append-only audit log. Entries are never removed or rewritten.
#[derive(Debug, Clone, Default)]
pub struct AuditLog {
    entries: Vec<AuditEntry>,
    next_seq: u64,
    /// When set, new entries are HMAC-chained; [`AuditLog::verify_chain`] checks them.
    signing_key: Option<Vec<u8>>,
}

/// The current OS user, best-effort (`local` when unknown).
pub fn current_user() -> String {
    std::env::var("USER")
        .or_else(|_| std::env::var("USERNAME"))
        .unwrap_or_else(|_| "local".into())
}

fn now_unix() -> u64 {
    std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map(|d| d.as_secs())
        .unwrap_or(0)
}

impl AuditLog {
    pub fn new() -> Self {
        Self::default()
    }

    /// Install (or clear) the HMAC signing key used for new entries.
    pub fn set_signing_key(&mut self, key: Option<Vec<u8>>) {
        self.signing_key = key.filter(|k| !k.is_empty());
    }

    /// Whether new entries will be signed.
    pub fn is_signing(&self) -> bool {
        self.signing_key.is_some()
    }

    /// Append a new entry (seq + timestamp + user filled in). Returns its seq.
    /// When a signing key is set, fills `prev_hash` / `hash` (HMAC-SHA256).
    #[allow(clippy::too_many_arguments)]
    pub fn record(
        &mut self,
        action: &str,
        object: ObjectId,
        prev: &str,
        new: &str,
        ai_suggested: bool,
        approved: bool,
        reason: &str,
    ) -> u64 {
        self.next_seq += 1;
        let seq = self.next_seq;
        let prev_hash = self
            .entries
            .last()
            .map(|e| e.hash.clone())
            .unwrap_or_default();
        let mut entry = AuditEntry {
            seq,
            at_unix: now_unix(),
            user: current_user(),
            action: action.into(),
            object,
            prev: prev.into(),
            new: new.into(),
            ai_suggested,
            approved,
            reason: reason.into(),
            prev_hash,
            hash: String::new(),
        };
        if let Some(key) = &self.signing_key {
            let mac = hmac::hmac_sha256(key, &entry.canonical_payload());
            entry.hash = hmac::hex_encode(&mac);
        }
        self.entries.push(entry);
        seq
    }

    /// Restore a persisted entry (advances the sequence). Does not re-sign.
    pub fn restore(&mut self, entry: AuditEntry) {
        self.next_seq = self.next_seq.max(entry.seq);
        self.entries.push(entry);
    }

    /// All entries, oldest first.
    pub fn entries(&self) -> &[AuditEntry] {
        &self.entries
    }

    pub fn len(&self) -> usize {
        self.entries.len()
    }

    pub fn is_empty(&self) -> bool {
        self.entries.is_empty()
    }

    /// Entries touching one object, oldest first.
    pub fn for_object(&self, id: ObjectId) -> Vec<&AuditEntry> {
        self.entries.iter().filter(|e| e.object == id).collect()
    }

    /// Verify the HMAC chain. Unsigned (empty-hash) entries are skipped.
    /// Returns `Ok(n)` signed entries checked, or `Err(seq)` of the first bad one.
    pub fn verify_chain(&self, key: &[u8]) -> Result<usize, u64> {
        let mut checked = 0usize;
        for (i, e) in self.entries.iter().enumerate() {
            if e.hash.is_empty() {
                continue;
            }
            if !e.prev_hash.is_empty() {
                let ok = i
                    .checked_sub(1)
                    .and_then(|j| self.entries.get(j))
                    .is_some_and(|prev| prev.hash == e.prev_hash);
                if !ok {
                    return Err(e.seq);
                }
            }
            let mac = hmac::hmac_sha256(key, &e.canonical_payload());
            if e.hash != hmac::hex_encode(&mac) {
                return Err(e.seq);
            }
            checked += 1;
        }
        Ok(checked)
    }
}

/// A one-line state summary used in audit `prev`/`new` fields.
pub fn summarize(o: &EngObject) -> String {
    let props: Vec<String> = o.props.iter().map(|(k, v)| format!("{k}={v}")).collect();
    format!(
        "{} entities={} conf={:.2} {}",
        o.kind,
        o.entities.len(),
        o.confidence,
        props.join(" ")
    )
}

#[cfg(test)]
mod tests {
    use super::*;

    fn props(pairs: &[(&str, &str)]) -> BTreeMap<String, String> {
        pairs
            .iter()
            .map(|(k, v)| (k.to_string(), v.to_string()))
            .collect()
    }

    #[test]
    fn insert_assigns_sequential_ids_and_queries_work() {
        let mut store = ObjectStore::new();
        let w = store.insert(
            ObjectKind::Wall,
            vec![EntityId(1), EntityId(2)],
            props(&[("thickness", "0.23"), ("length", "4.0")]),
            1.0,
            7,
        );
        let r = store.insert(ObjectKind::Room, vec![EntityId(9)], props(&[]), 1.0, 7);
        assert_eq!(w, ObjectId(1));
        assert_eq!(r, ObjectId(2));
        assert_eq!(store.len(), 2);
        assert_eq!(store.get(w).unwrap().prop("thickness"), Some("0.23"));
        assert_eq!(store.by_kind(ObjectKind::Wall).len(), 1);
        assert_eq!(store.find_by_entity(EntityId(2))[0].id, w);
        assert!(store.find_by_entity(EntityId(99)).is_empty());
        assert_eq!(
            store.kind_counts(),
            vec![(ObjectKind::Wall, 1), (ObjectKind::Room, 1)]
        );
    }

    #[test]
    fn restore_preserves_ids_and_advances_sequence() {
        let mut store = ObjectStore::new();
        store.restore(EngObject {
            id: ObjectId(41),
            kind: ObjectKind::Column,
            entities: vec![EntityId(3)],
            props: props(&[]),
            confidence: 0.9,
            revision: 1,
        });
        let next = store.insert(ObjectKind::Beam, vec![], props(&[]), 1.0, 1);
        assert!(next.0 > 41, "id sequence advanced past restored ids");
        assert_eq!(store.get(ObjectId(41)).unwrap().kind, ObjectKind::Column);
    }

    #[test]
    fn kind_tokens_round_trip() {
        for k in ObjectKind::ALL {
            assert_eq!(ObjectKind::parse(k.as_str()), Some(k));
        }
        assert_eq!(ObjectKind::parse("wall"), Some(ObjectKind::Wall));
        assert_eq!(ObjectKind::parse("nope"), None);
    }

    #[test]
    fn audit_is_append_only_with_monotonic_seq() {
        let mut log = AuditLog::new();
        let a = log.record("promote", ObjectId(1), "", "Wall …", false, true, "detect");
        let b = log.record(
            "update",
            ObjectId(1),
            "Wall …",
            "Wall …2",
            true,
            false,
            "ai",
        );
        assert!(b > a);
        assert_eq!(log.len(), 2);
        assert_eq!(log.for_object(ObjectId(1)).len(), 2);
        assert!(log.entries()[0].approved);
        assert!(log.entries()[1].ai_suggested);
        // Restore keeps sequence monotonic.
        let mut log2 = AuditLog::new();
        for e in log.entries() {
            log2.restore(e.clone());
        }
        let c = log2.record("x", ObjectId(2), "", "", false, true, "");
        assert!(c > b);
    }

    #[test]
    fn signed_audit_chain_verifies_and_detects_tamper() {
        let mut log = AuditLog::new();
        log.set_signing_key(Some(b"test-key".to_vec()));
        log.record("promote", ObjectId(1), "", "Wall", false, true, "detect");
        log.record("update", ObjectId(1), "Wall", "Wall2", false, true, "edit");
        assert_eq!(log.verify_chain(b"test-key"), Ok(2));
        assert!(!log.entries()[0].hash.is_empty());
        assert_eq!(log.entries()[1].prev_hash, log.entries()[0].hash);
        log.entries[1].new = "tampered".into();
        // Recompute would fail — hash no longer matches payload.
        assert_eq!(log.verify_chain(b"test-key"), Err(2));
    }
}
