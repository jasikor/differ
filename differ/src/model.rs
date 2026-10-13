//! Diff model: aligned rows and intra-line segmentation, built on `similar`.
//!
//! Ported from the mockup (`mockup/src/main.rs`).

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

/// Old-view row offsets where each change hunk begins.
pub fn hunk_offsets(rows: &[DiffRow]) -> Vec<usize> {
    let mut starts = Vec::new();
    let mut old = 0usize;
    let mut prev_change = false;
    for row in rows {
        let is_change = !matches!(row, DiffRow::Ctx(_));
        if is_change && !prev_change {
            starts.push(old);
        }
        if !matches!(row, DiffRow::Ins(_)) {
            old += 1;
        }
        prev_change = is_change;
    }
    starts
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
    fn hunk_offsets_finds_change_blocks() {
        let rows = vec![
            DiffRow::Ctx("a".into()),
            DiffRow::Del("b".into()),
            DiffRow::Ins("B".into()),
            DiffRow::Ctx("c".into()),
            DiffRow::Chg("d".into(), "D".into()),
            DiffRow::Ctx("e".into()),
        ];
        assert_eq!(hunk_offsets(&rows), vec![1, 3]);
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
