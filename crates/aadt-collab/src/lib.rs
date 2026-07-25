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
//! The **network transport** is implemented on top of that primitive: [`sync`]
//! is a dependency-free ([`std::net`] only) TCP relay — a [`RelayServer`] hub and
//! a [`Client`] per participant — that ships operations as lines (see [`wire`])
//! and folds each arriving op in with [`Replica::integrate`]. Because the relay
//! never decides the markup set, out-of-order and duplicate delivery are safe; it
//! is verified over a loopback socket in `tests/network.rs`.
//!
//! **Local presence** (cursors / join / leave) rides the same TCP relay as
//! ephemeral `J`/`L`/`P` lines — not stored in the markup op log. A **hosted**
//! session service remains deferred (needs deployment, not engine logic).

pub mod op;
pub mod presence;
pub mod replica;
pub mod sync;
pub mod wire;

pub use op::{Action, Op, OpId};
pub use presence::{Presence, PresenceMap};
pub use replica::Replica;
pub use sync::{Client, RelayServer};
pub use wire::PresenceMsg;

/// Milestone in which cloud collaboration is implemented.
pub const MILESTONE: &str = "M8";
