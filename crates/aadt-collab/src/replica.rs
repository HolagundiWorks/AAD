//! A replica — one collaborator's convergent view of the shared markup set.
//!
//! Each participant holds a [`Replica`]. Local edits append operations; syncing
//! is [`merge`](Replica::merge), which absorbs another replica's operations. The
//! convergence guarantee: **any two replicas that have seen the same set of
//! operations compute the same markup set**, no matter the order they arrived —
//! because operations carry a total order (Lamport clock, then site) and the set
//! is folded in that order. A network transport is just a pump that calls
//! `merge`; it is deliberately out of scope here (the deferred, display/network
//! half of M8).

use std::collections::{BTreeMap, HashSet};

use aadt_pdf::Markup;

use crate::op::{Action, Op, OpId};

/// One collaborator's state: its site identity, a Lamport clock, and the log of
/// operations it has seen, keyed by total order.
#[derive(Debug, Clone)]
pub struct Replica {
    site: String,
    clock: u64,
    seq: u64,
    ops: BTreeMap<(u64, String, u64), Op>,
}

impl Replica {
    /// A fresh replica for `site`. Site ids must be unique per participant.
    pub fn new(site: impl Into<String>) -> Self {
        Self {
            site: site.into(),
            clock: 0,
            seq: 0,
            ops: BTreeMap::new(),
        }
    }

    pub fn site(&self) -> &str {
        &self.site
    }

    /// Add a markup locally, returning its id (also the id for a later remove).
    pub fn add_markup(&mut self, markup: Markup) -> OpId {
        let op = self.next_op(Action::Add(markup));
        let id = op.id.clone();
        self.record(op);
        id
    }

    /// Remove the markup added by `target` locally.
    pub fn remove(&mut self, target: OpId) {
        let op = self.next_op(Action::Remove(target));
        self.record(op);
    }

    /// Absorb another replica's operations. Idempotent (re-merging changes
    /// nothing) and commutative (merge order does not matter), so repeated,
    /// out-of-order syncs still converge. Advances the Lamport clock so future
    /// local edits sort after everything seen.
    pub fn merge(&mut self, other: &Replica) {
        for (key, op) in &other.ops {
            self.ops.entry(key.clone()).or_insert_with(|| op.clone());
        }
        self.clock = self.clock.max(other.clock);
    }

    /// The current markup set: every add that has not been tombstoned, in total
    /// order. Deterministic.
    pub fn markups(&self) -> Vec<(OpId, &Markup)> {
        let tombstoned: HashSet<&OpId> = self
            .ops
            .values()
            .filter_map(|op| match &op.action {
                Action::Remove(target) => Some(target),
                _ => None,
            })
            .collect();

        self.ops
            .values()
            .filter_map(|op| match &op.action {
                Action::Add(m) if !tombstoned.contains(&op.id) => Some((op.id.clone(), m)),
                _ => None,
            })
            .collect()
    }

    /// The participants whose operations this replica has seen.
    pub fn participants(&self) -> Vec<String> {
        let mut sites: Vec<String> = self
            .ops
            .keys()
            .map(|(_, site, _)| site.clone())
            .collect::<HashSet<_>>()
            .into_iter()
            .collect();
        sites.sort();
        sites
    }

    fn next_op(&mut self, action: Action) -> Op {
        self.clock += 1;
        self.seq += 1;
        Op {
            id: OpId::new(self.site.clone(), self.seq),
            clock: self.clock,
            action,
        }
    }

    fn record(&mut self, op: Op) {
        let (c, site, seq) = op.order_key();
        self.ops.insert((c, site.to_string(), seq), op);
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use aadt_geometry::Point2;
    use aadt_pdf::MarkupKind;

    fn note(author: &str, text: &str) -> Markup {
        Markup::new(MarkupKind::Cloud, Point2::new(0.0, 0.0), author, text)
    }

    /// Text of the markups on a replica, in order.
    fn texts(r: &Replica) -> Vec<String> {
        r.markups().iter().map(|(_, m)| m.note.clone()).collect()
    }

    #[test]
    fn local_add_and_remove() {
        let mut r = Replica::new("alice");
        let a = r.add_markup(note("alice", "first"));
        r.add_markup(note("alice", "second"));
        assert_eq!(texts(&r), vec!["first", "second"]);
        r.remove(a);
        assert_eq!(texts(&r), vec!["second"], "tombstoned add disappears");
    }

    #[test]
    fn two_replicas_converge_after_mutual_merge() {
        let mut alice = Replica::new("alice");
        let mut bob = Replica::new("bob");

        alice.add_markup(note("alice", "a1"));
        bob.add_markup(note("bob", "b1"));
        bob.add_markup(note("bob", "b2"));

        // Exchange operations.
        let (a_snapshot, b_snapshot) = (alice.clone(), bob.clone());
        alice.merge(&b_snapshot);
        bob.merge(&a_snapshot);

        // Both see the same set, in the same order.
        assert_eq!(texts(&alice), texts(&bob));
        assert_eq!(texts(&alice).len(), 3);
    }

    #[test]
    fn merge_is_order_independent() {
        // Three sites edit concurrently; merging in different orders converges.
        let mut a = Replica::new("a");
        let mut b = Replica::new("b");
        let mut c = Replica::new("c");
        a.add_markup(note("a", "from-a"));
        b.add_markup(note("b", "from-b"));
        c.add_markup(note("c", "from-c"));
        let (sa, sb, sc) = (a.clone(), b.clone(), c.clone());

        // One peer merges a,b,c; another merges c,b,a.
        let mut x = Replica::new("x");
        x.merge(&sa);
        x.merge(&sb);
        x.merge(&sc);
        let mut y = Replica::new("y");
        y.merge(&sc);
        y.merge(&sb);
        y.merge(&sa);

        assert_eq!(
            texts(&x),
            texts(&y),
            "merge order must not affect the result"
        );
        assert_eq!(texts(&x).len(), 3);
    }

    #[test]
    fn a_remote_remove_wins_after_sync() {
        // Alice adds; Bob (having seen it) removes it. After sync both agree it's gone.
        let mut alice = Replica::new("alice");
        let id = alice.add_markup(note("alice", "draft"));
        let mut bob = Replica::new("bob");
        bob.merge(&alice);
        bob.remove(id);

        alice.merge(&bob);
        assert!(texts(&alice).is_empty(), "the remove propagated");
        assert_eq!(texts(&alice), texts(&bob));
    }

    #[test]
    fn merge_is_idempotent() {
        let mut alice = Replica::new("alice");
        alice.add_markup(note("alice", "x"));
        let mut bob = Replica::new("bob");
        bob.add_markup(note("bob", "y"));
        let snapshot = alice.clone();
        bob.merge(&snapshot);
        let once = texts(&bob);
        bob.merge(&snapshot); // merge the same ops again
        assert_eq!(texts(&bob), once, "re-merging changes nothing");
    }

    #[test]
    fn participants_are_tracked() {
        let mut alice = Replica::new("alice");
        alice.add_markup(note("alice", "x"));
        let mut bob = Replica::new("bob");
        bob.add_markup(note("bob", "y"));
        alice.merge(&bob);
        assert_eq!(alice.participants(), vec!["alice", "bob"]);
    }
}
