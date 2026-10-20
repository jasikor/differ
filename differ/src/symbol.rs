//! Naive symbol lookup: ripgrep-backed definitions and references.
//!
//! This deliberately stays language-agnostic. Definitions are recognised by
//! common declaration keywords (plus a C/Go/JS-style `name(...) {` shape), and
//! references are word-boundary matches. It is not a replacement for an LSP;
//! it is a fast first pass that works without a server.

use std::path::{Path, PathBuf};
use std::process::Command;

/// Keep the result list bounded even for very common words.
const MAX_RESULTS: usize = 300;
/// A field separator that cannot appear in a path or source line.
const FIELD_SEP: char = '\u{1f}';

/// Which kind of lookup to run.
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum SearchKind {
    Definition,
    References,
}

impl SearchKind {
    pub fn title(self) -> &'static str {
        match self {
            SearchKind::Definition => "Definition",
            SearchKind::References => "References",
        }
    }

    pub fn noun(self) -> &'static str {
        match self {
            SearchKind::Definition => "definition",
            SearchKind::References => "reference",
        }
    }
}

/// One source location. `line` and `byte_col` are zero-based; the column is a
/// byte offset inside `text` (ripgrep's `--column`), converted to a character
/// column when the cursor moves there.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Location {
    pub path: PathBuf,
    pub line: usize,
    pub byte_col: usize,
    pub text: String,
}

/// Search `root` for `word`, returning definitions or every reference.
pub fn find(root: &Path, word: &str, kind: SearchKind) -> Result<Vec<Location>, String> {
    if word.is_empty() {
        return Ok(Vec::new());
    }
    let mut out: Vec<Location> = run_ripgrep(root, word)?
        .into_iter()
        .filter(|loc| kind == SearchKind::References || is_definition(&loc.text, word))
        .collect();
    out.sort_by(|a, b| {
        a.path
            .cmp(&b.path)
            .then(a.line.cmp(&b.line))
            .then(a.byte_col.cmp(&b.byte_col))
    });
    out.dedup();
    out.truncate(MAX_RESULTS);
    Ok(out)
}

fn run_ripgrep(root: &Path, word: &str) -> Result<Vec<Location>, String> {
    let output = Command::new("rg")
        .current_dir(root)
        .args([
            "--line-number",
            "--column",
            "--no-heading",
            "--color",
            "never",
            "--field-match-separator",
            "\u{1f}",
            "--max-columns",
            "300",
            "--max-filesize",
            "2M",
            "--fixed-strings",
            "--word-regexp",
        ])
        .arg(word)
        .output()
        .map_err(|e| format!("rg: {e}"))?;

    let stdout = String::from_utf8_lossy(&output.stdout);
    // ripgrep exits 1 when there are simply no matches.
    if !output.status.success() && stdout.is_empty() {
        let stderr = String::from_utf8_lossy(&output.stderr);
        let stderr = stderr.trim();
        return Err(if stderr.is_empty() {
            "rg found no matches".to_string()
        } else {
            format!("rg: {stderr}")
        });
    }

    let mut out = Vec::new();
    for line in stdout.lines() {
        if let Some(loc) = parse_match(root, line) {
            out.push(loc);
            if out.len() >= MAX_RESULTS * 2 {
                break;
            }
        }
    }
    Ok(out)
}

fn parse_match(root: &Path, line: &str) -> Option<Location> {
    let mut fields = line.splitn(4, FIELD_SEP);
    let path = fields.next()?;
    let line_no: usize = fields.next()?.parse().ok()?;
    let col: usize = fields.next()?.parse().ok()?;
    let text = fields.next().unwrap_or("").trim_end_matches('\r');
    if path.is_empty() || line_no == 0 || col == 0 {
        return None;
    }
    Some(Location {
        path: root.join(path),
        line: line_no - 1,
        byte_col: col - 1,
        text: text.to_string(),
    })
}

/// The declaration keywords that count as a definition. The token is the one
/// immediately before the symbol, so `pub fn foo` checks `fn`.
const DEFINITION_KEYWORDS: &[&str] = &[
    "fn",
    "def",
    "class",
    "struct",
    "enum",
    "trait",
    "interface",
    "type",
    "const",
    "static",
    "let",
    "var",
    "function",
    "func",
    "mod",
    "module",
    "namespace",
    "union",
    "protocol",
    "record",
    "macro_rules",
];

fn is_definition(text: &str, word: &str) -> bool {
    let Some(idx) = occurrences(text, word).next() else {
        return false;
    };
    let before = text[..idx].trim_end();
    let token = last_token(before).trim_end_matches('!');
    if DEFINITION_KEYWORDS.contains(&token) {
        return true;
    }

    // C/Go/JS-style function definition without a declaration keyword:
    // `int main(...) {`, `func (r R) name(...) {`, `render() {`.
    let after = text[idx + word.len()..].trim_start();
    if after.starts_with('(') && text.contains('{') && !text.trim_end().ends_with(';') {
        let first = text.split_whitespace().next().unwrap_or("");
        if !matches!(
            first,
            "if" | "while"
                | "for"
                | "match"
                | "return"
                | "else"
                | "switch"
                | "loop"
                | "do"
                | "catch"
        ) {
            return true;
        }
    }
    false
}

fn occurrences<'a>(text: &'a str, word: &'a str) -> impl Iterator<Item = usize> + 'a {
    text.match_indices(word)
        .map(|(idx, _)| idx)
        .filter(move |&idx| {
            let before = text[..idx].chars().next_back();
            let after = text[idx + word.len()..].chars().next();
            before.is_none_or(|c| !is_word_char(c)) && after.is_none_or(|c| !is_word_char(c))
        })
}

fn last_token(before: &str) -> &str {
    let start = before
        .char_indices()
        .rev()
        .find(|(_, c)| !(is_word_char(*c) || *c == '!'))
        .map(|(idx, c)| idx + c.len_utf8())
        .unwrap_or(0);
    &before[start..]
}

fn is_word_char(c: char) -> bool {
    c.is_alphanumeric() || c == '_'
}

#[cfg(test)]
mod tests {
    use super::*;

    fn parse(line: &str) -> Location {
        parse_match(Path::new("/repo"), line).expect("valid rg line")
    }

    #[test]
    fn parses_ripgrep_fields_with_a_custom_separator() {
        let loc = parse("src/main.rs\u{1f}12\u{1f}5\u{1f}    let value = 1;");
        assert_eq!(loc.path, PathBuf::from("/repo/src/main.rs"));
        assert_eq!(loc.line, 11);
        assert_eq!(loc.byte_col, 4);
        assert_eq!(loc.text, "    let value = 1;");
    }

    #[test]
    fn recognises_common_definition_shapes() {
        let cases = [
            ("pub fn render(&self) {}", "render"),
            ("fn main() {", "main"),
            ("    struct Widget {", "Widget"),
            ("async def fetch(url):", "fetch"),
            ("class Foo:", "Foo"),
            ("const VALUE: usize = 3;", "VALUE"),
            ("macro_rules! my_macro {", "my_macro"),
            ("interface User {", "User"),
            ("function handle() {", "handle"),
            ("func (r Repo) status() {", "status"),
            ("int main(void) {", "main"),
            ("render() {", "render"),
        ];
        for (line, word) in cases {
            assert!(is_definition(line, word), "{line:?} should define {word}");
        }
    }

    #[test]
    fn does_not_treat_calls_or_control_flow_as_definitions() {
        let cases = [
            ("let x = render();", "render"),
            ("    return fetch();", "fetch"),
            ("if ready() {", "ready"),
            ("while next() {", "next"),
            ("value.status();", "status"),
            ("int main(void);", "main"),
        ];
        for (line, word) in cases {
            assert!(
                !is_definition(line, word),
                "{line:?} should not define {word}"
            );
        }
    }

    #[test]
    fn word_boundaries_keep_prefixes_out() {
        let mut hits = occurrences("foo foobar foo_bar foo!", "foo");
        assert_eq!(hits.next(), Some(0));
        assert_eq!(hits.next(), Some(19));
        assert_eq!(hits.next(), None);
    }

    #[test]
    fn ripgrep_finds_definitions_and_references() {
        if Command::new("rg").arg("--version").output().is_err() {
            return;
        }
        let dir = std::env::temp_dir().join(format!(
            "differ-symbol-{}-{}",
            std::process::id(),
            std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .unwrap()
                .as_nanos()
        ));
        std::fs::create_dir_all(&dir).unwrap();
        std::fs::write(
            dir.join("lib.rs"),
            "fn helper() {}\n\nfn caller() {\n    helper();\n}\n",
        )
        .unwrap();

        let defs = find(&dir, "helper", SearchKind::Definition).unwrap();
        assert_eq!(defs.len(), 1);
        assert_eq!(defs[0].line, 0);

        let refs = find(&dir, "helper", SearchKind::References).unwrap();
        assert_eq!(refs.len(), 2);
        assert_eq!(refs[0].line, 0);
        assert_eq!(refs[1].line, 3);

        std::fs::remove_dir_all(&dir).unwrap();
    }
}
