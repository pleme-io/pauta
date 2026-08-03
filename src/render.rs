//! Drawing the docket into an `egaku_term::Buffer`.
//!
//! Pure: takes `&PautaApp`, writes cells. No I/O, no terminal — which is what
//! lets every screen be asserted as a golden frame over `TestBackend`, by
//! position rather than by searching a blob.

use egaku_term::{Buffer, Style};
use metsuke::{Forge, ForgeReview, Verdict};

use crate::app::{Panel, PautaApp};
use crate::board::COLUMNS;

/// Column widths, left to right. Fixed rather than computed so a golden frame
/// is stable; the last column takes whatever remains.
const W_REPO: u16 = 24;
const W_PR: u16 = 7;
const W_AUTHOR: u16 = 14;
const W_STATE: u16 = 8;
const W_UPDATED: u16 = 21;

/// Below this, refuse to draw. Named so the gate and its test agree.
pub const MIN_W: u16 = 20;
pub const MIN_H: u16 = 4;

impl<F: Forge, R: ForgeReview> PautaApp<F, R> {
    /// Paint the current panel.
    pub fn render(&self, f: &mut Buffer) {
        let w = f.width();
        let h = f.height();
        if w < MIN_W || h < MIN_H {
            // MinSize gate: below this a table renders as unreadable
            // fragments. The message is SHORT on purpose — `set_string`
            // clips at the buffer edge, so "terminal too small" (18 cols)
            // renders as "terminal t" in a 10-column window: a
            // too-small warning too big to read. 9 columns fits anything
            // that can still show text at all.
            f.set_string(0, 0, "too small", Style::default());
            return;
        }
        self.draw_title(f, w);
        match self.panel() {
            Panel::Board => self.draw_board(f, w, h),
            Panel::Diff(v) => draw_diff(f, v, h),
            Panel::Pending { pr, verdict } => draw_pending(f, pr, verdict),
            Panel::Message(m) => {
                f.set_string(0, 2, m, Style::default());
            }
        }
        self.draw_status(f, w, h);
    }

    fn draw_title(&self, f: &mut Buffer, w: u16) {
        let mut t = String::from("pauta  ");
        t.push_str(self.board().lane().qualifier());
        f.set_stringn(0, 0, &t, w, Style::default());
    }

    fn draw_board(&self, f: &mut Buffer, w: u16, h: u16) {
        // Header
        let mut x = 0u16;
        for (name, width) in COLUMNS.iter().zip(col_widths(w)) {
            f.set_stringn(x, 1, name, width, Style::default());
            x += width;
        }
        // Rows
        let rows = self.board().view().rows();
        let sel = self.board().view().selected_index();
        let body_h = h.saturating_sub(3);
        for (i, row) in rows.iter().take(body_h as usize).enumerate() {
            let y = 2 + u16::try_from(i).unwrap_or(u16::MAX);
            let mut x = 0u16;
            for (name, width) in COLUMNS.iter().zip(col_widths(w)) {
                // The cursor is marked by a leading '>' in the first column
                // rather than by colour alone — NO_COLOR and a monochrome
                // terminal must still show which row is selected.
                let raw = egaku::TableRow::cell(row, name).unwrap_or("");
                let cell = if x == 0 {
                    let mut s = String::from(if i == sel { "> " } else { "  " });
                    s.push_str(raw);
                    s
                } else {
                    raw.to_owned()
                };
                f.set_stringn(x, y, &cell, width, Style::default());
                x += width;
            }
        }
        if rows.is_empty() {
            f.set_string(0, 2, "nothing awaits your review", Style::default());
        }
    }

    fn draw_status(&self, f: &mut Buffer, w: u16, h: u16) {
        let mut s = String::from(self.source_label());
        s.push_str("  ");
        s.push_str(&self.board().len().to_string());
        s.push_str(" prs");
        if self.feed().is_some_and(crate::PrFeed::faulted) {
            // A faulted feed must be visible: the rows on screen are the last
            // good ones, and an operator reading them as current is exactly
            // the failure a live plane is supposed to prevent.
            s.push_str("  [feed FAULTED — rows are stale]");
        }
        s.push_str("  ");
        s.push_str(&crate::keys::legend_parts().join(" "));
        f.set_stringn(0, h.saturating_sub(1), &s, w, Style::default());
    }
}

fn col_widths(total: u16) -> Vec<u16> {
    let fixed = W_REPO + W_PR + W_AUTHOR + W_STATE + W_UPDATED;
    let title = total.saturating_sub(fixed).max(8);
    vec![W_REPO, W_PR, title, W_AUTHOR, W_STATE, W_UPDATED]
}

fn draw_diff(f: &mut Buffer, v: &egaku::DiffView, h: u16) {
    for (i, row) in v
        .rows()
        .iter()
        .take((h.saturating_sub(3)) as usize)
        .enumerate()
    {
        let y = 2 + u16::try_from(i).unwrap_or(u16::MAX);
        let text = match row {
            egaku::Row::File(fd) => {
                let mut s = String::from("── ");
                s.push_str(fd.path());
                s
            }
            egaku::Row::HunkHeader { hunk, .. } => {
                let mut s = String::from("@@ ");
                s.push_str(&hunk.new_start().to_string());
                s
            }
            egaku::Row::Line { line, .. } => {
                // The +/- marker is re-emitted explicitly: `DiffLine::text`
                // strips it, and a red/green diff with no marker is invisible
                // under NO_COLOR.
                let mut s = String::from(match line.kind() {
                    egaku::LineKind::Added => "+",
                    egaku::LineKind::Removed => "-",
                    egaku::LineKind::Context => " ",
                    // `\ No newline at end of file` belongs to neither side.
                    egaku::LineKind::NoNewline => "\\",
                });
                s.push_str(line.text());
                s
            }
            egaku::Row::SideBySide { left, right, .. } => {
                let mut s = String::new();
                s.push_str(left.map_or("", egaku::DiffLine::text));
                s.push_str(" | ");
                s.push_str(right.map_or("", egaku::DiffLine::text));
                s
            }
        };
        f.set_stringn(0, y, &text, f.width(), Style::default());
    }
}

fn draw_pending(f: &mut Buffer, pr: &metsuke::PrSummary, verdict: &Verdict) {
    let mut head = String::from("CONFIRM ");
    head.push_str(match verdict {
        Verdict::Approve(_) => "APPROVE",
        Verdict::RequestChanges(_) => "REQUEST-CHANGES",
        Verdict::Comment(_) => "COMMENT",
    });
    head.push_str(" on ");
    head.push_str(&pr.repo.to_string());
    head.push('#');
    head.push_str(&pr.number.0.to_string());
    f.set_string(0, 2, &head, Style::default());
    f.set_string(0, 3, pr.title.as_str(), Style::default());
    f.set_string(0, 5, "y = submit    n = cancel", Style::default());
}
