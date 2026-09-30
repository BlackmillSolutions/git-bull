//! Syntax highlighting of the versions of a file (design, decision 7).
//!
//! Each version is highlighted whole, so that a construct that spans lines,
//! such as a block comment, is right in a hunk that starts inside it. The
//! lines of the diff take their colours from the lines of their version.

use std::ops::Range;
use std::sync::OnceLock;

use gitbull_git::cancel::CancelToken;
use syntect::easy::HighlightLines;
use syntect::highlighting::FontStyle;
use syntect::parsing::{SyntaxReference, SyntaxSet};
use syntect::util::LinesWithEndings;
use two_face::theme::{EmbeddedLazyThemeSet, EmbeddedThemeName};

/// Versions larger than this are not highlighted.
pub const HIGHLIGHT_LIMIT: u64 = 512 * 1024;

/// The colours of highlighting, which follow the appearance of the window.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub enum HighlightTheme {
    Light,
    Dark,
}

/// A run of text in one style.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Span {
    /// Bytes of the line, without its line ending.
    pub range: Range<usize>,
    pub color: [u8; 3],
    pub bold: bool,
    pub italic: bool,
}

/// The spans of each line of a version, the first line first.
pub type HighlightedLines = Vec<Vec<Span>>;

/// Highlights `content` as the type of the file at `path`. Returns `None`
/// when the type is not known, or when `cancel` stops the work.
pub fn highlight(
    path: &str,
    content: &str,
    theme: HighlightTheme,
    cancel: &CancelToken,
) -> Option<HighlightedLines> {
    let syntaxes = syntaxes();
    let syntax = syntax_for(syntaxes, path, content)?;
    let theme = themes().get(match theme {
        HighlightTheme::Light => EmbeddedThemeName::OneHalfLight,
        HighlightTheme::Dark => EmbeddedThemeName::OneHalfDark,
    });
    let mut highlighter = HighlightLines::new(syntax, theme);
    let mut lines = Vec::new();
    for line in LinesWithEndings::from(content) {
        if cancel.is_cancelled() {
            return None;
        }
        let styled = highlighter.highlight_line(line, syntaxes).ok()?;
        let text_end = line.trim_end_matches(['\n', '\r']).len();
        let mut start = 0;
        let mut spans = Vec::new();
        for (style, piece) in styled {
            let end = (start + piece.len()).min(text_end);
            if end > start {
                spans.push(Span {
                    range: start..end,
                    color: [style.foreground.r, style.foreground.g, style.foreground.b],
                    bold: style.font_style.contains(FontStyle::BOLD),
                    italic: style.font_style.contains(FontStyle::ITALIC),
                });
            }
            start += piece.len();
        }
        lines.push(spans);
    }
    Some(lines)
}

/// The syntax of the file at `path`: by its name, such as `Makefile`, by
/// its extension, or by its first line, such as `#!/bin/sh`. Plain text
/// counts as not known.
fn syntax_for<'a>(
    syntaxes: &'a SyntaxSet,
    path: &str,
    content: &str,
) -> Option<&'a SyntaxReference> {
    let name = path.rsplit('/').next().unwrap_or(path);
    let extension = name.rsplit_once('.').map(|(_, extension)| extension);
    let first_line = content.lines().next().unwrap_or("");
    let syntax = syntaxes
        .find_syntax_by_extension(name)
        .or_else(|| extension.and_then(|extension| syntaxes.find_syntax_by_extension(extension)))
        .or_else(|| syntaxes.find_syntax_by_first_line(first_line))?;
    (syntax.name != syntaxes.find_syntax_plain_text().name).then_some(syntax)
}

/// The syntax definitions, loaded once; loading takes a moment, so it
/// happens on the first worker that highlights.
fn syntaxes() -> &'static SyntaxSet {
    static SYNTAXES: OnceLock<SyntaxSet> = OnceLock::new();
    SYNTAXES.get_or_init(two_face::syntax::extra_newlines)
}

fn themes() -> &'static EmbeddedLazyThemeSet {
    static THEMES: OnceLock<EmbeddedLazyThemeSet> = OnceLock::new();
    THEMES.get_or_init(two_face::theme::extra)
}

#[cfg(test)]
mod tests {
    use super::*;

    /// The colour of the first span of `line`.
    fn first_color(lines: &HighlightedLines, line: usize) -> [u8; 3] {
        lines[line].first().expect("a span").color
    }

    const RUST: &str = "fn before() {}\n/*\n inside 1\n inside 2\n inside 3\n inside 4\n inside 5\n inside 6\n inside 7\n*/\nfn after() {}\n";

    #[test]
    fn a_rust_file_is_highlighted_by_its_extension() {
        let lines = highlight(
            "src/lib.rs",
            RUST,
            HighlightTheme::Light,
            &CancelToken::new(),
        )
        .expect("Rust is known");
        assert_eq!(lines.len(), 11);
        // `fn` and the name after it differ in colour.
        let colors: std::collections::HashSet<_> = lines[0].iter().map(|span| span.color).collect();
        assert!(colors.len() > 1, "{:?}", lines[0]);
    }

    #[test]
    fn lines_deep_inside_a_block_comment_are_highlighted_as_comment() {
        let lines = highlight(
            "src/lib.rs",
            RUST,
            HighlightTheme::Dark,
            &CancelToken::new(),
        )
        .expect("Rust is known");
        // Line 9 is six lines below the start of the comment: a hunk with
        // three lines of context around it does not show where it began.
        // `/*` is a comment even when highlighted on its own.
        let comment = first_color(&lines, 1);
        assert_eq!(first_color(&lines, 8), comment);
        assert_ne!(first_color(&lines, 10), comment, "{:?}", lines[10]);
    }

    #[test]
    fn spans_cover_each_line_without_its_ending() {
        let content = "let a = 1;\r\nlet b = 2;\n";
        let lines = highlight("x.rs", content, HighlightTheme::Light, &CancelToken::new())
            .expect("Rust is known");
        for (line, text) in lines.iter().zip(["let a = 1;", "let b = 2;"]) {
            assert_eq!(line.first().unwrap().range.start, 0);
            assert_eq!(line.last().unwrap().range.end, text.len());
        }
    }

    #[test]
    fn a_file_of_an_unknown_type_is_not_highlighted() {
        assert_eq!(
            highlight(
                "notes.unknown-type",
                "just words\n",
                HighlightTheme::Light,
                &CancelToken::new()
            ),
            None
        );
    }

    #[test]
    fn a_file_without_extension_is_found_by_its_name_or_first_line() {
        let cancel = CancelToken::new();
        assert!(
            highlight(
                "Makefile",
                "all:\n\techo hi\n",
                HighlightTheme::Light,
                &cancel
            )
            .is_some()
        );
        assert!(
            highlight(
                "bin/run",
                "#!/bin/sh\necho hi\n",
                HighlightTheme::Light,
                &cancel
            )
            .is_some()
        );
    }

    #[test]
    fn light_and_dark_use_different_colours() {
        let cancel = CancelToken::new();
        let light = highlight("a.rs", RUST, HighlightTheme::Light, &cancel).unwrap();
        let dark = highlight("a.rs", RUST, HighlightTheme::Dark, &cancel).unwrap();
        assert_ne!(first_color(&light, 2), first_color(&dark, 2));
    }

    #[test]
    fn cancelled_work_gives_nothing() {
        let cancel = CancelToken::new();
        cancel.cancel();
        assert_eq!(
            highlight("a.rs", RUST, HighlightTheme::Light, &cancel),
            None
        );
    }
}
