//! The web addresses in a commit message, which the commit panel shows as
//! links (spec `commit-details`, "Links in the message").

use std::ops::Range;

/// The byte ranges of the `http://` and `https://` addresses in `text`.
/// An address ends before whitespace; closing punctuation at its end and a
/// `)`, `]` or `}` that closes no bracket of the address are left out, as
/// long as either applies.
pub fn find(text: &str) -> Vec<Range<usize>> {
    let mut links = Vec::new();
    let mut from = 0;
    while let Some(offset) = text[from..].find("http") {
        let start = from + offset;
        let rest = &text[start..];
        let scheme = if rest.starts_with("https://") {
            "https://".len()
        } else if rest.starts_with("http://") {
            "http://".len()
        } else {
            from = start + "http".len();
            continue;
        };
        let end = rest
            .find(char::is_whitespace)
            .map_or(text.len(), |end| start + end);
        let end = start + trimmed(&text[start..end]).len();
        if end > start + scheme {
            links.push(start..end);
        }
        from = end.max(start + scheme);
    }
    links
}

/// Punctuation that closes a sentence or a quotation rather than an
/// address.
const CLOSING: &[char] = &['.', ',', ';', ':', '!', '?', '\'', '"', '>'];

/// Brackets an address may hold in pairs, as `(` and `)`.
const PAIRS: [(char, char); 3] = [('(', ')'), ('[', ']'), ('{', '}')];

/// `link` without closing punctuation and unbalanced closing brackets at
/// its end.
fn trimmed(mut link: &str) -> &str {
    'trim: loop {
        if let Some(rest) = link.strip_suffix(CLOSING) {
            link = rest;
            continue;
        }
        for (open, close) in PAIRS {
            if let Some(rest) = link.strip_suffix(close)
                && link.matches(open).count() < link.matches(close).count()
            {
                link = rest;
                continue 'trim;
            }
        }
        return link;
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn links(text: &str) -> Vec<&str> {
        find(text).into_iter().map(|range| &text[range]).collect()
    }

    #[test]
    fn a_full_stop_after_an_address_is_left_out() {
        assert_eq!(
            links("See https://example.com/issue/12. Thanks"),
            ["https://example.com/issue/12"]
        );
    }

    #[test]
    fn parentheses_inside_an_address_stay() {
        assert_eq!(
            links("(see https://en.wikipedia.org/wiki/Rust_(programming_language))"),
            ["https://en.wikipedia.org/wiki/Rust_(programming_language)"]
        );
    }

    #[test]
    fn punctuation_and_parentheses_are_left_out_in_any_order() {
        assert_eq!(
            links("(see https://example.com/page.)"),
            ["https://example.com/page"]
        );
        assert_eq!(
            links("(see https://example.com/page)."),
            ["https://example.com/page"]
        );
    }

    #[test]
    fn other_schemes_stay_text() {
        assert!(links("ftp://example.com/file and file:///etc/passwd").is_empty());
    }

    #[test]
    fn addresses_at_the_start_and_end_of_lines() {
        assert_eq!(
            links("https://a.example/x starts\nand ends with http://b.example\nhttps://c.example"),
            [
                "https://a.example/x",
                "http://b.example",
                "https://c.example"
            ]
        );
    }

    #[test]
    fn brackets_that_close_nothing_of_the_address_are_left_out() {
        assert_eq!(links("[https://a.example/x]"), ["https://a.example/x"]);
        assert_eq!(links("{https://a.example/x}."), ["https://a.example/x"]);
        assert_eq!(
            links("See http://[::1]:8080/a[1] and https://a.example/{id}"),
            ["http://[::1]:8080/a[1]", "https://a.example/{id}"]
        );
    }

    #[test]
    fn an_address_in_angle_brackets_leaves_them_out() {
        assert_eq!(
            links("Link: <https://a.example/x>"),
            ["https://a.example/x"]
        );
    }

    #[test]
    fn closing_punctuation_is_left_out() {
        assert_eq!(
            links(
                "https://a.example/1, https://a.example/2; \"https://a.example/3\" 'https://a.example/4'!"
            ),
            [
                "https://a.example/1",
                "https://a.example/2",
                "https://a.example/3",
                "https://a.example/4"
            ]
        );
    }

    #[test]
    fn a_scheme_with_nothing_after_it_is_no_link() {
        assert!(links("https:// and http://.").is_empty());
    }

    #[test]
    fn byte_ranges_count_bytes_of_text_before_them() {
        let text = "Grüße https://ä.example/ö";
        assert_eq!(find(text), vec![8..text.len()]);
    }
}
