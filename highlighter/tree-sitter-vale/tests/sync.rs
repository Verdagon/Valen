
use std::collections::HashSet;
use std::path::{Path, PathBuf};

use tree_sitter::{Node, Parser};

fn parse(src: &str) -> tree_sitter::Tree {
    let mut parser = Parser::new();
    parser
        .set_language(&tree_sitter_vale::LANGUAGE.into())
        .expect("load Vale grammar into a tree-sitter Parser");
    parser.parse(src, None).expect("parser produced a tree")
}

fn first_error(src: &str) -> Option<(usize, usize)> {
    all_errors(src).into_iter().next().map(|e| (e.row, e.col))
}

struct ErrorNode {
    row: usize,
    col: usize,
    kind: &'static str,
    text: String,
}

fn all_errors(src: &str) -> Vec<ErrorNode> {
    fn walk(node: Node, src: &str, out: &mut Vec<ErrorNode>) {
        if node.is_error() || node.is_missing() {
            let p = node.start_position();
            let text = node
                .utf8_text(src.as_bytes())
                .unwrap_or("<non-utf8>")
                .lines()
                .next()
                .unwrap_or("")
                .to_string();
            out.push(ErrorNode {
                row: p.row,
                col: p.column,
                kind: if node.is_missing() { "MISSING" } else { "ERROR" },
                text,
            });
            // Don't descend into an error node's synthetic children.
            return;
        }
        let mut cursor = node.walk();
        for child in node.children(&mut cursor) {
            walk(child, src, out);
        }
    }
    let tree = parse(src);
    let mut out = Vec::new();
    walk(tree.root_node(), src, &mut out);
    out
}

fn error_report(src: &str) -> String {
    all_errors(src)
        .iter()
        .map(|e| format!("  {}:{} [{}] `{}`", e.row + 1, e.col + 1, e.kind, e.text))
        .collect::<Vec<_>>()
        .join("\n")
}

#[test]
fn harness_accepts_valid_source() {
    assert_eq!(
        first_error("func main() { }\n"),
        None,
        "a trivially valid Vale function should parse without error nodes"
    );
}

#[test]
fn harness_flags_invalid_source() {
    assert!(
        first_error("func main( { }\n").is_some(),
        "the harness should surface an error node for structurally broken source"
    );
}

// ---------------------------------------------------------------------------
// L1: the grammar parses real Vale programs the compiler accepts.
// ---------------------------------------------------------------------------

#[test]
fn driver_valen_parses_clean() {
    let source = include_str!("fixtures/driver.valen");
    assert!(
        first_error(source).is_none(),
        "driver.valen must parse with no ERROR/MISSING nodes; found:\n{}",
        error_report(source)
    );
}

// ---------------------------------------------------------------------------
// L2: every keyword the compiler treats specially is actually highlighted.
// ---------------------------------------------------------------------------

const COMPILER_KEYWORDS: &[&str] = &[
    // declarations / structure
    "func", "import", "export", "impl", "struct", "interface", "for", "where",
    // attributes / modifiers
    "abstract", "sealed", "exported", "extern", "unsafe", "virtual",
    // control flow
    "if", "else", "while", "foreach", "in", "break", "return", "set", "destruct", "unlet", "as",
    // ownership / mutability
    "own", "weak", "share", "mut", "imm",
    // operators-as-words
    "and", "or", "not",
    // values / receiver
    "true", "false", "self", "this",
];

const NODE_CAPTURED_KEYWORDS: &[&str] = &[
    "abstract", "pure", "unsafe", "weakable", "sealed", "linear", "additive", "exported", "extern",
    "true", "false",
];

fn highlighted_literals() -> HashSet<String> {
    let scm = include_str!("../queries/highlights.scm");
    let mut out = HashSet::new();
    let mut chars = scm.chars().peekable();
    let mut in_string = false;
    let mut current = String::new();
    while let Some(c) = chars.next() {
        if in_string {
            if c == '"' {
                out.insert(std::mem::take(&mut current));
                in_string = false;
            } else {
                current.push(c);
            }
        } else if c == '"' {
            in_string = true;
        } else if c == ';' {
            // Comment to end of line.
            for skip in chars.by_ref() {
                if skip == '\n' {
                    break;
                }
            }
        }
    }
    out
}

#[test]
fn every_compiler_keyword_is_highlighted() {
    let literals = highlighted_literals();
    let node_captured: HashSet<&str> = NODE_CAPTURED_KEYWORDS.iter().copied().collect();
    let missing: Vec<&str> = COMPILER_KEYWORDS
        .iter()
        .copied()
        .filter(|k| !literals.contains(*k) && !node_captured.contains(k))
        .collect();
    assert!(
        missing.is_empty(),
        "compiler keywords not highlighted by queries/highlights.scm: {missing:?}"
    );
}


const CONSTRUCTS: &[(&str, &str)] = &[
    // struct / interface headers: `share` sharedness, empty `;`/`{}` bodies,
    // `where` clauses before the body, numeric-named fields.
    ("struct share body", "struct Muta share { hp int; }\n"),
    ("interface share empty", "sealed exported interface IShip share { }\n"),
    ("struct where func bound", "struct MySome<T> where func drop(T)void { x T; }\n"),
    ("interface where operator bound", "interface Eq<T> where func ==(&T, &T)bool { }\n"),
    ("struct where implements", "struct S<T> where implements(T, U) { }\n"),
    ("struct numeric fields", "struct Tup2<T0, T1> { 0 T0; 1 T1; }\n"),
    // expressions / statements
    ("typed int literal", "func f() int { return 73i64; }\n"),
    ("parenless if expr", "func f() int { return if x == 6 { 1 } else { 2 }; }\n"),
    ("parenless while", "func f() void { while notDone { doThing(); } }\n"),
    // `<` disambiguation (whitespace rule): comparison vs generic call.
    ("space-both comparison", "func f() bool { return a < b; }\n"),
    ("while less-than", "func f() void { while i < 42 { g(); } }\n"),
    ("generic method call", "func f() void { m = ship.try_as<Raza>(); }\n"),
    ("impl generics", "impl<T> Opt<T> for Some<T>;\n"),
    ("owning move expr", "func f() void { drop(^ownMuta); }\n"),
    ("array literal", "func f() void { t = IntTriple([#](6, 14, 22)); }\n"),
    ("tuple literal", "func f() void { t = (true, 42); }\n"),
    ("typed let binding", "func f() void { maybeRaza Result<Raza, IShip> = ship.try_as<Raza>(); }\n"),
    ("sequence destructure", "func f() void { [a, b] = ^tup; }\n"),
];

#[test]
fn language_constructs_parse() {
    let failures: Vec<String> = CONSTRUCTS
        .iter()
        .filter_map(|(label, src)| {
            first_error(src).map(|(r, c)| format!("  [{label}] error at {}:{}", r + 1, c + 1))
        })
        .collect();
    assert!(
        failures.is_empty(),
        "constructs that fail to parse:\n{}",
        failures.join("\n")
    );
}

// ---------------------------------------------------------------------------
// L1 (corpus): the grammar parses the compiler's own .vale test programs.
// ---------------------------------------------------------------------------

fn repo_root() -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR"))
        .join("../..")
        .canonicalize()
        .expect("resolve repo root from the grammar crate")
}

fn collect_vale_files(dir: &Path, out: &mut Vec<PathBuf>) {
    let Ok(entries) = std::fs::read_dir(dir) else {
        return;
    };
    for entry in entries.flatten() {
        let path = entry.path();
        if path.is_dir() {
            collect_vale_files(&path, out);
        } else if path.extension().and_then(|s| s.to_str()) == Some("vale") {
            out.push(path);
        }
    }
}

#[test]
fn corpus_parses_clean() {
    let root = repo_root();
    let mut files = Vec::new();
    collect_vale_files(&root.join("src/tests/programs"), &mut files);
    collect_vale_files(&root.join("src/builtins/resources"), &mut files);
    files.sort();

    let mut failures = Vec::new();
    for f in &files {
        let rel = f.strip_prefix(&root).unwrap_or(f);
        let src = std::fs::read_to_string(f).expect("read corpus .vale file");
        if let Some((row, col)) = first_error(&src) {
            failures.push(format!("{}:{}:{}", rel.display(), row + 1, col + 1));
        }
    }

    assert!(
        failures.is_empty(),
        "{} of {} corpus .vale files have parse errors:\n{}",
        failures.len(),
        files.len(),
        failures.join("\n")
    );
}
