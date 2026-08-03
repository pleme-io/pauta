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
//! panel, the keymap and its drift gates — all unit-tested with no network.
//! **Not yet:** a tatara-lisp authored catalog (banken's `(defbancada)` /
//! `(defview)` surface), so the keymap is a Rust table held to `Action::ALL`
//! by a test rather than derived from an authored spec. `pending-pauta:
//! catalog`. **Not measured:** a live GitHub run, inherited from
//! `pending-metsuke: live-read`.

#![forbid(unsafe_code)]

pub mod app;
pub mod board;
pub mod feed;
pub mod keys;
pub mod render;

pub use app::{Action, Panel, PautaApp};
pub use board::{Board, PrRow};
pub use feed::PrFeed;
