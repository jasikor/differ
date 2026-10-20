//! Naive symbol lookup: ripgrep-backed definitions and references.
//!
//! ripgrep finds the candidates. Rust files are then checked against a
//! tree-sitter syntax tree, so comments and strings are skipped and enum
//! variants, fields, aliases and raw identifiers work. Other languages fall
//! back to declaration keywords (plus a C/Go/JS-style `name(...) {` shape). It
//! is not a replacement for an LSP; it is a fast first pass without a server.

use std::collections::BTreeMap;
use std::path::{Path, PathBuf};
use std::process::Command;

use tree_sitter::{Node, Parser, Tree};

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
///
/// ripgrep finds the candidates; Rust files are then checked against a
/// tree-sitter syntax tree so comments, strings, enum variants and fields are
/// classified correctly.
pub fn find(root: &Path, word: &str, kind: SearchKind) -> Result<Vec<Location>, String> {
    if word.is_empty() {
        return Ok(Vec::new());
    }
    let mut by_file: BTreeMap<PathBuf, Vec<Location>> = BTreeMap::new();
    for loc in run_ripgrep(root, word)? {
        by_file.entry(loc.path.clone()).or_default().push(loc);
    }

    let mut parser = rust_parser();
    let mut out = Vec::new();
    for (path, hits) in by_file {
        let rust = path.extension().and_then(|ext| ext.to_str()) == Some("rs");
        if rust {
            if let Some(kept) = rust_hits(&mut parser, &path, &hits, word, kind) {
                out.extend(kept);
                continue;
            }
        }
        // Non-Rust files (and Rust files that could not be parsed) keep the
        // line-level heuristic.
        out.extend(
            hits.into_iter()
                .filter(|loc| kind == SearchKind::References || is_definition(&loc.text, word)),
        );
    }

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

fn rust_parser() -> Option<Parser> {
    let mut parser = Parser::new();
    parser
        .set_language(&tree_sitter_rust::LANGUAGE.into())
        .ok()?;
    Some(parser)
}

fn rust_hits(
    parser: &mut Option<Parser>,
    path: &Path,
    hits: &[Location],
    word: &str,
    kind: SearchKind,
) -> Option<Vec<Location>> {
    let parser = parser.as_mut()?;
    let source = std::fs::read_to_string(path).ok()?;
    let tree = parser.parse(&source, None)?;
    Some(
        hits.iter()
            .filter(|loc| keep_rust_hit(&tree, &source, loc, word, kind))
            .cloned()
            .collect(),
    )
}

fn keep_rust_hit(tree: &Tree, source: &str, loc: &Location, word: &str, kind: SearchKind) -> bool {
    let Some(offset) = absolute_offset(source, loc.line, loc.byte_col) else {
        return false;
    };
    let end = (offset + word.len()).min(source.len());
    let Some(node) = tree.root_node().descendant_for_byte_range(offset, end) else {
        return false;
    };
    if !is_name_node(node) || !node_text_matches(node, source, word) {
        return false;
    }
    kind == SearchKind::References || is_definition_node(node)
}

fn is_name_node(node: Node) -> bool {
    matches!(
        node.kind(),
        "identifier" | "type_identifier" | "field_identifier" | "shorthand_field_identifier"
    )
}

fn node_text_matches(node: Node, source: &str, word: &str) -> bool {
    let Some(text) = source.get(node.byte_range()) else {
        return false;
    };
    text == word || text.strip_prefix("r#") == Some(word)
}

fn absolute_offset(source: &str, line: usize, byte_col: usize) -> Option<usize> {
    let mut offset = 0;
    for (index, text) in source.split_inclusive('\n').enumerate() {
        if index == line {
            let line = text.trim_end_matches('\n').trim_end_matches('\r');
            return Some(offset + byte_col.min(line.len()));
        }
        offset += text.len();
    }
    None
}

/// Node kinds whose `name` field declares the symbol.
const NAME_DEFINITIONS: &[&str] = &[
    "function_item",
    "function_signature_item",
    "struct_item",
    "enum_item",
    "union_item",
    "trait_item",
    "type_item",
    "associated_type",
    "const_item",
    "static_item",
    "mod_item",
    "macro_definition",
    "enum_variant",
    "field_declaration",
    "type_parameter",
    "const_parameter",
];

/// Node kinds whose `pattern` field introduces bindings.
const PATTERN_DEFINITIONS: &[&str] = &[
    "let_declaration",
    "let_condition",
    "parameter",
    "for_expression",
    "match_arm",
];

fn is_definition_node(node: Node) -> bool {
    let mut current = Some(node);
    while let Some(ancestor) = current {
        if NAME_DEFINITIONS.contains(&ancestor.kind()) {
            if let Some(name) = ancestor.child_by_field_name("name") {
                if name.id() == node.id() {
                    return true;
                }
            }
        }
        if PATTERN_DEFINITIONS.contains(&ancestor.kind()) {
            if let Some(pattern) = ancestor.child_by_field_name("pattern") {
                // Only a plain identifier pattern is a binding. A range check
                // would also match the variant name in `Some(x)`.
                if pattern.id() == node.id() {
                    return true;
                }
            }
        }
        if ancestor.kind() == "use_as_clause" {
            if let Some(alias) = ancestor.child_by_field_name("alias") {
                if alias.id() == node.id() {
                    return true;
                }
            }
        }
        current = ancestor.parent();
    }
    false
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

    fn rust_hit_at(source: &str, word: &str, kind: SearchKind) -> bool {
        let mut parser = rust_parser().expect("Rust parser");
        let tree = parser.parse(source, None).expect("syntax tree");
        let offset = source.find(word).expect("word in source");
        let line = source[..offset].matches('\n').count();
        let line_start = source[..offset].rfind('\n').map(|i| i + 1).unwrap_or(0);
        let text = source.lines().nth(line).unwrap_or("").to_string();
        let loc = Location {
            path: PathBuf::from("test.rs"),
            line,
            byte_col: offset - line_start,
            text,
        };
        keep_rust_hit(&tree, source, &loc, word, kind)
    }

    #[test]
    fn rust_tree_recognises_variants_fields_aliases_and_raw_idents() {
        let defs = [
            ("enum E { Variant }\n", "Variant"),
            ("struct S { field: u32 }\n", "field"),
            ("use foo::bar as Alias;\n", "Alias"),
            ("fn r#type() {}\n", "type"),
            ("fn f<T>(x: T) {}\n", "T"),
            ("fn f(x: u32) {}\n", "x"),
            ("fn f() { let value = 1; }\n", "value"),
            ("trait T { fn method(&self); }\n", "method"),
        ];
        for (source, word) in defs {
            assert!(
                rust_hit_at(source, word, SearchKind::Definition),
                "{word} should be a definition in {source:?}"
            );
        }
    }

    #[test]
    fn rust_tree_ignores_comments_and_strings() {
        let ignored = [
            ("// fn fake() {}\n", "fake"),
            ("let s = \"fn fake() {}\";\n", "fake"),
            ("/// struct Fake;\n", "Fake"),
        ];
        for (source, word) in ignored {
            assert!(
                !rust_hit_at(source, word, SearchKind::Definition),
                "{word} should not be a definition in {source:?}"
            );
            assert!(
                !rust_hit_at(source, word, SearchKind::References),
                "{word} should not be a reference in {source:?}"
            );
        }
    }

    #[test]
    fn rust_tree_keeps_real_references() {
        assert!(rust_hit_at(
            "fn f() { let _ = E::Variant; }\n",
            "Variant",
            SearchKind::References
        ));
        assert!(rust_hit_at(
            "fn f() { let _ = object.field; }\n",
            "field",
            SearchKind::References
        ));
        assert!(rust_hit_at(
            "fn f() { println!(\"{}\", helper()); }\n",
            "helper",
            SearchKind::References
        ));
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
