//! Golden frames and the verdict gate.
//!
//! Every assertion reads the buffer **by position** — `row(buf, y)` — never
//! `.contains()` on a flattened blob. A `.contains()` assertion passes when
//! the text lands in the wrong place, which is precisely the class of bug a
//! golden frame exists to catch.

use std::sync::Arc;

use egaku_term::Buffer;
use metsuke::{
    FilePatch, Forge, ForgeReview, Involvement, MockForge, PrNumber, PrState, PrSummary, RepoId,
    Verdict,
};
use pauta::app::Panel;
use pauta::{Action, PautaApp};

fn pr(n: u64, title: &str, draft: bool) -> PrSummary {
    let mut ts = String::from("2026-08-03T00:00:0");
    ts.push_str(&n.to_string());
    ts.push('Z');
    PrSummary {
        repo: RepoId::parse("pleme-io/metsuke").unwrap(),
        number: PrNumber(n),
        title: title.into(),
        author: "drzzln".into(),
        state: PrState::Open,
        draft,
        updated_at: ts,
        url: "https://example.invalid".into(),
    }
}

/// One buffer row as a trimmed string — content by position.
fn row(buf: &Buffer, y: u16) -> String {
    (0..buf.width())
        .filter_map(|x| buf.get(x, y).map(egaku_term::Cell::symbol))
        .collect::<String>()
        .trim_end()
        .to_owned()
}

fn draw<F: Forge, R: ForgeReview>(app: &PautaApp<F, R>, w: u16, h: u16) -> Buffer {
    let mut b = Buffer::empty(w, h);
    app.render(&mut b);
    b
}

fn app_with(prs: Vec<PrSummary>) -> PautaApp<MockForge, MockForge> {
    let mut forge = MockForge::new();
    for p in prs {
        forge = forge.with_pr(p);
    }
    let mut app = PautaApp::try_new(
        forge,
        MockForge::new(),
        Involvement::ReviewRequested,
        "source: mock",
    )
    .expect("the shipped catalog must compile");
    app.refresh().expect("the mock must answer");
    app
}

#[test]
fn the_docket_draws_its_header_and_rows_where_it_says_it_does() {
    let app = app_with(vec![pr(2, "second", false), pr(1, "first", false)]);
    let b = draw(&app, 100, 10);

    assert!(
        row(&b, 0).starts_with("pauta"),
        "row 0 is the title; got {:?}",
        row(&b, 0)
    );
    assert!(
        row(&b, 0).contains("--review-requested=@me"),
        "the title must name the lane being shown, or the screen is ambiguous"
    );
    assert!(
        row(&b, 1).starts_with("REPO"),
        "row 1 is the header; got {:?}",
        row(&b, 1)
    );

    // UPDATED desc puts #2 first, and the cursor starts on row 0 of the body.
    let first = row(&b, 2);
    assert!(first.starts_with("> "), "the cursor marks row 2: {first:?}");
    assert!(first.contains("#2"), "most-recent first: {first:?}");
    assert!(row(&b, 3).starts_with("  "), "row 3 is not the cursor row");
    assert!(row(&b, 3).contains("#1"));
}

#[test]
fn the_cursor_is_visible_without_colour() {
    // NO_COLOR / monochrome: selection must be legible from the glyphs alone.
    let app = app_with(vec![pr(1, "only", false)]);
    let b = draw(&app, 100, 10);
    assert!(
        row(&b, 2).starts_with(">"),
        "the selected row must carry a glyph marker, not colour alone"
    );
}

#[test]
fn an_empty_docket_says_so_rather_than_drawing_nothing() {
    let app = app_with(vec![]);
    let b = draw(&app, 100, 10);
    assert_eq!(row(&b, 2), "nothing awaits your review");
}

#[test]
fn a_tiny_terminal_refuses_instead_of_drawing_fragments() {
    let app = app_with(vec![pr(1, "x", false)]);
    // Deliberately narrower than the message would like: the refusal has to
    // survive being clipped, or it is not a refusal the operator can read.
    let b = draw(&app, 10, 3);
    assert_eq!(row(&b, 0), "too small");
}

#[test]
fn one_column_over_the_minimum_draws_the_real_screen() {
    // Pins the gate from the other side, so `MIN_W` cannot silently drift up
    // and start refusing terminals that used to work.
    let app = app_with(vec![pr(1, "x", false)]);
    let b = draw(&app, pauta::render::MIN_W, pauta::render::MIN_H);
    assert!(
        row(&b, 0).starts_with("pauta"),
        "at exactly MIN_W x MIN_H the app must draw, not refuse: {:?}",
        row(&b, 0)
    );
}

#[test]
fn a_draft_is_marked_in_the_title_and_the_state_still_reads_open() {
    let app = app_with(vec![pr(1, "wip thing", true)]);
    let b = draw(&app, 100, 10);
    let r = row(&b, 2);
    assert!(r.contains("[draft]"), "{r:?}");
    assert!(
        r.contains("open"),
        "STATE must still say open — draft is readiness, not state: {r:?}"
    );
}

// ── The verdict gate — the part that must not be got wrong ──────────────

#[test]
fn approve_stages_a_confirmation_and_submits_nothing() {
    let mut app = app_with(vec![pr(1, "a change", false)]);
    app.dispatch(Action::Approve);

    assert!(
        matches!(app.panel(), Panel::Pending { .. }),
        "approve must STAGE, never submit"
    );
    assert_eq!(
        app.review_env().submitted_count(),
        0,
        "nothing may reach the forge before the second keystroke"
    );

    let b = draw(&app, 100, 10);
    assert!(
        row(&b, 2).starts_with("CONFIRM APPROVE"),
        "{:?}",
        row(&b, 2)
    );
    assert!(
        row(&b, 5).contains("y = submit"),
        "the gate must say how to commit"
    );
}

#[test]
fn cancelling_a_staged_verdict_submits_nothing_and_returns_to_the_docket() {
    let mut app = app_with(vec![pr(1, "a change", false)]);
    app.dispatch(Action::Approve);
    app.dispatch(Action::Cancel);
    assert!(matches!(app.panel(), Panel::Board));
    assert_eq!(app.review_env().submitted_count(), 0);
}

#[test]
fn only_a_confirm_after_a_stage_reaches_the_forge() {
    let mut app = app_with(vec![pr(7, "a change", false)]);
    app.dispatch(Action::Approve);
    // The intermediate assertion is what makes the test's NAME true. Without
    // it this passed even with `approve` authored `:gate free` — stage would
    // submit immediately, the final count would still be 1, and a test called
    // "only a confirm reaches the forge" would be green while a single
    // keystroke reached the forge. Found by flipping the catalog and watching
    // which tests actually went red.
    assert_eq!(
        app.review_env().submitted_count(),
        0,
        "the first keystroke must not reach the forge"
    );
    app.dispatch(Action::Confirm);

    let submitted = app.review_env().submitted.borrow();
    assert_eq!(submitted.len(), 1, "exactly one review submitted");
    assert_eq!(submitted[0].1, PrNumber(7), "…against the selected PR");
    assert!(
        matches!(submitted[0].2, Verdict::Approve(_)),
        "…carrying the verdict that was staged"
    );
}

#[test]
fn confirm_with_nothing_staged_is_a_no_op_not_an_invented_verdict() {
    let mut app = app_with(vec![pr(1, "a change", false)]);
    app.dispatch(Action::Confirm);
    assert_eq!(
        app.review_env().submitted_count(),
        0,
        "a stray confirm must never synthesise a verdict"
    );
}

#[test]
fn request_changes_takes_the_same_two_keystroke_path() {
    let mut app = app_with(vec![pr(3, "a change", false)]);
    app.dispatch(Action::RequestChanges);
    assert_eq!(app.review_env().submitted_count(), 0);
    app.dispatch(Action::Confirm);
    let submitted = app.review_env().submitted.borrow();
    assert!(matches!(submitted[0].2, Verdict::RequestChanges(_)));
}

#[test]
fn approve_with_an_empty_docket_stages_nothing() {
    let mut app = app_with(vec![]);
    app.dispatch(Action::Approve);
    assert!(
        matches!(app.panel(), Panel::Board),
        "with no PR selected there is nothing to approve"
    );
    assert_eq!(app.review_env().submitted_count(), 0);
}

// ── The diff panel ──────────────────────────────────────────────────────

#[test]
fn opening_a_diff_renders_the_patch_lines() {
    let base = pr(1, "a change", false);
    let forge = MockForge::new().with_pr(base.clone()).with_file(
        PrNumber(1),
        FilePatch {
            path: "src/lib.rs".into(),
            previous_path: None,
            additions: 1,
            deletions: 1,
            patch: Some("@@ -1,2 +1,2 @@\n-old line\n+new line\n context\n".into()),
        },
    );
    let mut app = PautaApp::try_new(
        forge,
        MockForge::new(),
        Involvement::ReviewRequested,
        "source: mock",
    )
    .expect("the shipped catalog must compile");
    app.refresh().unwrap();
    app.dispatch(Action::OpenDiff);
    assert!(matches!(app.panel(), Panel::Diff(_)));

    let b = draw(&app, 100, 12);
    let body: Vec<String> = (2..8).map(|y| row(&b, y)).collect();
    assert!(
        body.iter().any(|l| l.contains("src/lib.rs")),
        "the file header must be drawn: {body:?}"
    );
    assert!(
        body.iter().any(|l| l.starts_with("-old line")),
        "a removal must carry its marker (invisible under NO_COLOR otherwise): {body:?}"
    );
    assert!(
        body.iter().any(|l| l.starts_with("+new line")),
        "an addition must carry its marker: {body:?}"
    );
}

#[test]
fn a_binary_file_says_so_instead_of_rendering_as_unchanged() {
    let forge = MockForge::new()
        .with_pr(pr(1, "add a logo", false))
        .with_file(
            PrNumber(1),
            FilePatch {
                path: "logo.png".into(),
                previous_path: None,
                additions: 0,
                deletions: 0,
                patch: None,
            },
        );
    let mut app = PautaApp::try_new(
        forge,
        MockForge::new(),
        Involvement::ReviewRequested,
        "source: mock",
    )
    .expect("the shipped catalog must compile");
    app.refresh().unwrap();
    app.dispatch(Action::OpenDiff);
    let b = draw(&app, 100, 12);
    let body: Vec<String> = (2..8).map(|y| row(&b, y)).collect();
    assert!(
        body.iter().any(|l| l.contains("logo.png")),
        "a binary file must still appear in the file list: {body:?}"
    );
}

#[test]
fn a_forge_error_becomes_a_readable_message_not_a_panic() {
    let forge = MockForge::new().with_pr(pr(1, "a change", false)).failing(
        metsuke::ForgeError::NotAuthenticated {
            detail: "gh auth login".into(),
        },
    );
    let mut app = PautaApp::try_new(
        forge,
        MockForge::new(),
        Involvement::ReviewRequested,
        "source: mock",
    )
    .expect("the shipped catalog must compile");
    // The refresh itself fails; the app must survive it.
    assert!(app.refresh().is_err());
    app.dispatch(Action::OpenDiff);
    assert!(matches!(app.panel(), Panel::Board | Panel::Message(_)));
}

// ── Lanes ───────────────────────────────────────────────────────────────

#[test]
fn cycling_the_lane_clears_the_old_lanes_rows() {
    let mut app = app_with(vec![pr(1, "a change", false)]);
    assert_eq!(app.board().len(), 1);
    app.dispatch(Action::CycleLane);
    assert_ne!(app.board().lane(), Involvement::ReviewRequested);
    assert_eq!(
        app.board().len(),
        0,
        "rows from the previous lane must not sit under a new lane's heading"
    );
}

#[test]
fn cycling_through_every_lane_returns_to_the_start() {
    let mut app = app_with(vec![]);
    let start = app.board().lane();
    for _ in 0..Involvement::all().len() {
        app.dispatch(Action::CycleLane);
    }
    assert_eq!(app.board().lane(), start, "the cycle must be closed");
}

// ── The live plane (QUADRO T14) ─────────────────────────────────────────

/// A forge whose result set grows on each call, so "did the feed run again"
/// is answerable from the docket alone. `MockForge` is `RefCell`-based and
/// therefore not `Sync`; a feed reads from a spawned task, so it needs an
/// atomic double.
struct GrowingForge {
    calls: std::sync::atomic::AtomicUsize,
}

impl Forge for GrowingForge {
    fn search(&self, _lane: Involvement, _limit: usize) -> metsuke::Result<Vec<PrSummary>> {
        let n = self.calls.fetch_add(1, std::sync::atomic::Ordering::SeqCst) + 1;
        Ok((1..=n as u64).map(|i| pr(i, "a change", false)).collect())
    }
    fn pull_request(&self, _r: &RepoId, _n: PrNumber) -> metsuke::Result<PrSummary> {
        Err(unused())
    }
    fn files(&self, _r: &RepoId, _n: PrNumber) -> metsuke::Result<Vec<FilePatch>> {
        Err(unused())
    }
    fn comments(&self, _r: &RepoId, _n: PrNumber) -> metsuke::Result<Vec<metsuke::ReviewComment>> {
        Err(unused())
    }
}

fn unused() -> metsuke::ForgeError {
    metsuke::ForgeError::Unsupported {
        operation: "not exercised by the feed test",
        backend: "growing",
    }
}

/// The T14 predicate: the docket changes with ZERO key events.
#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn the_docket_grows_without_a_keystroke() {
    let forge: Arc<GrowingForge> = Arc::new(GrowingForge {
        calls: std::sync::atomic::AtomicUsize::new(0),
    });
    let mut app: PautaApp<GrowingForge, MockForge> = PautaApp::try_new(
        forge,
        MockForge::new(),
        Involvement::ReviewRequested,
        "source: growing",
    )
    .expect("the shipped catalog must compile")
    .with_feed(std::time::Duration::from_millis(20));

    let before = app.board().len();
    assert_eq!(before, 0, "no inline refresh was called");

    let feed = app.feed().expect("with_feed attaches a feed");
    for _ in 0..200 {
        if feed.generation() >= 3 {
            break;
        }
        tokio::time::sleep(std::time::Duration::from_millis(10)).await;
    }
    assert!(
        feed.generation() >= 3,
        "the feed must publish on its own cadence; reached {}",
        feed.generation()
    );

    app.apply_feed();
    assert!(
        app.board().len() > before,
        "the docket must absorb rows with no key pressed: {before} -> {}",
        app.board().len()
    );
}

#[tokio::test]
async fn an_app_without_a_feed_has_none() {
    let app: PautaApp<MockForge, MockForge> = app_with(vec![]);
    assert!(
        app.feed().is_none(),
        "no feed unless asked for one — input-only is the default"
    );
}
