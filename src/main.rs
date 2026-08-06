//! `pauta` — the binary.
//!
//! pauta shipped as a library with a README documenting
//! `pauta --review-requested=@me` and no `[[bin]]` to run it. Everything the
//! docket needs already existed — the catalog-derived keymap, the board, the
//! diff panel, the two-keystroke verdict gate, the T14 feed — with nothing to
//! drive them. This is that driver, and deliberately nothing more: it owns no
//! review logic, no GitHub knowledge and no keybindings of its own.
//!
//! ## Why the runtime lives here and not in the library
//!
//! [`PautaApp`] is generic over `F: Forge` and `R: ForgeReview` precisely so a
//! caller chooses where PRs come from and where a verdict goes. Binding it to
//! one runtime in the library would forfeit that. The binary picks a live
//! `GhForge`; a future offline run against acervo's corpus is the same
//! `PautaApp` with a different `F`, and needs no change here beyond the
//! constructor.
//!
//! ## The wake pacing
//!
//! [`PrFeed`] exposes `rows`/`generation` but no watch channel to await, so
//! [`AsyncApp::wake`] is a paced `sleep` and [`AsyncApp::on_wake`] applies
//! whatever the feed has. `sleep` is cancellation-safe, which the trait
//! explicitly requires of a signal future — it is dropped un-polled on every
//! keystroke that wins the select, and dropping a sleep loses nothing. The
//! mutation lives in `on_wake`, which the runtime never drops part-way.

use std::time::Duration;

use egaku::KeyMap;
use egaku_term::{AsyncApp, Buffer, Result as TermResult, run_async};
use metsuke::{Forge, ForgeReview, GhForge, Involvement, RealGh};
use pauta::{Action, PautaApp};

/// How often the applied view catches up with the feed.
const WAKE_INTERVAL: Duration = Duration::from_millis(250);

/// How often the feed itself re-reads the forge.
const FEED_INTERVAL: Duration = Duration::from_secs(60);

/// The `AsyncApp` adapter. Thin by design: every method forwards to the
/// library, so the runtime cannot drift from the tested behaviour.
struct Runner<F: Forge, R: ForgeReview> {
    app: PautaApp<F, R>,
}

// `Sync`, not just `Send`, on BOTH seams: `draw` and `wake` take `&self`, and
// a `&T` is only `Send` when `T: Sync`. Without it the runtime cannot hold the
// app across an await point.
impl<F, R> AsyncApp for Runner<F, R>
where
    F: Forge + Send + Sync + 'static,
    R: ForgeReview + Send + Sync,
{
    type Action = Action;

    fn keymap(&self) -> &KeyMap<Self::Action> {
        self.app.keymap()
    }

    async fn handle(&mut self, action: &Self::Action) -> TermResult<()> {
        self.app.dispatch(*action);
        Ok(())
    }

    async fn draw(&self, frame: &mut Buffer) -> TermResult<()> {
        self.app.render(frame);
        Ok(())
    }

    fn should_quit(&self) -> bool {
        self.app.should_quit()
    }

    /// Returns the sleep directly rather than `async fn { …await }`, so the
    /// future captures nothing — the runtime drops it un-polled on every
    /// keystroke that wins the select, and a future holding no borrow cannot
    /// leave anything half-written when it is dropped.
    fn wake(&self) -> impl std::future::Future<Output = ()> + Send {
        tokio::time::sleep(WAKE_INTERVAL)
    }

    async fn on_wake(&mut self) -> TermResult<()> {
        self.app.apply_feed();
        Ok(())
    }
}

/// Resolve the lane from argv.
///
/// Accepts the `gh`-shaped spellings the README documents
/// (`--review-requested=@me`) as well as bare `--authored`, so the flag a
/// reader copies out of the docs is the flag that works.
fn lane_from_args(args: &[String]) -> Option<Involvement> {
    for a in args {
        let flag = a.split('=').next().unwrap_or(a);
        match flag {
            "--review-requested" => return Some(Involvement::ReviewRequested),
            "--author" | "--authored" => return Some(Involvement::Authored),
            "--assignee" | "--assigned" => return Some(Involvement::Assigned),
            "--involves" | "--involved" => return Some(Involvement::Involved),
            _ => {}
        }
    }
    None
}

fn print_usage() {
    println!("pauta — the PR-review quadro");
    println!();
    println!("USAGE:");
    println!("    pauta [LANE]");
    println!();
    println!("LANE (default: --review-requested):");
    for lane in Involvement::all() {
        println!("    {:<22} {}", lane.qualifier(), lane_help(*lane));
    }
    println!();
    println!("Keys are authored in pauta-spec's (defpautaaction) catalog and");
    println!("shown in the status legend at runtime — this help does not");
    println!("restate them, so the two cannot disagree.");
}

const fn lane_help(lane: Involvement) -> &'static str {
    match lane {
        Involvement::ReviewRequested => "someone is waiting on you",
        Involvement::Authored => "pull requests you opened",
        Involvement::Assigned => "pull requests assigned to you",
        Involvement::Involved => "the union — mentions and comments included",
    }
}

#[tokio::main]
async fn main() -> Result<(), Box<dyn std::error::Error>> {
    let args: Vec<String> = std::env::args().skip(1).collect();
    if args.iter().any(|a| a == "--help" || a == "-h") {
        print_usage();
        return Ok(());
    }

    let lane = lane_from_args(&args).unwrap_or(Involvement::ReviewRequested);

    // Two GhForge values, not one shared: `Forge` (read) and `ForgeReview`
    // (write) are separate seams in metsuke on purpose, and `RealGh` is a unit
    // struct, so keeping them separate costs nothing and preserves the split.
    let forge = GhForge::new(RealGh);
    let review = GhForge::new(RealGh);

    // The label is rendered in the UI. It says `gh` because that is what is
    // actually being read — a fixture- or corpus-backed docket must never be
    // able to read as a live one.
    let app = PautaApp::try_new(forge, review, lane, "gh")?.with_feed(FEED_INTERVAL);

    run_async(&mut Runner { app }).await?;
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_default_lane_is_the_one_someone_is_waiting_on() {
        assert_eq!(
            lane_from_args(&[]).unwrap_or(Involvement::ReviewRequested),
            Involvement::ReviewRequested
        );
    }

    /// The README documents `--review-requested=@me`; the `=value` form must
    /// resolve to the same lane as the bare flag, or the documented invocation
    /// silently falls back to the default.
    #[test]
    fn the_documented_gh_shaped_spelling_resolves() {
        assert_eq!(
            lane_from_args(&["--review-requested=@me".to_owned()]),
            Some(Involvement::ReviewRequested)
        );
        assert_eq!(
            lane_from_args(&["--author=@me".to_owned()]),
            Some(Involvement::Authored)
        );
    }

    #[test]
    fn bare_spellings_resolve_too() {
        assert_eq!(
            lane_from_args(&["--authored".to_owned()]),
            Some(Involvement::Authored)
        );
        assert_eq!(
            lane_from_args(&["--assigned".to_owned()]),
            Some(Involvement::Assigned)
        );
        assert_eq!(
            lane_from_args(&["--involved".to_owned()]),
            Some(Involvement::Involved)
        );
    }

    #[test]
    fn an_unknown_flag_does_not_silently_pick_a_lane() {
        assert_eq!(lane_from_args(&["--nonsense".to_owned()]), None);
    }

    /// Every lane the client can query must be helpable, so `--help` cannot
    /// advertise a shorter list than `Involvement::all()`.
    #[test]
    fn every_lane_has_help_text() {
        for lane in Involvement::all() {
            assert!(!lane_help(*lane).is_empty());
            assert!(!lane.qualifier().is_empty());
        }
        assert_eq!(Involvement::all().len(), 4);
    }
}
