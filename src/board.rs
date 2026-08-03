//! The docket: which pull requests are set down for your judgment, in order.
//!
//! This is the view *model* — no rendering, no I/O. It holds the rows, the
//! cursor, and the lane, and it answers "what is selected". Keeping it
//! separate from both the renderer and the forge is what lets the whole
//! screen be asserted in a unit test with nothing running.

use egaku::{Column, SortKey, SortOrder, TableRow, TableView};
use metsuke::{Involvement, PrSummary};

/// A PR as the table sees it.
///
/// `TableRow::cell` returns `&str`, so the rendered values are computed once
/// at construction and stored rather than formatted per draw. That is the
/// trait's shape, and it is the right one here: a docket redraws far more
/// often than it changes.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct PrRow {
    pr: PrSummary,
    id: String,
    repo: String,
    number: String,
    title: String,
}

/// The docket's sort column: most recently touched first.
///
/// **Not `PR`.** `TableView` sorts on the rendered cell STRING, and PR
/// numbers render as `#1`, `#2`, `#10` — lexicographically `#10` sorts before
/// `#2`, so a repo with ten open PRs would show them in an order that looks
/// arbitrary. ISO-8601 has the opposite property: it is *designed* to sort
/// correctly as text, so `UPDATED` is both the right default for a review
/// docket and safe under a string sort.
pub const SORT_COLUMN: &str = "UPDATED";

/// The columns, named once. The table, the renderer and the golden tests all
/// read this, so a renamed column cannot drift into disagreeing with the
/// header it is drawn under.
pub const COLUMNS: &[&str] = &["REPO", "PR", "TITLE", "AUTHOR", "STATE", "UPDATED"];

impl PrRow {
    #[must_use]
    pub fn new(pr: PrSummary) -> Self {
        // repo + number: a PR number alone is unique only per repo, and this
        // docket spans repos. Getting it wrong makes the cursor jump between
        // two same-numbered PRs on every refresh.
        let mut id = pr.repo.to_string();
        id.push('#');
        id.push_str(&pr.number.0.to_string());

        // Draft is a readiness fact, not a state, so it rides the TITLE —
        // STATE must keep saying open/merged/closed and nothing else.
        let title = if pr.draft {
            let mut t = String::from("[draft] ");
            t.push_str(&pr.title);
            t
        } else {
            pr.title.clone()
        };

        Self {
            id,
            repo: pr.repo.to_string(),
            number: pr.number.to_string(),
            title,
            pr,
        }
    }

    #[must_use]
    pub fn pr(&self) -> &PrSummary {
        &self.pr
    }
}

impl TableRow for PrRow {
    fn identity(&self) -> &str {
        &self.id
    }

    fn cell(&self, field: &str) -> Option<&str> {
        match field {
            "REPO" => Some(&self.repo),
            "PR" => Some(&self.number),
            "TITLE" => Some(&self.title),
            "AUTHOR" => Some(&self.pr.author),
            "STATE" => Some(self.pr.state.label()),
            "UPDATED" => Some(&self.pr.updated_at),
            // `None`, not `Some("")`: the table's own `unresolved_fields`
            // gate reports a column whose field no row carries, and an empty
            // string would hide that behind a blank cell.
            _ => None,
        }
    }
}

/// The docket itself.
pub struct Board {
    table: TableView<PrRow>,
    lane: Involvement,
}

impl Board {
    /// An empty docket on `lane`.
    ///
    /// # Panics
    /// If the columns do not resolve against [`PrRow`] — a construction bug
    /// caught by `every_column_resolves` below, not something a user reaches.
    #[must_use]
    pub fn new(lane: Involvement) -> Self {
        // Header and field key are the SAME string here on purpose: egaku's
        // own doc warns about the model where a column said `:field phase`
        // while every reader emitted `"STATUS"` — two vocabularies for one
        // thing. One name, used both ways, cannot drift.
        let cols: Vec<Column> = COLUMNS.iter().map(|n| Column::new(*n, *n)).collect();
        let sort = SortKey::new(SORT_COLUMN, SortOrder::Desc);
        Self {
            table: TableView::new(cols, Vec::new(), sort)
                .expect("pauta's own columns must resolve against PrRow"),
            lane,
        }
    }

    /// Replace the rows, preserving the selection by identity.
    ///
    /// Preserving by identity and not by index is the whole point: the feed
    /// re-reads on a cadence, and an index-preserving refresh moves the
    /// cursor onto a different PR under the operator's hands the moment
    /// anything is inserted above it.
    pub fn set_rows(&mut self, prs: Vec<PrSummary>) {
        self.table
            .set_rows(prs.into_iter().map(PrRow::new).collect());
    }

    #[must_use]
    pub fn lane(&self) -> Involvement {
        self.lane
    }

    pub fn set_lane(&mut self, lane: Involvement) {
        self.lane = lane;
    }

    #[must_use]
    pub fn view(&self) -> &TableView<PrRow> {
        &self.table
    }

    pub fn view_mut(&mut self) -> &mut TableView<PrRow> {
        &mut self.table
    }

    /// The PR under the cursor, if any.
    #[must_use]
    pub fn selected(&self) -> Option<&PrSummary> {
        self.table.selected_row().map(PrRow::pr)
    }

    #[must_use]
    pub fn len(&self) -> usize {
        self.table.len()
    }

    #[must_use]
    pub fn is_empty(&self) -> bool {
        self.len() == 0
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use metsuke::{PrNumber, PrState, RepoId};

    /// `n` also drives the timestamp, so a higher number is MORE recently
    /// updated — which under the UPDATED-desc sort puts it first.
    fn pr(n: u64, repo: &str, draft: bool) -> PrSummary {
        let mut ts = String::from("2026-08-03T00:00:");
        if n < 10 {
            ts.push('0');
        }
        ts.push_str(&n.to_string());
        ts.push('Z');
        PrSummary {
            repo: RepoId::parse(repo).unwrap(),
            number: PrNumber(n),
            title: "a change".into(),
            author: "drzzln".into(),
            state: PrState::Open,
            draft,
            updated_at: ts,
            url: String::new(),
        }
    }

    #[test]
    fn identity_spans_repos_so_two_same_numbered_prs_stay_distinct() {
        let a = PrRow::new(pr(7, "pleme-io/metsuke", false));
        let b = PrRow::new(pr(7, "pleme-io/banken", false));
        assert_ne!(
            a.identity(),
            b.identity(),
            "a docket spans repos; #7 in two repos must not collide"
        );
    }

    #[test]
    fn draft_rides_the_title_not_the_state_column() {
        let r = PrRow::new(pr(1, "pleme-io/metsuke", true));
        assert!(r.cell("TITLE").unwrap().starts_with("[draft]"));
        assert_eq!(
            r.cell("STATE").unwrap(),
            "open",
            "STATE must keep saying open/merged/closed — draft is readiness, not state"
        );
    }

    #[test]
    fn the_selection_survives_a_refresh_that_inserts_above_it() {
        let mut b = Board::new(Involvement::ReviewRequested);
        // UPDATED desc => #2 first, #1 second. Move to the second row.
        b.set_rows(vec![pr(1, "a/b", false), pr(2, "a/b", false)]);
        b.view_mut().select_next();
        let before = b.selected().unwrap().number;
        assert_eq!(before, PrNumber(1), "row 1 under UPDATED-desc is #1");

        // A newer PR arrives and sorts above both — the index of #1 shifts.
        b.set_rows(vec![
            pr(9, "a/b", false),
            pr(1, "a/b", false),
            pr(2, "a/b", false),
        ]);
        assert_eq!(
            b.selected().unwrap().number,
            PrNumber(1),
            "an index-preserving refresh would have moved the cursor onto a \
             different PR under the operator's hands"
        );
    }

    #[test]
    fn every_column_resolves_to_something() {
        let r = PrRow::new(pr(3, "pleme-io/metsuke", false));
        for c in COLUMNS {
            assert!(
                r.cell(c).is_some_and(|v| !v.is_empty()),
                "column {c} resolved to nothing — a header with no field behind it"
            );
        }
    }
}
