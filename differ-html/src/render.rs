//! Static HTML renderer: a graphical probe for the rails/connectors idea.
//!
//! Rust lays the diff rows out as HTML, so text selection and copy keep working
//! and no font metrics are guessed. A small script then reads the laid-out row
//! positions and draws the rails and connectors into an SVG overlay — the same
//! split a real GUI would use, but with none of the character-cell constraints
//! that force the TUI's box-drawing lanes.

use std::fmt::Write as _;

use differ_core::model::{self, DiffRow, Kind};

/// One file's before/after pair, ready to render.
pub struct Diff {
    pub path: String,
    pub old_label: String,
    pub new_label: String,
    pub rows: Vec<DiffRow>,
    /// Set when the file was skipped (too large, unreadable, …).
    pub note: Option<String>,
}

/// What the two sides mean: the two revisions for a range, or `git status`'s
/// HEAD / index / worktree for a worktree entry.
pub fn side_labels(
    comparison: &differ_core::git::Comparison,
    entry: &differ_core::git::FileEntry,
) -> (String, String) {
    if let Some(labels) = comparison.side_labels() {
        return labels;
    }
    let differ_core::git::Status::Worktree { staged, unstaged } = entry.status else {
        return (String::new(), String::new());
    };
    if let Some(y) = unstaged {
        match y {
            '?' => ("(new file)".into(), "working tree".into()),
            'D' => ("index".into(), "(deleted)".into()),
            _ => ("index".into(), "working tree".into()),
        }
    } else if let Some(x) = staged {
        match x {
            'A' => ("(new file)".into(), "index".into()),
            'D' => ("HEAD".into(), "(deleted)".into()),
            _ => ("HEAD".into(), "index".into()),
        }
    } else {
        (String::new(), String::new())
    }
}

/// Render the whole page. `source` is the repo root or a description of the two
/// files compared directly.
pub fn page(diffs: &[Diff], source: &str, wrap: bool) -> String {
    let (files, removed, added, changed) = totals(diffs);
    let mut out = String::with_capacity(64 * 1024);

    out.push_str("<!doctype html>\n<html lang=\"en\">\n<head>\n<meta charset=\"utf-8\">\n");
    out.push_str("<meta name=\"viewport\" content=\"width=device-width, initial-scale=1\">\n");
    out.push_str("<title>differ — rails</title>\n<style>\n");
    out.push_str(STYLE);
    out.push_str("</style>\n</head>\n<body>\n");

    out.push_str("<header class=\"chrome\">\n");
    out.push_str("<div class=\"brand\">differ<span class=\"dot\">·</span>rails</div>\n");
    let _ = writeln!(
        out,
        "<div class=\"summary\"><span class=\"src\">{}</span>\
         <span class=\"count\">{files} file{}</span>\
         <span class=\"stat\"><b class=\"add\">+{added}</b>\
         <b class=\"del\">−{removed}</b><b class=\"chg\">~{changed}</b></span></div>",
        escape(source),
        if files == 1 { "" } else { "s" },
    );
    out.push_str(
        "<div class=\"legend\">\
         <span class=\"key k-removed\">removed</span>\
         <span class=\"key k-added\">added</span>\
         <span class=\"key k-changed\">changed</span>\
         <span class=\"hint\">hover a changed row to highlight its rail</span>\
         </div>\n</header>\n",
    );

    if files > 1 {
        out.push_str("<nav class=\"files\">\n");
        let mut n = 0;
        for (i, diff) in diffs.iter().enumerate() {
            if diff.note.is_some() {
                continue;
            }
            n += 1;
            let counts = stats(&diff.rows);
            let _ = writeln!(
                out,
                "<a href=\"#f{i}\"><span class=\"n\">{n}</span>\
                 <span class=\"p\">{}</span>\
                 <span class=\"c\"><b class=\"add\">+{}</b><b class=\"del\">−{}</b></span></a>",
                escape(&diff.path),
                counts.1,
                counts.0,
            );
        }
        out.push_str("</nav>\n");
    }

    out.push_str(if wrap {
        "<main>\n"
    } else {
        "<main class=\"nowrap\">\n"
    });
    if diffs.is_empty() {
        out.push_str(
            "<div class=\"empty\"><h2>Working tree clean</h2>\
             <p>Nothing to diff for this selection.</p></div>\n",
        );
    }
    out.push_str("<script>window.DOCS=[");
    for (i, diff) in diffs.iter().enumerate() {
        if i > 0 {
            out.push(',');
        }
        out.push_str(&blocks_json(&diff.rows));
    }
    out.push_str("];</script>\n");

    for (i, diff) in diffs.iter().enumerate() {
        render_diff(&mut out, i, diff);
    }
    out.push_str(
        "</main>\n<footer>differ · static prototype · no character grid, no lanes</footer>\n",
    );
    out.push_str("<script>\n");
    out.push_str(SCRIPT);
    out.push_str("</script>\n</body>\n</html>\n");
    out
}

fn totals(diffs: &[Diff]) -> (usize, usize, usize, usize) {
    let mut files = 0;
    let (mut removed, mut added, mut changed) = (0, 0, 0);
    for diff in diffs {
        if diff.note.is_some() {
            continue;
        }
        files += 1;
        let (r, a, c) = stats(&diff.rows);
        removed += r;
        added += a;
        changed += c;
    }
    (files, removed, added, changed)
}

/// `(removed, added, changed)` line counts for one file.
fn stats(rows: &[DiffRow]) -> (usize, usize, usize) {
    let mut removed = 0;
    let mut added = 0;
    let mut changed = 0;
    for row in rows {
        match row {
            DiffRow::Del(_) => removed += 1,
            DiffRow::Ins(_) => added += 1,
            DiffRow::Chg(_, _) => {
                removed += 1;
                added += 1;
                changed += 1;
            }
            DiffRow::Ctx(_) => {}
        }
    }
    (removed, added, changed)
}

fn render_diff(out: &mut String, index: usize, diff: &Diff) {
    let _ = writeln!(out, "<section class=\"file\" id=\"f{index}\">");
    let (removed, added, changed) = stats(&diff.rows);
    let _ = writeln!(
        out,
        "<header class=\"fhead\"><h2>{}</h2><span class=\"stat\">\
         <b class=\"add\">+{added}</b><b class=\"del\">−{removed}</b><b class=\"chg\">~{changed}</b>\
         </span></header>",
        escape(&diff.path),
    );
    if let Some(note) = &diff.note {
        let _ = write!(out, "<p class=\"note\">{}</p>\n</section>\n", escape(note));
        return;
    }

    out.push_str("<div class=\"diff\">\n<div class=\"pheads\"><span class=\"ph old\">");
    let _ = write!(out, "{}", escape(&diff.old_label));
    out.push_str("</span><span class=\"ph mid\"></span><span class=\"ph new\">");
    let _ = write!(out, "{}", escape(&diff.new_label));
    out.push_str("</span></div>\n");

    let _ = writeln!(out, "<div class=\"body\" data-diff=\"{index}\">");
    out.push_str("<svg class=\"rails\" xmlns=\"http://www.w3.org/2000/svg\"></svg>\n");

    let sides: Vec<(Side, Side)> = diff.rows.iter().map(sides).collect();
    let block_of = block_rows(&diff.rows);

    // Old pane.
    out.push_str("<div class=\"pane old\"><div class=\"rows\">");
    let mut no = 0;
    for (i, (old, _)) in sides.iter().enumerate() {
        let line_no = if matches!(old, Side::Blank) {
            None
        } else {
            no += 1;
            Some(no)
        };
        row_html(
            out,
            index,
            i,
            old,
            line_no,
            block_of[i],
            kind_of(&diff.rows[i]),
        );
    }
    out.push_str("</div></div>\n");

    out.push_str("<div class=\"gutter\"></div>\n");

    // New pane.
    out.push_str("<div class=\"pane new\"><div class=\"rows\">");
    let mut no = 0;
    for (i, (_, new)) in sides.iter().enumerate() {
        let line_no = if matches!(new, Side::Blank) {
            None
        } else {
            no += 1;
            Some(no)
        };
        row_html(
            out,
            index,
            i,
            new,
            line_no,
            block_of[i],
            kind_of(&diff.rows[i]),
        );
    }
    out.push_str("</div></div>\n");

    out.push_str("</div>\n</div>\n</section>\n");
}

/// One side of one row.
enum Side {
    /// Nothing on this side (the other side's insertion/deletion).
    Blank,
    /// Unchanged or whole-line text.
    Plain(String),
    /// Word-level segments of a changed line.
    Segs(Vec<(String, Kind)>),
}

fn sides(row: &DiffRow) -> (Side, Side) {
    match row {
        DiffRow::Ctx(t) => (Side::Plain(t.clone()), Side::Plain(t.clone())),
        DiffRow::Chg(o, n) => {
            let (o, n) = model::intra_line(o, n);
            (Side::Segs(o), Side::Segs(n))
        }
        DiffRow::Del(t) => (Side::Plain(t.clone()), Side::Blank),
        DiffRow::Ins(t) => (Side::Blank, Side::Plain(t.clone())),
    }
}

fn row_html(
    out: &mut String,
    diff: usize,
    row: usize,
    side: &Side,
    line_no: Option<usize>,
    block: Option<usize>,
    kind: &'static str,
) {
    // The counterpart of a removed/added line is empty on this side: hatch it.
    let class = if matches!(side, Side::Blank) {
        "none"
    } else {
        kind
    };
    let _ = write!(out, "<div class=\"row r-{class}\" data-row=\"{row}\"");
    if let Some(block) = block {
        let _ = write!(out, " data-block=\"blk-{diff}-{block}\"");
    }
    out.push('>');
    match line_no {
        Some(n) => {
            let _ = write!(out, "<span class=\"ln\">{n}</span>");
        }
        None => out.push_str("<span class=\"ln\"></span>"),
    }
    out.push_str("<span class=\"code\">");
    match side {
        Side::Blank => out.push_str("&nbsp;"),
        Side::Plain(t) => out.push_str(&escape(&expand_tabs(t))),
        Side::Segs(segs) => {
            for (text, kind) in segs {
                let text = expand_tabs(text);
                match kind {
                    Kind::Equal => out.push_str(&escape(&text)),
                    _ => {
                        let _ = write!(
                            out,
                            "<span class=\"s-{}\">{}</span>",
                            kind_name(*kind),
                            escape(&text)
                        );
                    }
                }
            }
        }
    }
    out.push_str("</span></div>\n");
}

/// Row tint: the model row's kind, or `plain` for a context line.
fn kind_of(row: &DiffRow) -> &'static str {
    match row {
        DiffRow::Ctx(_) => "plain",
        DiffRow::Del(_) => "removed",
        DiffRow::Ins(_) => "added",
        DiffRow::Chg(_, _) => "changed",
    }
}

fn kind_name(kind: Kind) -> &'static str {
    match kind {
        Kind::Equal => "equal",
        Kind::Added => "added",
        Kind::Removed => "removed",
        Kind::Changed => "changed",
    }
}

/// Rows of each change block, so rows can point at the block they belong to.
fn block_rows(rows: &[DiffRow]) -> Vec<Option<usize>> {
    let mut out = vec![None; rows.len()];
    for (b, block) in blocks(rows).iter().enumerate() {
        if let Some((from, to)) = block.old {
            for slot in &mut out[from..to] {
                *slot = Some(b);
            }
        }
        if let Some((from, to)) = block.new {
            for slot in &mut out[from..to] {
                *slot = Some(b);
            }
        }
    }
    out
}

struct Block {
    kind: &'static str,
    /// Row range on the old side (None if the block is inserted only).
    old: Option<(usize, usize)>,
    /// Row range on the new side (None if the block is removed only).
    new: Option<(usize, usize)>,
}

/// Contiguous runs of rows that share a kind.
///
/// Grouping per row kind (rather than per hunk) keeps each rail honest: a run
/// of deleted lines never borrows the rows of a changed line that follows it,
/// and an unpaired delete/insert pair reads as two separate one-sided blocks
/// instead of one misleading link.
fn blocks(rows: &[DiffRow]) -> Vec<Block> {
    let mut out = Vec::new();
    let mut i = 0;
    while i < rows.len() {
        let kind = match &rows[i] {
            DiffRow::Ctx(_) => {
                i += 1;
                continue;
            }
            DiffRow::Del(_) => "removed",
            DiffRow::Ins(_) => "added",
            DiffRow::Chg(_, _) => "changed",
        };
        let start = i;
        while i < rows.len() && same_kind(&rows[start], &rows[i]) {
            i += 1;
        }
        let span = Some((start, i));
        let (old, new) = match kind {
            "removed" => (span, None),
            "added" => (None, span),
            _ => (span, span),
        };
        out.push(Block { kind, old, new });
    }
    out
}

fn same_kind(a: &DiffRow, b: &DiffRow) -> bool {
    matches!(
        (a, b),
        (DiffRow::Del(_), DiffRow::Del(_))
            | (DiffRow::Ins(_), DiffRow::Ins(_))
            | (DiffRow::Chg(_, _), DiffRow::Chg(_, _))
    )
}

fn blocks_json(rows: &[DiffRow]) -> String {
    let mut out = String::from("{\"blocks\":[");
    for (i, block) in blocks(rows).iter().enumerate() {
        if i > 0 {
            out.push(',');
        }
        let _ = write!(out, "{{\"k\":\"{}\"", block.kind);
        if let Some((from, to)) = block.old {
            let _ = write!(out, ",\"o\":[{from},{to}]");
        }
        if let Some((from, to)) = block.new {
            let _ = write!(out, ",\"n\":[{from},{to}]");
        }
        out.push('}');
    }
    out.push_str("]}");
    out
}

/// Tabs become four spaces: the browser's own tab stops would not line up with
/// the pane's `pre` layout consistently across rows.
fn expand_tabs(text: &str) -> String {
    text.replace('\t', "    ")
}

fn escape(text: &str) -> String {
    let mut out = String::with_capacity(text.len());
    for ch in text.chars() {
        match ch {
            '&' => out.push_str("&amp;"),
            '<' => out.push_str("&lt;"),
            '>' => out.push_str("&gt;"),
            '"' => out.push_str("&quot;"),
            '\'' => out.push_str("&#39;"),
            c => out.push(c),
        }
    }
    out
}

const STYLE: &str = include_str!("style.css");

const SCRIPT: &str = include_str!("rails.js");

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn escapes_markup_in_code() {
        assert_eq!(escape("<a> & 'b'"), "&lt;a&gt; &amp; &#39;b&#39;");
    }

    #[test]
    fn blocks_are_runs_of_one_kind() {
        let rows = vec![
            DiffRow::Ctx("a".into()),
            DiffRow::Del("b".into()),
            DiffRow::Ins("B".into()),
            DiffRow::Ctx("c".into()),
            DiffRow::Ins("x".into()),
            DiffRow::Ins("y".into()),
        ];
        let bs = blocks(&rows);
        assert_eq!(bs.len(), 3);
        assert_eq!(bs[0].kind, "removed");
        assert_eq!(bs[0].old, Some((1, 2)));
        assert_eq!(bs[0].new, None);
        assert_eq!(bs[1].kind, "added");
        assert_eq!(bs[1].old, None);
        assert_eq!(bs[1].new, Some((2, 3)));
        assert_eq!(bs[2].new, Some((4, 6)));
    }

    #[test]
    fn a_changed_run_spans_both_sides() {
        let rows = vec![
            DiffRow::Chg("a".into(), "A".into()),
            DiffRow::Chg("b".into(), "B".into()),
            DiffRow::Ctx("c".into()),
        ];
        let bs = blocks(&rows);
        assert_eq!(bs.len(), 1);
        assert_eq!(bs[0].kind, "changed");
        assert_eq!(bs[0].old, Some((0, 2)));
        assert_eq!(bs[0].new, Some((0, 2)));
    }

    #[test]
    fn rows_after_a_block_are_not_tagged() {
        let rows = vec![DiffRow::Ctx("a".into()), DiffRow::Ins("x".into())];
        assert_eq!(block_rows(&rows), vec![None, Some(0)]);
    }

    #[test]
    fn page_contains_both_panes_and_the_block_data() {
        let diff = Diff {
            path: "a.rs".into(),
            old_label: "index".into(),
            new_label: "working tree".into(),
            rows: vec![DiffRow::Ctx("a".into()), DiffRow::Ins("b".into())],
            note: None,
        };
        let html = page(&[diff], "/repo", true);
        assert!(html.contains("class=\"pane old\""));
        assert!(html.contains("class=\"pane new\""));
        assert!(html.contains("\"k\":\"added\""));
        assert!(html.contains("data-diff=\"0\""));
    }
}
