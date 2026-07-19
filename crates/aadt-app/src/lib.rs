//! # aadt-app — Desktop application shell (spec §3, top of the stack)
//!
//! The command-line-first desktop application that composes the substrate (CAD
//! core, render canvas, command interpreter) with the AADT Engine (detection,
//! takeoff, docs) and the on-demand AI layer. Planned UI stack: egui + a custom
//! `wgpu` canvas (see ADR-0001), with the command line as the primary surface and
//! toolbars secondary (spec §2, principle 2).
//!
//! **Status:** skeleton. The interactive shell grows from **M1** onward (see
//! `docs/ROADMAP.md`); until then the runnable entry point is the `aadt` CLI in the
//! `aadt-cli` crate.

/// Milestone in which the interactive shell first ships.
pub const MILESTONE: &str = "M1";
