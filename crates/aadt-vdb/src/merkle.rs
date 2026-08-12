//! Merkle content-addressed version store for `.vdb` v4 (ADR-0021).
//!
//! Objects: blob → tree → commit; refs: `HEAD`. Unchanged entity blobs are
//! reused across commits so one drawing file holds many versions without
//! full-document copies.

use std::collections::{BTreeMap, HashMap, HashSet};
use std::path::Path;
use std::time::{SystemTime, UNIX_EPOCH};

use aadt_graph::Graph;
use aadt_model::{Document, EntityId};
use aadt_object::{AuditLog, ObjectStore};

use crate::buf::{Reader, VdbError, Writer};
use crate::codec::{self, decode_entity, encode_entity, encode_shell};
use crate::objects;

/// Trailer magic after object/graph/audit sections.
pub const MERK_TAG: &[u8; 4] = b"MERK";

/// Max number of `auto` commits retained (named ones are never pruned).
pub const MAX_AUTO_COMMITS: usize = 20;

/// BLAKE3 object id (32 bytes).
pub type ObjectHash = [u8; 32];

/// Kind byte stored with each CAS object.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
#[repr(u8)]
pub enum ObjectKind {
    Blob = 0,
    Tree = 1,
    Commit = 2,
}

impl ObjectKind {
    fn from_u8(v: u8) -> Result<Self, VdbError> {
        match v {
            0 => Ok(Self::Blob),
            1 => Ok(Self::Tree),
            2 => Ok(Self::Commit),
            other => Err(VdbError::Format(format!("unknown merkle kind {other}"))),
        }
    }
}

/// Commit kind for HISTORY UI / purge policy.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum CommitKind {
    Auto,
    Named,
}

impl CommitKind {
    pub fn as_str(self) -> &'static str {
        match self {
            Self::Auto => "auto",
            Self::Named => "named",
        }
    }

    fn from_str(s: &str) -> Self {
        if s.eq_ignore_ascii_case("named") {
            Self::Named
        } else {
            Self::Auto
        }
    }
}

/// Metadata for one commit (payload not included).
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct CommitMeta {
    /// Full hex object id.
    pub id: String,
    pub created_at: String,
    pub label: String,
    pub kind: String,
    pub summary: String,
    /// Parent commit hex, or empty for root.
    pub parent: String,
    pub tree: String,
}

/// In-memory Merkle store (CAS + refs).
#[derive(Debug, Default, Clone)]
pub struct MerkleStore {
    /// hash → (kind, payload bytes)
    objects: HashMap<ObjectHash, (ObjectKind, Vec<u8>)>,
    /// name → hash (`HEAD` is the tip commit)
    refs: BTreeMap<String, ObjectHash>,
}

impl MerkleStore {
    pub fn new() -> Self {
        Self::default()
    }

    pub fn head(&self) -> Option<ObjectHash> {
        self.refs.get("HEAD").copied()
    }

    pub fn set_head(&mut self, hash: ObjectHash) {
        self.refs.insert("HEAD".into(), hash);
    }

    pub fn object_count(&self) -> usize {
        self.objects.len()
    }

    pub fn get(&self, hash: &ObjectHash) -> Option<&(ObjectKind, Vec<u8>)> {
        self.objects.get(hash)
    }

    /// Insert raw payload under its content hash. Returns the hash.
    pub fn put(&mut self, kind: ObjectKind, payload: Vec<u8>) -> ObjectHash {
        let hash = hash_payload(kind, &payload);
        self.objects.entry(hash).or_insert((kind, payload));
        hash
    }

    /// Merge objects/refs from `other` (other wins on ref name conflict).
    pub fn merge_from(&mut self, other: &MerkleStore) {
        for (h, v) in &other.objects {
            self.objects.entry(*h).or_insert_with(|| v.clone());
        }
        for (k, v) in &other.refs {
            self.refs.insert(k.clone(), *v);
        }
    }

    /// Build a tree for `doc` (+ object sections), insert blobs, return tree hash.
    pub fn build_tree(
        &mut self,
        doc: &Document,
        objects: &ObjectStore,
        graph: &Graph,
        audit: &AuditLog,
    ) -> ObjectHash {
        let mut ents: Vec<&aadt_model::Entity> = doc.entities();
        ents.sort_by_key(|e| e.id.0);
        let mut entries: Vec<(u64, ObjectHash)> = Vec::with_capacity(ents.len());
        for e in ents {
            let blob = encode_entity(e);
            let h = self.put(ObjectKind::Blob, blob);
            entries.push((e.id.0, h));
        }

        let shell = encode_shell(doc);
        let shell_hash = self.put(ObjectKind::Blob, shell);

        let mut ow = Writer::new();
        objects::write_sections(&mut ow, objects, graph, audit);
        let obj_hash = self.put(ObjectKind::Blob, ow.into_bytes());

        let tree_bytes = encode_tree(&entries, &shell_hash, &obj_hash);
        self.put(ObjectKind::Tree, tree_bytes)
    }

    /// Tree hash of the commit at `HEAD`, if any.
    pub fn head_tree_hash(&self) -> Result<Option<ObjectHash>, VdbError> {
        let Some(head) = self.head() else {
            return Ok(None);
        };
        let commit = self.decode_commit(&head)?;
        Ok(Some(commit.tree))
    }

    /// Create a commit pointing at `tree` with `parent` (zeros = root).
    pub fn commit(
        &mut self,
        tree: ObjectHash,
        parent: Option<ObjectHash>,
        kind: CommitKind,
        label: &str,
        summary: &str,
    ) -> ObjectHash {
        let created_at = now_unix();
        let payload = encode_commit(
            &tree,
            parent.as_ref(),
            created_at,
            kind,
            label,
            summary,
        );
        let hash = self.put(ObjectKind::Commit, payload);
        self.set_head(hash);
        hash
    }

    /// Walk HEAD → parents (newest first).
    pub fn list_commits(&self) -> Result<Vec<CommitMeta>, VdbError> {
        let mut out = Vec::new();
        let mut cur = self.head();
        let mut seen = HashSet::new();
        while let Some(h) = cur {
            if !seen.insert(h) {
                break;
            }
            let c = self.decode_commit(&h)?;
            out.push(CommitMeta {
                id: hex::encode(h),
                created_at: c.created_at.to_string(),
                label: c.label,
                kind: c.kind.as_str().into(),
                summary: c.summary,
                parent: c.parent.map(hex::encode).unwrap_or_default(),
                tree: hex::encode(c.tree),
            });
            cur = match c.parent {
                Some(p) if self.objects.contains_key(&p) => Some(p),
                _ => None,
            };
        }
        Ok(out)
    }

    /// Materialize a document (+ object stores) from a commit hash.
    pub fn materialize(
        &self,
        commit: &ObjectHash,
    ) -> Result<(Document, ObjectStore, Graph, AuditLog), VdbError> {
        let c = self.decode_commit(commit)?;
        self.materialize_tree(&c.tree)
    }

    pub fn materialize_tree(
        &self,
        tree_hash: &ObjectHash,
    ) -> Result<(Document, ObjectStore, Graph, AuditLog), VdbError> {
        let tree = self.decode_tree(tree_hash)?;
        let shell = self.blob(&tree.shell)?;
        let mut doc = {
            let mut r = Reader::new(shell);
            codec::decode_document(&mut r)?
        };
        for (id, eh) in &tree.entities {
            let bytes = self.blob(eh)?;
            let mut ent = decode_entity(bytes)?;
            // Trust tree key for id consistency.
            ent.id = EntityId(*id);
            doc.restore_entity(ent);
        }
        let obj_bytes = self.blob(&tree.objects)?;
        let mut r = Reader::new(obj_bytes);
        let (objects, graph, audit) = objects::read_sections(&mut r, true)?;
        Ok((doc, objects, graph, audit))
    }

    /// Resolve a commit by full/prefix hex, or 1-based index into [`list_commits`].
    pub fn resolve_commit(&self, id: &str) -> Result<ObjectHash, VdbError> {
        if let Ok(n) = id.parse::<usize>() {
            if n >= 1 {
                let list = self.list_commits()?;
                if let Some(c) = list.get(n - 1) {
                    return parse_hex_hash(&c.id);
                }
            }
        }
        let needle = id.trim().to_ascii_lowercase();
        if needle.len() < 4 {
            return Err(VdbError::Format(
                "commit id too short (use hex prefix ≥4 or 1-based list index)".into(),
            ));
        }
        let list = self.list_commits()?;
        let mut matches: Vec<ObjectHash> = Vec::new();
        for c in &list {
            if c.id.starts_with(&needle) {
                matches.push(parse_hex_hash(&c.id)?);
            }
        }
        match matches.len() {
            0 => Err(VdbError::Format(format!("no commit matching {id:?}"))),
            1 => Ok(matches[0]),
            _ => Err(VdbError::Format(format!(
                "ambiguous commit prefix {id:?} ({} matches)",
                matches.len()
            ))),
        }
    }

    /// Keep HEAD chain's newest [`MAX_AUTO_COMMITS`] autos + all named; GC the rest.
    /// Returns the number of auto commits removed from the walkable chain.
    pub fn purge_auto_commits(&mut self) -> Result<usize, VdbError> {
        let list = self.list_commits()?;
        let mut keep: HashSet<ObjectHash> = HashSet::new();
        let mut autos = 0usize;
        let mut purged = 0usize;
        for c in &list {
            let h = parse_hex_hash(&c.id)?;
            if c.kind == "named" || autos < MAX_AUTO_COMMITS {
                keep.insert(h);
                if c.kind != "named" {
                    autos += 1;
                }
            } else {
                purged += 1;
            }
        }
        if let Some(head) = self.head() {
            keep.insert(head);
        }
        let reachable = self.collect_reachable(&keep)?;
        self.objects.retain(|h, _| reachable.contains(h));
        Ok(purged)
    }

    fn collect_reachable(&self, commits: &HashSet<ObjectHash>) -> Result<HashSet<ObjectHash>, VdbError> {
        let mut out = HashSet::new();
        for c in commits {
            self.mark_commit_reachable(c, &mut out)?;
        }
        Ok(out)
    }

    fn mark_commit_reachable(
        &self,
        commit: &ObjectHash,
        out: &mut HashSet<ObjectHash>,
    ) -> Result<(), VdbError> {
        if !out.insert(*commit) {
            return Ok(());
        }
        let c = self.decode_commit(commit)?;
        out.insert(c.tree);
        let tree = self.decode_tree(&c.tree)?;
        out.insert(tree.shell);
        out.insert(tree.objects);
        for (_, eh) in tree.entities {
            out.insert(eh);
        }
        Ok(())
    }

    fn blob(&self, hash: &ObjectHash) -> Result<&[u8], VdbError> {
        match self.objects.get(hash) {
            Some((ObjectKind::Blob, b)) => Ok(b.as_slice()),
            Some(_) => Err(VdbError::Format("expected blob object".into())),
            None => Err(VdbError::Format(format!(
                "missing blob {}",
                hex::encode(hash)
            ))),
        }
    }

    fn decode_commit(&self, hash: &ObjectHash) -> Result<CommitObj, VdbError> {
        let (kind, bytes) = self
            .objects
            .get(hash)
            .ok_or_else(|| VdbError::Format(format!("missing commit {}", hex::encode(hash))))?;
        if *kind != ObjectKind::Commit {
            return Err(VdbError::Format("object is not a commit".into()));
        }
        decode_commit(bytes)
    }

    fn decode_tree(&self, hash: &ObjectHash) -> Result<TreeObj, VdbError> {
        let (kind, bytes) = self
            .objects
            .get(hash)
            .ok_or_else(|| VdbError::Format(format!("missing tree {}", hex::encode(hash))))?;
        if *kind != ObjectKind::Tree {
            return Err(VdbError::Format("object is not a tree".into()));
        }
        decode_tree(bytes)
    }
}

struct CommitObj {
    tree: ObjectHash,
    parent: Option<ObjectHash>,
    created_at: u64,
    kind: CommitKind,
    label: String,
    summary: String,
}

struct TreeObj {
    entities: Vec<(u64, ObjectHash)>,
    shell: ObjectHash,
    objects: ObjectHash,
}

fn hash_payload(kind: ObjectKind, payload: &[u8]) -> ObjectHash {
    let mut hasher = blake3::Hasher::new();
    hasher.update(&[kind as u8]);
    hasher.update(payload);
    *hasher.finalize().as_bytes()
}

fn encode_tree(entries: &[(u64, ObjectHash)], shell: &ObjectHash, objects: &ObjectHash) -> Vec<u8> {
    let mut w = Writer::new();
    w.u32(entries.len() as u32);
    for (id, h) in entries {
        w.u64(*id);
        w.raw(h);
    }
    w.raw(shell);
    w.raw(objects);
    w.into_bytes()
}

fn decode_tree(bytes: &[u8]) -> Result<TreeObj, VdbError> {
    let mut r = Reader::new(bytes);
    let n = r.u32()? as usize;
    let mut entities = Vec::with_capacity(n);
    for _ in 0..n {
        let id = r.u64()?;
        let mut h = [0u8; 32];
        h.copy_from_slice(r.raw(32)?);
        entities.push((id, h));
    }
    let mut shell = [0u8; 32];
    shell.copy_from_slice(r.raw(32)?);
    let mut objects = [0u8; 32];
    objects.copy_from_slice(r.raw(32)?);
    Ok(TreeObj {
        entities,
        shell,
        objects,
    })
}

fn encode_commit(
    tree: &ObjectHash,
    parent: Option<&ObjectHash>,
    created_at: u64,
    kind: CommitKind,
    label: &str,
    summary: &str,
) -> Vec<u8> {
    let mut w = Writer::new();
    w.raw(tree);
    match parent {
        Some(p) => w.raw(p),
        None => w.raw(&[0u8; 32]),
    }
    w.u64(created_at);
    w.str(kind.as_str());
    w.str(label);
    w.str(summary);
    w.into_bytes()
}

fn decode_commit(bytes: &[u8]) -> Result<CommitObj, VdbError> {
    let mut r = Reader::new(bytes);
    let mut tree = [0u8; 32];
    tree.copy_from_slice(r.raw(32)?);
    let mut parent_raw = [0u8; 32];
    parent_raw.copy_from_slice(r.raw(32)?);
    let parent = if parent_raw == [0u8; 32] {
        None
    } else {
        Some(parent_raw)
    };
    let created_at = r.u64()?;
    let kind = CommitKind::from_str(&r.str()?);
    let label = r.str()?;
    let summary = r.str()?;
    Ok(CommitObj {
        tree,
        parent,
        created_at,
        kind,
        label,
        summary,
    })
}

fn now_unix() -> u64 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map(|d| d.as_secs())
        .unwrap_or(0)
}

/// Hex-encode a hash (lowercase).
pub fn hex_encode(h: &ObjectHash) -> String {
    hex::encode(h)
}

pub fn parse_hex_hash(s: &str) -> Result<ObjectHash, VdbError> {
    let s = s.trim();
    if s.len() != 64 {
        return Err(VdbError::Format(format!(
            "expected 64 hex chars, got {}",
            s.len()
        )));
    }
    let mut out = [0u8; 32];
    for i in 0..32 {
        let byte = u8::from_str_radix(&s[i * 2..i * 2 + 2], 16)
            .map_err(|_| VdbError::Format("invalid hex in commit id".into()))?;
        out[i] = byte;
    }
    Ok(out)
}

/// Write the `MERK` trailer.
pub fn write_merkle(w: &mut Writer, store: &MerkleStore) {
    w.raw(MERK_TAG);
    w.u32(store.objects.len() as u32);
    // Stable order for deterministic files when content identical.
    let mut keys: Vec<ObjectHash> = store.objects.keys().copied().collect();
    keys.sort_unstable();
    for h in keys {
        let (kind, payload) = store.objects.get(&h).expect("key from map");
        w.raw(&h);
        w.u8(*kind as u8);
        w.u32(payload.len() as u32);
        w.raw(payload);
    }
    w.u32(store.refs.len() as u32);
    for (name, h) in &store.refs {
        w.str(name);
        w.raw(h);
    }
}

/// Read a `MERK` trailer at the current reader position.
///
/// If the stream ends before a tag (v3 files), returns an empty store.
pub fn read_merkle(r: &mut Reader<'_>) -> Result<MerkleStore, VdbError> {
    let tag = match r.raw(MERK_TAG.len()) {
        Ok(t) => t,
        Err(_) => return Ok(MerkleStore::new()),
    };
    if tag != MERK_TAG {
        return Err(VdbError::Format(
            "expected MERK trailer after object sections".into(),
        ));
    }

    let mut store = MerkleStore::new();
    let n = r.u32()? as usize;
    for _ in 0..n {
        let mut h = [0u8; 32];
        h.copy_from_slice(r.raw(32)?);
        let kind = ObjectKind::from_u8(r.u8()?)?;
        let len = r.u32()? as usize;
        let payload = r.raw(len)?.to_vec();
        store.objects.insert(h, (kind, payload));
    }
    let nr = r.u32()? as usize;
    for _ in 0..nr {
        let name = r.str()?;
        let mut h = [0u8; 32];
        h.copy_from_slice(r.raw(32)?);
        store.refs.insert(name, h);
    }
    Ok(store)
}

/// Load only the Merkle store from a `.vdb` path (tip ignored).
pub fn load_merkle_from_path(path: impl AsRef<Path>) -> Result<MerkleStore, VdbError> {
    let bytes = std::fs::read(path.as_ref())?;
    load_merkle_from_bytes(&bytes)
}

/// Parse merkle from a full `.vdb` file buffer.
pub fn load_merkle_from_bytes(bytes: &[u8]) -> Result<MerkleStore, VdbError> {
    let mut r = Reader::new(bytes);
    let ver = crate::read_header(&mut r)?;
    let _doc = codec::decode_document(&mut r)?;
    let _ = shilpidb::SpatialGrid::read(&mut r);
    if ver >= 2 {
        let _ = objects::read_sections(&mut r, ver >= 3)?;
    }
    if ver >= 4 {
        read_merkle(&mut r)
    } else {
        Ok(MerkleStore::new())
    }
}

/// Short change summary for commits.
pub fn change_summary(doc: &Document) -> String {
    format!(
        "{} ents, {} dims, {} hatches, {} layers",
        doc.len(),
        doc.dimensions().len(),
        doc.hatches().len(),
        doc.layers().count()
    )
}

/// Entity-level diff of a Merkle commit vs the working tip document.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct HistoryDiff {
    pub added: Vec<u64>,
    pub removed: Vec<u64>,
    pub modified: Vec<u64>,
    pub unchanged: usize,
    pub shell_changed: bool,
}

impl HistoryDiff {
    pub fn narrate(&self) -> String {
        format!(
            "+{} added · ~{} modified · -{} removed · {} unchanged{}",
            self.added.len(),
            self.modified.len(),
            self.removed.len(),
            self.unchanged,
            if self.shell_changed {
                " · shell changed"
            } else {
                ""
            }
        )
    }

    pub fn is_empty(&self) -> bool {
        self.added.is_empty()
            && self.removed.is_empty()
            && self.modified.is_empty()
            && !self.shell_changed
    }
}

/// Diff commit `id` (hex prefix or 1-based LIST index) against `doc`.
/// Returns the diff plus the materialized baseline document (for ghost traces).
pub fn diff_commit_vs_doc(
    store: &mut MerkleStore,
    id: &str,
    doc: &Document,
    objects: &ObjectStore,
    graph: &Graph,
    audit: &AuditLog,
) -> Result<(HistoryDiff, Document), VdbError> {
    let commit = store.resolve_commit(id)?;
    let (old_doc, _, _, _) = store.materialize(&commit)?;
    let old_tree = {
        let c = store.decode_commit(&commit)?;
        store.decode_tree(&c.tree)?
    };
    let new_tree_hash = store.build_tree(doc, objects, graph, audit);
    let new_tree = store.decode_tree(&new_tree_hash)?;

    let old_map: BTreeMap<u64, ObjectHash> = old_tree.entities.into_iter().collect();
    let new_map: BTreeMap<u64, ObjectHash> = new_tree.entities.into_iter().collect();

    let mut diff = HistoryDiff {
        shell_changed: old_tree.shell != new_tree.shell,
        ..HistoryDiff::default()
    };
    for (id, nh) in &new_map {
        match old_map.get(id) {
            None => diff.added.push(*id),
            Some(oh) if oh != nh => diff.modified.push(*id),
            Some(_) => diff.unchanged += 1,
        }
    }
    for id in old_map.keys() {
        if !new_map.contains_key(id) {
            diff.removed.push(*id);
        }
    }
    diff.added.sort_unstable();
    diff.removed.sort_unstable();
    diff.modified.sort_unstable();
    Ok((diff, old_doc))
}

/// Options for a SAVE that may create a commit.
#[derive(Debug, Clone)]
pub struct CommitOptions {
    pub kind: CommitKind,
    pub label: String,
    /// When true (default), skip creating a commit if tip tree == HEAD tree.
    pub skip_if_unchanged: bool,
}

impl Default for CommitOptions {
    fn default() -> Self {
        Self {
            kind: CommitKind::Auto,
            label: "auto".into(),
            skip_if_unchanged: true,
        }
    }
}

impl CommitOptions {
    pub fn auto() -> Self {
        Self::default()
    }

    pub fn named(label: impl Into<String>) -> Self {
        Self {
            kind: CommitKind::Named,
            label: label.into(),
            skip_if_unchanged: false,
        }
    }
}

// Minimal hex helpers — avoid pulling a hex crate into the workspace root.
mod hex {
    pub fn encode(h: impl AsRef<[u8]>) -> String {
        const HEX: &[u8; 16] = b"0123456789abcdef";
        let b = h.as_ref();
        let mut s = String::with_capacity(b.len() * 2);
        for &x in b {
            s.push(HEX[(x >> 4) as usize] as char);
            s.push(HEX[(x & 0xf) as usize] as char);
        }
        s
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use aadt_geometry::{Point2, Segment};
    use aadt_model::Geometry;

    #[test]
    fn tree_dedups_unchanged_entity_blob() {
        let mut doc = Document::new();
        let a = doc.add_geometry(
            "0",
            Geometry::Line(Segment::new(Point2::new(0.0, 0.0), Point2::new(1.0, 0.0))),
        );
        let mut store = MerkleStore::new();
        let empty = ObjectStore::new();
        let g = Graph::new();
        let audit = AuditLog::new();
        let t1 = store.build_tree(&doc, &empty, &g, &audit);
        let blobs_after_first = store.object_count();

        doc.add_geometry(
            "0",
            Geometry::Line(Segment::new(Point2::new(0.0, 1.0), Point2::new(1.0, 1.0))),
        );
        let t2 = store.build_tree(&doc, &empty, &g, &audit);
        assert_ne!(t1, t2);
        // First entity blob reused — object count grows by less than a full second tree.
        let e0 = store
            .decode_tree(&t1)
            .unwrap()
            .entities
            .into_iter()
            .find(|(id, _)| EntityId(*id) == a)
            .unwrap()
            .1;
        let e0_again = store
            .decode_tree(&t2)
            .unwrap()
            .entities
            .into_iter()
            .find(|(id, _)| EntityId(*id) == a)
            .unwrap()
            .1;
        assert_eq!(e0, e0_again);
        assert!(store.object_count() > blobs_after_first);
    }
}
