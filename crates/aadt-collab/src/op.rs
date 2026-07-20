//! Operations — the unit of change shipped between collaborators.

use aadt_pdf::Markup;

/// A globally-unique operation id: the site that issued it and its local
/// sequence number there. Two sites never collide because the site id differs;
/// one site never collides because the sequence increments.
#[derive(Debug, Clone, PartialEq, Eq, Hash, PartialOrd, Ord)]
pub struct OpId {
    pub site: String,
    pub seq: u64,
}

impl OpId {
    pub fn new(site: impl Into<String>, seq: u64) -> Self {
        Self {
            site: site.into(),
            seq,
        }
    }
}

impl std::fmt::Display for OpId {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "{}#{}", self.site, self.seq)
    }
}

/// What an operation does to the shared markup set. Adds are identified by the
/// operation's own id; a remove tombstones the add it names — an OR-Set-style
/// structure, so add/remove ordering is unambiguous and convergence holds.
#[derive(Debug, Clone, PartialEq)]
pub enum Action {
    /// Add a markup; its id is the enclosing [`Op`]'s id.
    Add(Markup),
    /// Tombstone the markup added by `target`.
    Remove(OpId),
}

/// One operation: its id, a Lamport timestamp for total ordering, and the action.
#[derive(Debug, Clone, PartialEq)]
pub struct Op {
    pub id: OpId,
    /// Lamport clock — the primary sort key, so causally-later ops sort later.
    pub clock: u64,
    pub action: Action,
}

impl Op {
    /// The total-order key: Lamport clock first, then site, then sequence. A
    /// total order over all ops means every replica folds them identically, so
    /// they converge regardless of the order they arrive in.
    pub fn order_key(&self) -> (u64, &str, u64) {
        (self.clock, self.id.site.as_str(), self.id.seq)
    }
}
