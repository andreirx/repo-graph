//! Comment- and string-aware IMPORT EVIDENCE scanning for the HTTP detectors.
//!
//! review-6 item 1 / STANDING HONESTY RULE 2: import evidence must come from a
//! real `import`/`require`/`from` DECLARATION, never from a bare
//! `content.contains(...)`. A `content.contains("axios")` is satisfied by a
//! comment (`// axios`) or a string literal (`const s = "require('axios')"`) and
//! would emit a false Layer-3 HTTP fact. This module extracts specifiers from
//! actual declarations, ignoring comments and unrelated string-literal contents.
//!
//! ## Abstraction record (STANDING SCOPE PRE-RATIFICATION 2)
//!
//! - **what:** crate-private `imports` submodule — a comment/string-aware
//!   import-declaration scanner over C-family source.
//! - **concrete current users (two):** `typescript::TsHttpEvidence::of`
//!   (axios / api-client / CDK apigatewayv2 module specifiers) and
//!   `java_consumer::JavaClientImports::of` + the Spring provider gate in
//!   `mod.rs` (imported Java packages).
//! - **axis of variation:** two source languages (TS/JS, Java) whose
//!   import-evidence scan must skip comments and string literals — a fixed
//!   operation (find imports) over a growing-by-one set of languages.
//! - **rejected simpler alternative:** keep `content.contains(<pkg>)` — the very
//!   name-only match review-6 rejected (comments / string literals qualify).

/// A minimal token of interest for import scanning. Comments and whitespace are
/// dropped by the tokenizer; string literals keep their *decoded-enough* inner
/// text (backslash escapes collapsed) so a module specifier can be read out.
#[derive(Debug, PartialEq)]
enum Tok {
    Ident(String),
    Str(String),
    Punct(char),
}

/// Tokenize C-family source (TS/JS) into `Ident` / `Str` / `Punct`, skipping
/// `//` line comments, `/* */` block comments, and whitespace. String literals
/// (`"`, `'`, `` ` ``) become a single `Str` token — so the `import`/`from`/
/// `require` keywords that appear INSIDE a string are never seen as `Ident`s,
/// which is exactly what defeats the comment/string false positives.
fn tokenize(src: &str) -> Vec<Tok> {
    let chars: Vec<char> = src.chars().collect();
    let n = chars.len();
    let mut toks = Vec::new();
    let mut i = 0;
    while i < n {
        let c = chars[i];
        if c.is_whitespace() {
            i += 1;
            continue;
        }
        // Line comment.
        if c == '/' && i + 1 < n && chars[i + 1] == '/' {
            i += 2;
            while i < n && chars[i] != '\n' {
                i += 1;
            }
            continue;
        }
        // Block comment.
        if c == '/' && i + 1 < n && chars[i + 1] == '*' {
            i += 2;
            while i + 1 < n && !(chars[i] == '*' && chars[i + 1] == '/') {
                i += 1;
            }
            i += 2; // consume the closing */ (saturates past EOF harmlessly)
            continue;
        }
        // String / template literal.
        if c == '"' || c == '\'' || c == '`' {
            let quote = c;
            i += 1;
            let mut s = String::new();
            while i < n && chars[i] != quote {
                if chars[i] == '\\' && i + 1 < n {
                    s.push(chars[i + 1]);
                    i += 2;
                } else {
                    s.push(chars[i]);
                    i += 1;
                }
            }
            i += 1; // consume the closing quote
            toks.push(Tok::Str(s));
            continue;
        }
        // Identifier (keywords land here too).
        if c.is_alphanumeric() || c == '_' || c == '$' {
            let mut s = String::new();
            while i < n && {
                let d = chars[i];
                d.is_alphanumeric() || d == '_' || d == '$'
            } {
                s.push(chars[i]);
                i += 1;
            }
            toks.push(Tok::Ident(s));
            continue;
        }
        // Any other single char is punctuation we may need (`(`, `.`, `,` …).
        toks.push(Tok::Punct(c));
        i += 1;
    }
    toks
}

/// Module specifiers of every `import` / `export … from` / `require(…)`
/// declaration in TS/JS source, ignoring comments and unrelated string literals.
///
/// Recognized declaration forms (specifier = the quoted module path):
/// - `import x from 'spec'` / `import { a } from 'spec'` / `export … from 'spec'`
///   (the specifier follows the `from` keyword),
/// - `import 'spec'` (side-effect) and `import('spec')` (dynamic),
/// - `require('spec')` / `const x = require('spec')`.
///
/// Multi-line braced imports are handled — the tokenizer is line-agnostic, so a
/// `from 'spec'` on a later physical line is still captured.
pub(super) fn ts_import_specifiers(src: &str) -> Vec<String> {
    let toks = tokenize(src);
    let mut specs = Vec::new();
    for (w, tok) in toks.iter().enumerate() {
        match tok {
            // `… from 'spec'` — the specifier is the string immediately after
            // `from`. `from` used as an identifier/property/key is not followed
            // by a string literal, so it will not match.
            Tok::Ident(id) if id == "from" => {
                if let Some(Tok::Str(s)) = toks.get(w + 1) {
                    specs.push(s.clone());
                }
            }
            // `import 'spec'` (side-effect) or `import('spec')` (dynamic).
            Tok::Ident(id) if id == "import" => match (toks.get(w + 1), toks.get(w + 2)) {
                (Some(Tok::Str(s)), _) => specs.push(s.clone()),
                (Some(Tok::Punct('(')), Some(Tok::Str(s))) => specs.push(s.clone()),
                _ => {}
            },
            // `require('spec')` — `require` as a code identifier followed by `(`
            // then a string. A `require(` inside a string literal is a single
            // `Str` token, never `Ident("require")`, so it cannot match.
            Tok::Ident(id) if id == "require" => {
                if matches!(toks.get(w + 1), Some(Tok::Punct('('))) {
                    if let Some(Tok::Str(s)) = toks.get(w + 2) {
                        specs.push(s.clone());
                    }
                }
            }
            _ => {}
        }
    }
    specs
}

/// Whether a `.java` source has an `import` (or `import static`) declaration for
/// a type under `package_prefix`, ignoring comments and string literals.
///
/// Java imports are single-line statements terminated by `;`, so a line-anchored
/// scan over comment-stripped source is exact here (operator ruling: line-anchored
/// parsing is acceptable). A string literal that merely *contains* import text
/// (`String s = "import org.foo.Bar";`) does not start its line with `import`, so
/// it is rejected; a commented-out import is removed before the scan.
pub(super) fn java_imports_package(src: &str, package_prefix: &str) -> bool {
    let code = strip_comments(src);
    code.lines().any(|line| {
        let l = line.trim_start();
        let rest = match l
            .strip_prefix("import ")
            .or_else(|| l.strip_prefix("import\t"))
        {
            Some(r) => r.trim_start(),
            None => return false,
        };
        // `import static a.b.C.M;` — drop the `static ` qualifier.
        let rest = rest.strip_prefix("static ").unwrap_or(rest).trim_start();
        // Fully-qualified name up to the terminating `;`.
        let fqn = rest.split(';').next().unwrap_or(rest).trim();
        // review-7 item 1: match on a PACKAGE-COMPONENT boundary, not an arbitrary
        // textual prefix. A raw `starts_with(package_prefix)` accepts sibling
        // packages whose name merely extends the prefix's last component
        // (`org.springframework.web.clientish.Fake` for prefix `...web.client`,
        // `java.net.httpx.Foo` for prefix `java.net.http`), emitting a false
        // Layer-3 HTTP fact. A real type/wildcard/static import under the package
        // always continues with `.` after the package path, so require it.
        let mut dotted = String::with_capacity(package_prefix.len() + 1);
        dotted.push_str(package_prefix);
        dotted.push('.');
        fqn.starts_with(&dotted)
    })
}

/// Remove `//` line comments and `/* */` block comments from C-family source,
/// preserving string-literal contents and newlines (so line numbers and
/// line-anchored parsing stay valid). String delimiters `"`, `'`, `` ` `` are
/// respected, so a `//` inside a URL string is NOT stripped. Java-only helper —
/// the TS path uses `tokenize` directly.
fn strip_comments(src: &str) -> String {
    #[derive(PartialEq, Clone, Copy)]
    enum S {
        Code,
        Line,
        Block,
        Str(char),
    }
    let chars: Vec<char> = src.chars().collect();
    let n = chars.len();
    let mut out = String::with_capacity(src.len());
    let mut state = S::Code;
    let mut i = 0;
    while i < n {
        let c = chars[i];
        match state {
            S::Code => {
                if c == '/' && i + 1 < n && chars[i + 1] == '/' {
                    state = S::Line;
                    i += 2;
                    continue;
                }
                if c == '/' && i + 1 < n && chars[i + 1] == '*' {
                    state = S::Block;
                    i += 2;
                    continue;
                }
                if c == '"' || c == '\'' || c == '`' {
                    state = S::Str(c);
                }
                out.push(c);
            }
            S::Line => {
                if c == '\n' {
                    out.push('\n');
                    state = S::Code;
                }
            }
            S::Block => {
                if c == '*' && i + 1 < n && chars[i + 1] == '/' {
                    state = S::Code;
                    i += 2;
                    continue;
                }
                if c == '\n' {
                    out.push('\n'); // preserve line count
                }
            }
            S::Str(q) => {
                out.push(c);
                if c == '\\' && i + 1 < n {
                    out.push(chars[i + 1]);
                    i += 2;
                    continue;
                }
                if c == q {
                    state = S::Code;
                }
            }
        }
        i += 1;
    }
    out
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn ts_specifiers_from_real_import_forms() {
        let src = "import axios from 'axios';\n\
                   import { getApiClient } from '../config/api-client';\n\
                   import * as apigateway from 'aws-cdk-lib/aws-apigatewayv2';\n\
                   const fs = require('node:fs');";
        let specs = ts_import_specifiers(src);
        assert!(specs.contains(&"axios".to_string()), "{specs:?}");
        assert!(
            specs.contains(&"../config/api-client".to_string()),
            "{specs:?}"
        );
        assert!(
            specs.contains(&"aws-cdk-lib/aws-apigatewayv2".to_string()),
            "{specs:?}"
        );
        assert!(specs.contains(&"node:fs".to_string()), "{specs:?}");
    }

    #[test]
    fn ts_multiline_braced_import_specifier_is_captured() {
        // review recall guard: the `from 'axios'` sits on a later physical line.
        let src = "import {\n  a,\n  b,\n} from 'axios';";
        let specs = ts_import_specifiers(src);
        assert_eq!(specs, vec!["axios".to_string()]);
    }

    #[test]
    fn ts_comment_import_does_not_qualify() {
        // review-6 item 1: a commented-out import is NOT evidence.
        let line = "// import axios from 'axios';";
        assert!(ts_import_specifiers(line).is_empty(), "line comment");
        let block = "/* import axios from 'axios'; */";
        assert!(ts_import_specifiers(block).is_empty(), "block comment");
    }

    #[test]
    fn ts_string_literal_import_text_does_not_qualify() {
        // review-6 item 1: import/require text inside a string literal is NOT a
        // declaration and must not become evidence.
        let s1 = "const s = \"import axios from 'axios'\";";
        assert!(ts_import_specifiers(s1).is_empty(), "string import: {s1}");
        let s2 = "const s = \"require('axios')\";";
        assert!(ts_import_specifiers(s2).is_empty(), "string require: {s2}");
    }

    #[test]
    fn ts_from_as_identifier_is_not_a_specifier() {
        // `from` used as a variable / object key is not `import … from`.
        let src = "const from = 3; const o = { from: 'not-a-module' }; use(from);";
        assert!(
            ts_import_specifiers(src).is_empty(),
            "{:?}",
            ts_import_specifiers(src)
        );
    }

    #[test]
    fn java_import_declaration_matches_package_prefix() {
        let src = "package a;\nimport org.springframework.web.bind.annotation.RestController;\n\
                   import org.springframework.web.client.RestTemplate;\nclass C {}";
        assert!(java_imports_package(
            src,
            "org.springframework.web.bind.annotation"
        ));
        assert!(java_imports_package(src, "org.springframework.web.client"));
        // A sibling package that is NOT imported must not match (import scoping).
        assert!(!java_imports_package(
            src,
            "org.springframework.web.reactive.function.client"
        ));
    }

    #[test]
    fn java_wildcard_and_static_imports_match() {
        let wild = "import org.springframework.web.bind.annotation.*;";
        assert!(java_imports_package(
            wild,
            "org.springframework.web.bind.annotation"
        ));
        let stat = "import static org.springframework.web.bind.annotation.RequestMethod.GET;";
        assert!(java_imports_package(
            stat,
            "org.springframework.web.bind.annotation"
        ));
    }

    #[test]
    fn java_sibling_package_extending_prefix_component_does_not_qualify() {
        // review-7 item 1: a package-component boundary is required. Sibling
        // packages whose name merely EXTENDS the prefix's last component must not
        // match — otherwise a raw textual `starts_with` emits a false HTTP fact.
        let clientish = "import org.springframework.web.clientish.Fake;\nclass C {}";
        assert!(
            !java_imports_package(clientish, "org.springframework.web.client"),
            "clientish must not match web.client"
        );
        let httpx = "import java.net.httpx.Foo;\nclass C {}";
        assert!(
            !java_imports_package(httpx, "java.net.http"),
            "httpx must not match java.net.http"
        );
        // The genuine sibling type import under the real package still matches,
        // proving the boundary check did not over-reject.
        let real = "import java.net.http.HttpClient;\nclass C {}";
        assert!(
            java_imports_package(real, "java.net.http"),
            "real java.net.http import must still match"
        );
    }

    #[test]
    fn java_comment_and_string_import_text_does_not_qualify() {
        // review-6 item 1: neither a commented import nor a string literal that
        // contains import text is evidence.
        let commented = "// import org.springframework.web.client.RestTemplate;\nclass C {}";
        assert!(!java_imports_package(
            commented,
            "org.springframework.web.client"
        ));
        let block = "/* import org.springframework.web.client.RestTemplate; */\nclass C {}";
        assert!(!java_imports_package(
            block,
            "org.springframework.web.client"
        ));
        let string_lit =
            "class C { String s = \"import org.springframework.web.client.RestTemplate;\"; }";
        assert!(!java_imports_package(
            string_lit,
            "org.springframework.web.client"
        ));
    }

    #[test]
    fn strip_comments_preserves_url_double_slash_in_string() {
        let src = "String u = \"https://host/x\"; // trailing\n";
        let out = strip_comments(src);
        assert!(out.contains("https://host/x"), "url kept: {out}");
        assert!(!out.contains("trailing"), "comment removed: {out}");
    }
}
