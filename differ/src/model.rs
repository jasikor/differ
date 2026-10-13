//! Diff model: aligned rows and intra-line segmentation, built on `similar`.
//!
//! Ported from the original design sketch.

use similar::{ChangeTag, TextDiff};

#[derive(Clone, Copy, PartialEq)]
pub enum Kind {
    Equal,
    Added,
    Removed,
    Changed,
}

pub type Segs = Vec<(String, Kind)>;
pub type Rows = (usize, usize);

pub enum DiffRow {
    Ctx(String),
    Chg(String, String),
    Del(String),
    Ins(String),
}

/// Align two texts into display rows.
pub fn diff_rows(old: &str, new: &str) -> Vec<DiffRow> {
    let mut rows = Vec::new();
    let mut dels: Vec<String> = Vec::new();
    let mut ins: Vec<String> = Vec::new();
    for change in TextDiff::from_lines(old, new).iter_all_changes() {
        let value = change.value().trim_end_matches(['\n', '\r']).to_string();
        match change.tag() {
            ChangeTag::Equal => {
                flush_rows(&mut dels, &mut ins, &mut rows);
                rows.push(DiffRow::Ctx(value));
            }
            ChangeTag::Delete => dels.push(value),
            ChangeTag::Insert => ins.push(value),
        }
    }
    flush_rows(&mut dels, &mut ins, &mut rows);
    rows
}

fn similarity(a: &str, b: &str) -> f64 {
    let equal: usize = TextDiff::from_chars(a, b)
        .iter_all_changes()
        .filter(|c| c.tag() == ChangeTag::Equal)
        .map(|c| c.value().chars().count())
        .sum();
    let total = a.chars().count().max(b.chars().count()).max(1);
    equal as f64 / total as f64
}

fn flush_rows(dels: &mut Vec<String>, ins: &mut Vec<String>, rows: &mut Vec<DiffRow>) {
    // Pair removed and added lines by similarity (not position), so that a hunk
    // with unequal counts still matches the lines that actually correspond.
    let mut matched: Vec<Option<usize>> = vec![None; dels.len()];
    let mut used = vec![false; ins.len()];
    let mut candidates: Vec<(f64, usize, usize)> = Vec::new();
    for (i, d) in dels.iter().enumerate() {
        for (j, n) in ins.iter().enumerate() {
            candidates.push((similarity(d, n), i, j));
        }
    }
    candidates.sort_by(|a, b| b.0.partial_cmp(&a.0).unwrap_or(std::cmp::Ordering::Equal));
    for (_, i, j) in candidates {
        if matched[i].is_none() && !used[j] {
            matched[i] = Some(j);
            used[j] = true;
        }
    }
    for (i, line) in dels.iter().enumerate() {
        match matched[i] {
            Some(j) => rows.push(DiffRow::Chg(line.clone(), ins[j].clone())),
            None => rows.push(DiffRow::Del(line.clone())),
        }
    }
    for (j, line) in ins.iter().enumerate() {
        if !used[j] {
            rows.push(DiffRow::Ins(line.clone()));
        }
    }
    dels.clear();
    ins.clear();
}

/// Intra-line (word/char) segmentation for a changed pair.
pub fn intra_line(old: &str, new: &str) -> (Segs, Segs) {
    let mut o: Segs = Vec::new();
    let mut n: Segs = Vec::new();
    let mut del = String::new();
    let mut ins = String::new();

    for change in TextDiff::from_words(old, new).iter_all_changes() {
        match change.tag() {
            ChangeTag::Equal => {
                flush_word(&del, &ins, &mut o, &mut n);
                del.clear();
                ins.clear();
                o.push((change.value().to_string(), Kind::Equal));
                n.push((change.value().to_string(), Kind::Equal));
            }
            ChangeTag::Delete => del.push_str(change.value()),
            ChangeTag::Insert => ins.push_str(change.value()),
        }
    }
    flush_word(&del, &ins, &mut o, &mut n);
    collapse_if_rewritten(&mut o, &mut n);
    (o, n)
}

fn flush_word(del: &str, ins: &str, o: &mut Segs, n: &mut Segs) {
    if del.is_empty() && ins.is_empty() {
        return;
    }
    if ins.is_empty() {
        o.push((del.to_string(), Kind::Removed));
    } else if del.is_empty() {
        n.push((ins.to_string(), Kind::Added));
    } else {
        let mut cdel = String::new();
        let mut cins = String::new();
        for ch in TextDiff::from_chars(del, ins).iter_all_changes() {
            match ch.tag() {
                ChangeTag::Equal => {
                    flush_pair(&cdel, &cins, o, n);
                    cdel.clear();
                    cins.clear();
                    o.push((ch.value().to_string(), Kind::Equal));
                    n.push((ch.value().to_string(), Kind::Equal));
                }
                ChangeTag::Delete => cdel.push_str(ch.value()),
                ChangeTag::Insert => cins.push_str(ch.value()),
            }
        }
        flush_pair(&cdel, &cins, o, n);
    }
}

fn flush_pair(cdel: &str, cins: &str, o: &mut Segs, n: &mut Segs) {
    match (cdel.is_empty(), cins.is_empty()) {
        (false, false) => {
            o.push((cdel.to_string(), Kind::Changed));
            n.push((cins.to_string(), Kind::Changed));
        }
        (false, true) => o.push((cdel.to_string(), Kind::Removed)),
        (true, false) => n.push((cins.to_string(), Kind::Added)),
        (true, true) => {}
    }
}

fn collapse_if_rewritten(o: &mut Segs, n: &mut Segs) {
    let frac = |segs: &Segs| -> f64 {
        let total: usize = segs.iter().map(|(t, _)| t.chars().count()).sum();
        let changed: usize = segs
            .iter()
            .filter(|(_, k)| *k != Kind::Equal)
            .map(|(t, _)| t.chars().count())
            .sum();
        if total == 0 {
            0.0
        } else {
            changed as f64 / total as f64
        }
    };
    if frac(o) > 0.6 && frac(n) > 0.6 {
        let ot: String = o.iter().map(|(t, _)| t.as_str()).collect();
        let nt: String = n.iter().map(|(t, _)| t.as_str()).collect();
        *o = vec![(ot, Kind::Changed)];
        *n = vec![(nt, Kind::Changed)];
    }
}

/// A contiguous change block: the row it begins at, plus the old and new line
/// index at that row.
pub struct Hunk {
    pub row: usize,
    pub old: usize,
    pub new: usize,
}

/// Contiguous change hunks in row order.
pub fn hunks(rows: &[DiffRow]) -> Vec<Hunk> {
    let mut out = Vec::new();
    let (mut old, mut new) = (0usize, 0usize);
    let mut i = 0;
    while i < rows.len() {
        if let DiffRow::Ctx(_) = &rows[i] {
            old += 1;
            new += 1;
            i += 1;
            continue;
        }
        let hunk = Hunk { row: i, old, new };
        while i < rows.len() && !matches!(rows[i], DiffRow::Ctx(_)) {
            match &rows[i] {
                DiffRow::Del(_) => old += 1,
                DiffRow::Ins(_) => new += 1,
                _ => {
                    old += 1;
                    new += 1;
                }
            }
            i += 1;
        }
        out.push(hunk);
    }
    out
}

/// Row index of old line `l` (the last row if `l` is past the end).
pub fn old_row(rows: &[DiffRow], l: usize) -> usize {
    let mut old = 0;
    for (i, r) in rows.iter().enumerate() {
        if matches!(r, DiffRow::Ins(_)) {
            continue;
        }
        if old == l {
            return i;
        }
        old += 1;
    }
    rows.len().saturating_sub(1)
}

/// Row index of new line `l` (the last row if `l` is past the end).
pub fn new_row(rows: &[DiffRow], l: usize) -> usize {
    let mut new = 0;
    for (i, r) in rows.iter().enumerate() {
        if matches!(r, DiffRow::Del(_)) {
            continue;
        }
        if new == l {
            return i;
        }
        new += 1;
    }
    rows.len().saturating_sub(1)
}

/// Per-line change kind for one side (context lines map to `None`), used by the
/// overview rulers.
pub fn side_kinds(rows: &[DiffRow], old_side: bool) -> Vec<Option<Kind>> {
    let mut out = Vec::new();
    for row in rows {
        match row {
            DiffRow::Ctx(_) => out.push(None),
            DiffRow::Chg(_, _) => out.push(Some(Kind::Changed)),
            DiffRow::Del(_) => {
                if old_side {
                    out.push(Some(Kind::Removed));
                }
            }
            DiffRow::Ins(_) => {
                if !old_side {
                    out.push(Some(Kind::Added));
                }
            }
        }
    }
    out
}

/// Line pairing between the two sides, used to scroll them together.
///
/// `new_of_old[i]` is the new line that pairs with old line `i` (for a removed
/// line, the new insertion point); `old_of_new[j]` is the dual. Scrolling one
/// side then re-derives the other from its map, so the panes stay aligned as
/// best they can without padding.
pub fn line_maps(rows: &[DiffRow]) -> (Vec<usize>, Vec<usize>) {
    let mut new_of_old = Vec::new();
    let mut old_of_new = Vec::new();
    let (mut oi, mut ni) = (0usize, 0usize);
    for row in rows {
        match row {
            DiffRow::Ctx(_) | DiffRow::Chg(_, _) => {
                new_of_old.push(ni);
                old_of_new.push(oi);
                oi += 1;
                ni += 1;
            }
            DiffRow::Del(_) => {
                new_of_old.push(ni);
                oi += 1;
            }
            DiffRow::Ins(_) => {
                old_of_new.push(oi);
                ni += 1;
            }
        }
    }
    (new_of_old, old_of_new)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn identical_is_all_context() {
        let rows = diff_rows("a\nb\n", "a\nb\n");
        assert_eq!(rows.len(), 2);
        assert!(rows.iter().all(|r| matches!(r, DiffRow::Ctx(_))));
    }

    #[test]
    fn insertion_yields_ins() {
        let rows = diff_rows("a\nc\n", "a\nb\nc\n");
        assert!(rows.iter().any(|r| matches!(r, DiffRow::Ins(_))));
    }

    #[test]
    fn deletion_yields_del() {
        let rows = diff_rows("a\nb\nc\n", "a\nc\n");
        assert!(rows.iter().any(|r| matches!(r, DiffRow::Del(_))));
    }

    #[test]
    fn replacement_pairs_into_chg() {
        let rows = diff_rows("a\nold line\nc\n", "a\nnew line\nc\n");
        assert!(rows.iter().any(|r| matches!(r, DiffRow::Chg(_, _))));
    }

    #[test]
    fn hunks_split_on_context() {
        let rows = vec![
            DiffRow::Ctx("a".into()),
            DiffRow::Del("b".into()),
            DiffRow::Ins("B".into()),
            DiffRow::Ctx("c".into()),
            DiffRow::Chg("d".into(), "D".into()),
            DiffRow::Ctx("e".into()),
        ];
        let hs = hunks(&rows);
        assert_eq!(hs.len(), 2);
        assert_eq!((hs[0].row, hs[0].old, hs[0].new), (1, 1, 1));
        assert_eq!((hs[1].row, hs[1].old, hs[1].new), (4, 3, 3));
    }

    #[test]
    fn leading_insert_hunk_starts_on_the_new_side() {
        let rows = vec![
            DiffRow::Ctx("a".into()),
            DiffRow::Ins("X".into()),
            DiffRow::Ins("Y".into()),
            DiffRow::Ctx("b".into()),
        ];
        let hs = hunks(&rows);
        assert_eq!((hs[0].row, hs[0].old, hs[0].new), (1, 1, 1));
        assert!(matches!(rows[hs[0].row], DiffRow::Ins(_)));
    }

    #[test]
    fn row_lookups_skip_the_other_side() {
        let rows = vec![
            DiffRow::Ctx("a".into()),
            DiffRow::Ins("X".into()),
            DiffRow::Ctx("b".into()),
            DiffRow::Del("c".into()),
            DiffRow::Ctx("d".into()),
        ];
        // old lines: a(0) b(1) c(2) d(3); new lines: a(0) X(1) b(2) d(3)
        assert_eq!(old_row(&rows, 0), 0);
        assert_eq!(old_row(&rows, 1), 2);
        assert_eq!(old_row(&rows, 2), 3);
        assert_eq!(new_row(&rows, 0), 0);
        assert_eq!(new_row(&rows, 1), 1);
        assert_eq!(new_row(&rows, 2), 2);
        assert_eq!(new_row(&rows, 3), 4);
    }

    #[test]
    fn line_maps_pair_across_blocks() {
        let rows = vec![
            DiffRow::Ctx("a".into()),
            DiffRow::Del("b".into()),
            DiffRow::Ins("B".into()),
            DiffRow::Ctx("c".into()),
        ];
        let (new_of_old, old_of_new) = line_maps(&rows);
        assert_eq!(new_of_old, vec![0, 1, 2]);
        assert_eq!(old_of_new, vec![0, 2, 2]);
    }
}
