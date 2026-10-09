use vmd_highlighter::run;

#[test]
fn vale_func_keyword_is_highlighted() {
    let source = "func main() {}\n";
    let html = run("vale", source).unwrap();
    assert!(
        html.contains(r#"<span class="keyword">func</span>"#),
        "expected `func` to be highlighted as a keyword span; got:\n{html}"
    );
}

#[test]
fn unknown_language_is_escaped_plaintext() {
    let source = "not <a> language\n";
    let html = run("madeuplang", source).unwrap();
    assert_eq!(html, "not &lt;a&gt; language\n");
}

#[test]
fn chevron_note_markers_pass_through() {
    let source = "func main() { println(\"hi «7»\"); }\n";
    let html = run("vale", source).unwrap();
    assert!(
        html.contains("«7»"),
        "expected `«7»` to survive highlighting; got:\n{html}"
    );
}

#[test]
fn mojo_routes_through_python() {
    let source = "def main():\n    print(\"hi\")\n";
    let html = run("mojo", source).unwrap();
    assert!(
        html.contains(r#"<span class="keyword function">def</span>"#),
        "expected `def` keyword span; got:\n{html}"
    );
    assert!(
        html.contains(r#"<span class="string">&quot;hi&quot;</span>"#),
        "expected highlighted string; got:\n{html}"
    );
}

#[test]
fn vale_golden() {
    let source = include_str!("golden/vale_basic.vale");
    let expected = include_str!("golden/vale_basic.expected.html");
    let actual = run("vale", source).unwrap();
    assert_eq!(actual, expected);
}

#[test]
fn ante_is_unknown_language() {
    // Ante is gone: `ante` is no longer a recognized language, so it falls through
    // the unknown-language path and comes back as HTML-escaped plaintext, unhighlighted.
    let source = "type Foo = a & b\n";
    let html = run("ante", source).unwrap();
    assert_eq!(html, "type Foo = a &amp; b\n");
}
