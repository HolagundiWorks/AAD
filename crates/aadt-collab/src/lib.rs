//! # aadt-collab — Cloud Collaboration (separate track, spec §11 Phase 7)
//!
//! Opt-in real-time multi-user markup, architected **independently of the
//! local-first core**: nothing here touches the document model or the takeoff
//! pipeline, and AADT runs fully without it (spec §8).
//!
//! The testable heart is a **convergent operation log**. Every participant holds
//! a [`Replica`] of the shared markup set; local edits append operations, and
//! syncing is [`Replica::merge`]. Operations carry a total order (a Lamport
//! clock, then the site id), and the markup set is folded in that order, so any
//! two replicas that have exchanged the same operations compute the identical
//! set — regardless of the order syncs arrive, whether they arrive more than
//! once, or whether a participant was offline. That property (commutative,
//! idempotent, order-independent merge) is what makes real-time collaboration
//! safe, and it is proven here without any network.
//!
//! ```
//! # use aadt_collab::Replica;
//! # use aadt_pdf::{Markup, MarkupKind};
//! # use aadt_geometry::Point2;
//! let mut alice = Replica::new("alice");
//! let mut bob = Replica::new("bob");
//! alice.add_markup(Markup::new(MarkupKind::Cloud, Point2::new(0.0, 0.0), "alice", "note"));
//! bob.add_markup(Markup::new(MarkupKind::Text, Point2::new(1.0, 1.0), "bob", "check"));
//! let (a, b) = (alice.clone(), bob.clone());
//! alice.merge(&b);
//! bob.merge(&a);
//! assert_eq!(alice.markups().len(), bob.markups().len()); // converged
//! ```
//!
//! **Deferred:** the network transport (a WebSocket/session server that pumps
//! operations between replicas) and presence/cursors. Those need a running
//! service and a display; [`Replica::merge`] is the sync primitive they drive.

pub mod op;
pub mod replica;

pub use op::{Action, Op, OpId};
pub use replica::Replica;

/// Milestone in which cloud collaboration is implemented.
pub const MILESTONE: &str = "M8";
