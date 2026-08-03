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
use pauta_spec::{Catalog, Effect};

use crate::board::Board;
use crate::feed::PrFeed;

/// The runtime's action type IS the catalog's [`Effect`].
///
/// There is no second enum here. pauta previously declared its own `Action`
/// with a hand-written `ALL` and a test holding the keymap to it; the catalog
/// makes that list authored instead, so the alias is all that remains.
pub type Action = Effect;

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
    catalog: Catalog,
    keys: KeyMap<Effect>,
    done: bool,
    feed: Option<PrFeed>,
    /// A short label naming where the rows came from — tier honesty in the UI
    /// itself, so a fixture-backed docket never reads as a live one.
    source_label: String,
}

impl<F: Forge, R: ForgeReview> PautaApp<F, R> {
    /// Build the app over a forge and a review sink.
    /// # Errors
    /// [`pauta_spec::SpecError`] when the authored catalog fails to compile
    /// or is self-inconsistent. Fallible on purpose: the keymap and the
    /// legend come from that catalog, and falling back to hardcoded chords
    /// would put the operator on a keyboard the legend does not describe.
    pub fn try_new(
        forge: impl Into<Arc<F>>,
        review: R,
        lane: Involvement,
        source_label: impl Into<String>,
    ) -> Result<Self, pauta_spec::SpecError> {
        let catalog = Catalog::load()?;
        Ok(Self {
            forge: forge.into(),
            review,
            board: Board::new(lane),
            panel: Panel::Board,
            keys: crate::keys::keymap(&catalog),
            catalog,
            done: false,
            feed: None,
            source_label: source_label.into(),
        })
    }

    /// The authored catalog — the renderer reads the legend from it.
    #[must_use]
    pub fn catalog(&self) -> &Catalog {
        &self.catalog
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
    pub fn keymap(&self) -> &KeyMap<Effect> {
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
            Action::Approve => self.stage(Effect::Approve, Verdict::Approve(String::new())),
            Action::RequestChanges => {
                self.stage(
                    Effect::RequestChanges,
                    Verdict::RequestChanges(String::new()),
                );
            }
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

    /// Stage a verdict for confirmation. Never submits.
    ///
    /// The gate is read from the AUTHORED catalog rather than decided here,
    /// so the runtime cannot disagree with what `(defpautaaction)` declares.
    /// If an effect were ever authored `:gate free`, it would submit straight
    /// through — which is why `pauta-spec` has no way to author a verdict
    /// that way.
    fn stage(&mut self, effect: Effect, verdict: Verdict) {
        let Some(pr) = self.board.selected().cloned() else {
            return;
        };
        if self.catalog.is_binding(effect) {
            self.panel = Panel::Pending { pr, verdict };
        } else {
            self.submit(&pr.repo.clone(), pr.number, &verdict);
        }
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
        self.submit(&repo, number, &v);
    }

    /// Submit and report. The ONLY caller of [`ForgeReview::submit_review`].
    fn submit(&mut self, repo: &metsuke::RepoId, number: metsuke::PrNumber, v: &Verdict) {
        self.panel = match self.review.submit_review(repo, number, v) {
            Ok(()) => {
                let mut m = String::from("submitted ");
                m.push_str(match *v {
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
