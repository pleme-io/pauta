//! # pauta — the PR-review quadro
//!
//! *Pauta* is Brazilian-Portuguese for the **docket**: everything set down
//! for your judgment, in order. That is the main screen — every pull request
//! awaiting you, read through [`metsuke`] and rendered with `egaku`.
//!
//! ## What it is
//!
//! A QUADRO (terminal UI) over two seams:
//!
//! - `F: metsuke::Forge` — where PRs, diffs and threads come from.
//! - `R: metsuke::ForgeReview` — where a verdict goes.
//!
//! They are independent, so the runtime is byte-identical against a mock,
//! against a live `gh`, and against a build with neither.
//!
//! ## Two things it does on purpose
//!
//! **The docket refreshes itself** (QUADRO T14), adopted from the first
//! commit rather than retrofitted. The read runs on tokio's blocking pool via
//! `izumi::refresh`, never on the task that owns the terminal — a `gh` call
//! there would freeze input for its duration, and a slow forge is exactly
//! when the keyboard matters. See [`feed`].
//!
//! **A binding verdict takes two keystrokes.** `Verdict::is_binding()`
//! distinguishes a comment from an approval or a request-for-changes;
//! approving notifies the author and changes mergeability. The first chord
//! resolves and previews the typed plan ([`app::Panel::Pending`]), a second
//! commits. This is banken's `(defbancada)` shape, and it is the only path
//! that reaches `submit_review`.
//!
//! ## Tier honesty
//!
//! **Shipped:** the docket model, the live plane, the verdict gate, the diff
//! panel, and the authored `(defpautaaction)` catalog that the keymap, the
//! legend and the gate are all DERIVED from. 36 tests, no network.
//!
//! `pending-pauta: catalog` is **closed** for the action vocabulary — see
//! [`pauta_spec`]. The gate is verified non-vacuously: authoring `approve`
//! as `:gate free` turns three tests red, including the one whose name
//! promises only a confirm reaches the forge.
//!
//! Still authored in Rust, not lisp: the column set and the sort default.
//! Those want a `(defpautaview)` and it is not built. `pending-pauta:
//! view-catalog`.
//!
//! The read path is proven live through metsuke (`pending-metsuke:
//! live-read` closed 2026-08-03). The WRITE path is not — nobody should
//! approve a real pull request to satisfy a test suite, so `submit_review`
//! is exercised against `MockForge` only.

#![forbid(unsafe_code)]

pub mod app;
pub mod archetype;
pub mod board;
pub mod feed;
pub mod keys;
pub mod render;

pub use app::{Action, Panel, PautaApp};
pub use board::{Board, PrRow};
pub use feed::PrFeed;
pub use pauta_spec::{Catalog, Effect, Gate};
