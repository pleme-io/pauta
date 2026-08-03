//! The pauta runtime: the docket, the diff, and the verdict gate.
//!
//! Two seams, banken's shape: `F: Forge` is where PRs come from, `R:
//! ForgeReview` is where a verdict goes. They are independent — pauta reads a
//! live forge and records into a mock, or the reverse, without either knowing.
//!
//! The screen is a sum type ([`Panel`]) so an illegal screen state has no
//! representation: pauta is always showing exactly one of {the docket, a
//! diff, a pending verdict awaiting confirmation, a message}.

use std::sync::Arc;

use egaku::{DiffView, KeyMap};
use metsuke::{Forge, ForgeReview, Involvement, PrSummary, Verdict};

use crate::board::Board;
use crate::feed::PrFeed;

/// What the operator can ask for.
///
/// Deliberately unit-only. `#[derive(KindStr, AllVariants)]` is UNIT-ONLY
/// (`tatara-rust-derive/src/enum_fold.rs:138`), and a payload-carrying action
/// cannot compile with them — so the target of an action is read from the
/// cursor at dispatch time rather than baked into the variant. That keeps the
/// enum derivable later without a rewrite.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Action {
    SelectNext,
    SelectPrev,
    OpenDiff,
    CloseDiff,
    ToggleDiffMode,
    CycleLane,
    Approve,
    RequestChanges,
    Confirm,
    Cancel,
    Quit,
}

impl Action {
    /// Every variant, hand-listed exactly once and then TESTED against the
    /// keymap, because the derive cannot be used here (see the enum's note).
    /// A hand list nobody checks is the drift this fleet keeps finding; a
    /// hand list a test holds to the keymap is merely a hand list.
    pub const ALL: &'static [Action] = &[
        Action::SelectNext,
        Action::SelectPrev,
        Action::OpenDiff,
        Action::CloseDiff,
        Action::ToggleDiffMode,
        Action::CycleLane,
        Action::Approve,
        Action::RequestChanges,
        Action::Confirm,
        Action::Cancel,
        Action::Quit,
    ];

    #[must_use]
    pub fn slug(self) -> &'static str {
        match self {
            Self::SelectNext => "select-next",
            Self::SelectPrev => "select-prev",
            Self::OpenDiff => "open-diff",
            Self::CloseDiff => "close-diff",
            Self::ToggleDiffMode => "toggle-diff-mode",
            Self::CycleLane => "cycle-lane",
            Self::Approve => "approve",
            Self::RequestChanges => "request-changes",
            Self::Confirm => "confirm",
            Self::Cancel => "cancel",
            Self::Quit => "quit",
        }
    }
}

/// Exactly one of these is on screen.
pub enum Panel {
    /// The docket.
    Board,
    /// A PR's diff.
    Diff(Box<DiffView>),
    /// A binding verdict, resolved and previewed, awaiting a second key.
    ///
    /// This is banken's `(defbancada)` two-keystroke shape and it exists
    /// because [`Verdict::is_binding`] is true here: approving notifies the
    /// author and changes the PR's mergeability. The chord resolves and shows
    /// the typed plan; a second key commits.
    Pending { pr: PrSummary, verdict: Verdict },
    /// A result or an error to read.
    Message(String),
}

/// The app.
pub struct PautaApp<F: Forge, R: ForgeReview> {
    forge: Arc<F>,
    review: R,
    board: Board,
    panel: Panel,
    keys: KeyMap<Action>,
    done: bool,
    feed: Option<PrFeed>,
    /// A short label naming where the rows came from — tier honesty in the UI
    /// itself, so a fixture-backed docket never reads as a live one.
    source_label: String,
}

impl<F: Forge, R: ForgeReview> PautaApp<F, R> {
    /// Build the app over a forge and a review sink.
    #[must_use]
    pub fn new(
        forge: impl Into<Arc<F>>,
        review: R,
        lane: Involvement,
        source_label: impl Into<String>,
    ) -> Self {
        Self {
            forge: forge.into(),
            review,
            board: Board::new(lane),
            panel: Panel::Board,
            keys: crate::keys::keymap(),
            done: false,
            feed: None,
            source_label: source_label.into(),
        }
    }

    /// Attach the T14 live plane. Opt-in and builder-style: an app without a
    /// feed is input-only, which is what every synchronous test wants.
    ///
    /// Requires a tokio runtime, which is why it is not done in `new`.
    #[must_use]
    pub fn with_feed(mut self, interval: std::time::Duration) -> Self
    where
        F: Send + Sync + 'static,
    {
        self.feed = Some(PrFeed::start(
            Arc::clone(&self.forge),
            self.board.lane(),
            crate::feed::DEFAULT_LIMIT,
            interval,
        ));
        self
    }

    #[must_use]
    pub fn feed(&self) -> Option<&PrFeed> {
        self.feed.as_ref()
    }

    #[must_use]
    pub fn board(&self) -> &Board {
        &self.board
    }

    #[must_use]
    pub fn panel(&self) -> &Panel {
        &self.panel
    }

    #[must_use]
    pub fn source_label(&self) -> &str {
        &self.source_label
    }

    #[must_use]
    pub fn keymap(&self) -> &KeyMap<Action> {
        &self.keys
    }

    #[must_use]
    pub fn should_quit(&self) -> bool {
        self.done
    }

    /// The review sink, exposed so a test can assert against a recording mock
    /// — "the app said it approved" and "the seam was called" are different
    /// claims, and only the second is evidence.
    #[must_use]
    pub fn review_env(&self) -> &R {
        &self.review
    }

    /// Pull whatever the feed absorbed into the docket. The `&mut self` apply
    /// half of the wakeup; performs no read.
    pub fn apply_feed(&mut self) {
        if let Some(rows) = self.feed.as_ref().map(PrFeed::rows) {
            self.board.set_rows(rows);
        }
    }

    /// Read the docket synchronously, on this thread. For a one-shot or a
    /// test; the running app uses the feed instead — see [`Self::with_feed`].
    ///
    /// # Errors
    /// Whatever the forge returns.
    pub fn refresh(&mut self) -> metsuke::Result<()> {
        let rows = self
            .forge
            .search(self.board.lane(), crate::feed::DEFAULT_LIMIT)?;
        self.board.set_rows(rows);
        Ok(())
    }

    /// Apply one action.
    pub fn dispatch(&mut self, action: Action) {
        match action {
            Action::Quit => self.done = true,
            Action::SelectNext => self.board.view_mut().select_next(),
            Action::SelectPrev => self.board.view_mut().select_prev(),
            Action::CycleLane => self.cycle_lane(),
            Action::OpenDiff => self.open_diff(),
            Action::CloseDiff => self.panel = Panel::Board,
            Action::ToggleDiffMode => self.toggle_diff_mode(),
            Action::Approve => self.stage(Verdict::Approve(String::new())),
            Action::RequestChanges => self.stage(Verdict::RequestChanges(String::new())),
            Action::Confirm => self.commit_pending(),
            Action::Cancel => self.panel = Panel::Board,
        }
    }

    fn cycle_lane(&mut self) {
        let lanes = Involvement::all();
        let here = lanes
            .iter()
            .position(|l| *l == self.board.lane())
            .unwrap_or(0);
        let next = lanes[(here + 1) % lanes.len()];
        self.board.set_lane(next);
        // The rows on screen belong to the OLD lane. Clearing them is more
        // honest than leaving them under a new lane's heading until the next
        // refresh lands — a stale row read as a current one is the whole
        // failure mode a live plane exists to prevent.
        self.board.set_rows(Vec::new());
    }

    fn open_diff(&mut self) {
        let Some(pr) = self.board.selected().cloned() else {
            return;
        };
        match self.forge.files(&pr.repo, pr.number) {
            Ok(files) => {
                // Re-join the per-file patches into one unified diff and let
                // egaku parse it. pauta does NOT parse diffs — `egaku::chigai`
                // and `DiffView` own that, and a second parser here would be
                // exactly the duplication metsuke was scoped to avoid.
                let mut unified = String::new();
                for f in &files {
                    unified.push_str("diff --git a/");
                    unified.push_str(f.previous_path.as_deref().unwrap_or(&f.path));
                    unified.push_str(" b/");
                    unified.push_str(&f.path);
                    unified.push('\n');
                    match &f.patch {
                        Some(p) => {
                            unified.push_str("--- a/");
                            unified.push_str(f.previous_path.as_deref().unwrap_or(&f.path));
                            unified.push('\n');
                            unified.push_str("+++ b/");
                            unified.push_str(&f.path);
                            unified.push('\n');
                            unified.push_str(p);
                            if !p.ends_with('\n') {
                                unified.push('\n');
                            }
                        }
                        // No patch means binary or too large. Say so, rather
                        // than rendering an empty file as if it were unchanged.
                        None => unified.push_str("Binary files differ\n"),
                    }
                }
                self.panel = Panel::Diff(Box::new(DiffView::parse(&unified)));
            }
            Err(e) => self.panel = Panel::Message(e.to_string()),
        }
    }

    fn toggle_diff_mode(&mut self) {
        if let Panel::Diff(v) = &mut self.panel {
            // egaku owns the flip; re-deriving it here would be a second
            // place for the mode set to drift.
            v.toggle_mode();
        }
    }

    /// Stage a binding verdict for confirmation. Never submits.
    fn stage(&mut self, verdict: Verdict) {
        let Some(pr) = self.board.selected().cloned() else {
            return;
        };
        debug_assert!(
            verdict.is_binding(),
            "only a binding verdict needs the two-keystroke gate"
        );
        self.panel = Panel::Pending { pr, verdict };
    }

    /// Commit the staged verdict. The ONLY path that reaches
    /// [`ForgeReview::submit_review`] — a verdict cannot be submitted without
    /// first having been staged and previewed.
    fn commit_pending(&mut self) {
        let Panel::Pending { pr, verdict } = &self.panel else {
            // Confirm with nothing pending is a no-op, not an error: it means
            // a stray keypress, and inventing a verdict for it would be the
            // worst possible reading of an ambiguous keystroke.
            return;
        };
        let (repo, number) = (pr.repo.clone(), pr.number);
        let v = verdict.clone();
        self.panel = match self.review.submit_review(&repo, number, &v) {
            Ok(()) => {
                let mut m = String::from("submitted ");
                m.push_str(match v {
                    Verdict::Approve(_) => "approval",
                    Verdict::RequestChanges(_) => "request-changes",
                    Verdict::Comment(_) => "comment",
                });
                m.push_str(" on ");
                m.push_str(&repo.to_string());
                m.push('#');
                m.push_str(&number.0.to_string());
                Panel::Message(m)
            }
            Err(e) => Panel::Message(e.to_string()),
        };
    }
}
