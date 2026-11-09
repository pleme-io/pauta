//! The live plane (QUADRO T14) — the docket refreshes itself.
//!
//! Adopted from the first commit rather than retrofitted, which is the point
//! of T14 being a doctrine rather than a nice-to-have. banken got this the
//! other way round: its `refresh()` sat with zero callers for weeks because
//! there was no non-input path into the runtime loop, and the table was
//! frozen against a live cluster.
//!
//! Two rules, both load-bearing:
//!
//! 1. **The read never runs on the app task.** `Forge::search` is a `gh`
//!    subprocess and an HTTPS round trip. On the task that owns the terminal
//!    that freezes input for its whole duration — and a slow or unreachable
//!    forge is exactly when the operator most wants the keyboard. So the read
//!    goes to tokio's blocking pool via `izumi::refresh`, which also preserves
//!    the last-known rows when a refresh panics instead of blanking the
//!    docket.
//! 2. **The signal never writes.** [`PrFeed::changed`] observes and may be
//!    dropped un-polled on any keystroke; the `&mut self` apply happens in the
//!    app's `on_wake`. Nothing is written inside the cancellable half.

use std::sync::Arc;
use std::sync::atomic::{AtomicBool, Ordering};
use std::time::Duration;

use izumi::refresh::{LiveStore, StopFlag, spawn_interval_refresh};
use metsuke::{Forge, Involvement, PrSummary};
use tokio::sync::watch;

/// Default cadence. A PR docket changes on human timescales — a minute is
/// responsive without making a `gh search` call every few seconds for a
/// screen nobody is looking at.
pub const DEFAULT_POLL: Duration = Duration::from_secs(60);

/// How many PRs to ask for. A ceiling, not a promise.
pub const DEFAULT_LIMIT: usize = 50;

/// A running background read of one involvement lane.
///
/// Dropping it stops the task: the [`StopFlag`] flips in [`Drop`], so a feed
/// cannot outlive the app and keep calling a forge for a closed terminal.
pub struct PrFeed {
    store: Arc<LiveStore<Vec<PrSummary>>>,
    rx: watch::Receiver<u64>,
    stop: StopFlag,
    faulted: Arc<AtomicBool>,
}

impl PrFeed {
    /// Start reading `lane` from `forge` every `interval`.
    ///
    /// The first read fires immediately, so the docket populates shortly
    /// after start rather than one interval later.
    #[must_use]
    pub fn start<F>(forge: Arc<F>, lane: Involvement, limit: usize, interval: Duration) -> Self
    where
        F: Forge + Send + Sync + 'static,
    {
        let store = LiveStore::new(Vec::<PrSummary>::new());
        let rx = store.subscribe();
        let stop: StopFlag = Arc::new(AtomicBool::new(false));
        let faulted = Arc::new(AtomicBool::new(false));

        let sink_store = Arc::clone(&store);
        let panic_flag = Arc::clone(&faulted);

        spawn_interval_refresh(
            interval,
            Arc::clone(&stop),
            // `Option<Vec<_>>`, never a bare `Vec`: a failed search has to stay
            // distinguishable from "nothing awaits your review". Collapsing
            // them renders a broken credential identically to an empty docket
            // — and only one of those means stop trusting the screen.
            Arc::new(move || forge.search(lane, limit).ok()),
            move |rows: Option<Vec<PrSummary>>| {
                if let Some(rows) = rows {
                    sink_store.push(rows);
                }
            },
            move || panic_flag.store(true, Ordering::SeqCst),
        );

        Self {
            store,
            rx,
            stop,
            faulted,
        }
    }

    /// Resolve when the feed has new rows — the cancellable `&self` half.
    pub async fn changed(&self) {
        let mut rx = self.rx.clone();
        let _ = rx.changed().await;
    }

    /// The most recently absorbed rows.
    #[must_use]
    pub fn rows(&self) -> Vec<PrSummary> {
        self.store.snapshot()
    }

    /// Whether a refresh panicked; [`Self::rows`] then holds the last good set.
    #[must_use]
    pub fn faulted(&self) -> bool {
        self.faulted.load(Ordering::SeqCst)
    }

    /// How many times the feed has published. `0` = no completed read yet,
    /// which is what lets a test wait on the feed instead of on a guessed
    /// sleep.
    #[must_use]
    pub fn generation(&self) -> u64 {
        self.store.generation()
    }
}

impl Drop for PrFeed {
    fn drop(&mut self) {
        self.stop.store(true, Ordering::SeqCst);
    }
}
