use std::io::{self, Read, Write};

use vmd_highlighter::{html_escape, run};

fn main() {
    let mut lang = String::new();
    let mut args = std::env::args().skip(1);
    while let Some(a) = args.next() {
        match a.as_str() {
            "--lang" => lang = args.next().unwrap_or_default(),
            other => {
                eprintln!("unexpected arg: {other}");
                std::process::exit(2);
            }
        }
    }

    let mut source = String::new();
    if let Err(e) = io::stdin().read_to_string(&mut source) {
        eprintln!("failed to read stdin: {e}");
        std::process::exit(1);
    }

    match run(&lang, &source) {
        Ok(html) => {
            let _ = io::stdout().write_all(html.as_bytes());
        }
        Err(e) => {
            eprintln!("highlighting failed for lang '{lang}': {e}");
            // Fall back to escaped plaintext so the build doesn't die on a bad snippet.
            let _ = io::stdout().write_all(html_escape(&source).as_bytes());
            std::process::exit(1);
        }
    }
}
