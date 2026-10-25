use highlight::Highlighter;

fn highlighter() -> Highlighter {
    Highlighter::new().expect("highlighter should init")
}

#[test]
fn recognizes_languages_used_in_content() {
    let h = highlighter();
    for lang in [
        "typescript",
        "ts",
        "tsx",
        "jsx",
        "js",
        "javascript",
        "rust",
        "rs",
        "bash",
        "sh",
        "toml",
        "yaml",
        "yml",
        "Markdown",
        "md",
        "yamd",
        "ARM Assembly",
        "Plain",
        "text",
        "",
    ] {
        let out = h.highlight("x", lang).expect("highlight");
        assert!(out.recognized, "language '{lang}' should be recognized");
    }
}

#[test]
fn unknown_language_falls_back_to_escaped_plain_text() {
    let out = highlighter().highlight("a < b", "rsut").expect("highlight");
    assert!(!out.recognized);
    assert!(out.html.contains("a &lt; b"), "got: {}", out.html);
}

#[test]
fn rust_emits_scope_classes() {
    let out = highlighter()
        .highlight("fn main() {}\n", "rust")
        .expect("highlight");
    assert!(
        out.html
            .contains("<span class=\"storage type function rust\">fn</span>"),
        "got: {}",
        out.html
    );
    assert!(
        out.html.contains("entity name function"),
        "got: {}",
        out.html
    );
}

#[test]
fn toml_is_highlighted() {
    let out = highlighter()
        .highlight("[package]\nname = \"bar\"\n", "toml")
        .expect("highlight");
    assert!(out.html.contains("source toml"), "got: {}", out.html);
    assert!(out.html.contains("string"), "got: {}", out.html);
}

#[test]
fn typescript_uses_tsx_grammar() {
    let out = highlighter()
        .highlight("const a = <div />;\n", "ts")
        .expect("highlight");
    assert!(out.html.contains("source tsx"), "got: {}", out.html);
}

#[test]
fn arm_assembly_is_highlighted() {
    let out = highlighter()
        .highlight("    ldrb   w16, [x14, x11]   ; comment\n", "ARM Assembly")
        .expect("highlight");
    assert!(out.html.contains("comment"), "got: {}", out.html);
}
