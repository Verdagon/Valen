use std::sync::OnceLock;

use inkjet::constants::HIGHLIGHT_NAMES;
use inkjet::formatter::Html;
use inkjet::tree_sitter_highlight::HighlightConfiguration;
use inkjet::{Highlighter, InkjetError, Language};

const VALE_HIGHLIGHTS: &str = include_str!("../tree-sitter-vale/queries/highlights.scm");

fn vale_config() -> &'static HighlightConfiguration {
    static CONFIG: OnceLock<HighlightConfiguration> = OnceLock::new();
    CONFIG.get_or_init(|| {
        let mut c = HighlightConfiguration::new(
            tree_sitter_vale::LANGUAGE.into(),
            "vale",
            VALE_HIGHLIGHTS,
            "",
            "",
        )
        .expect("invalid Vale highlights query");
        c.configure(HIGHLIGHT_NAMES);
        c
    })
}

fn language_for(name: &str) -> Option<Language> {
    Some(match name.to_ascii_lowercase().as_str() {
        "vale" => Language::Runtime(vale_config),
        "rust" | "rs" => Language::Rust,
        "c" | "h" => Language::C,
        "cpp" | "c++" | "cxx" | "hpp" => Language::Cpp,
        // Mojo is a Python superset; route through Python's grammar. Mojo-only
        // keywords (fn, var, let, struct, inout, borrowed, owned, mut) tokenize
        // as plain identifiers, but everything else highlights correctly.
        "python" | "py" | "mojo" | "🔥" => Language::Python,
        "javascript" | "js" => Language::Javascript,
        "typescript" | "ts" => Language::Typescript,
        "bash" | "sh" | "shell" => Language::Bash,
        "json" => Language::Json,
        "html" => Language::Html,
        "css" => Language::Css,
        "go" => Language::Go,
        "java" => Language::Java,
        "scala" => Language::Scala,
        "swift" => Language::Swift,
        _ => return None,
    })
}

pub fn html_escape(s: &str) -> String {
    let mut out = String::with_capacity(s.len());
    for ch in s.chars() {
        match ch {
            '&' => out.push_str("&amp;"),
            '<' => out.push_str("&lt;"),
            '>' => out.push_str("&gt;"),
            _ => out.push(ch),
        }
    }
    out
}

pub fn run(lang: &str, source: &str) -> Result<String, InkjetError> {
    let language = match language_for(lang) {
        Some(l) => l,
        None => return Ok(html_escape(source)),
    };
    let mut highlighter = Highlighter::new();
    highlighter.highlight_to_string(language, &Html, source)
}
