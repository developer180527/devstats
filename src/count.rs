//! Line counting: splits a file into code / comment / blank lines.
//!
//! The scanner is a single pass per line that tracks block-comment and string
//! state. It is deliberately not a parser: constructs like raw strings, nested
//! block comments and heredocs are approximated. A line that contains any code
//! at all counts as code, even if it also carries a trailing comment — that is
//! the convention `cloc` and `tokei` use.

use crate::lang::Language;

#[derive(Debug, Default, Clone, Copy, PartialEq, Eq)]
pub struct Counts {
    pub code: u64,
    pub comments: u64,
    pub blank: u64,
}

impl Counts {
    pub fn add(&mut self, other: &Counts) {
        self.code += other.code;
        self.comments += other.comments;
        self.blank += other.blank;
    }
}

/// True if the buffer looks like binary content (a NUL byte in the first 8 KiB).
pub fn is_binary(bytes: &[u8]) -> bool {
    let window = &bytes[..bytes.len().min(8192)];
    window.contains(&0)
}

pub fn count(text: &str, lang: &Language) -> Counts {
    let mut counts = Counts::default();
    let mut in_block: Option<&'static str> = None;

    for raw in text.lines() {
        let line = raw.trim();

        if line.is_empty() {
            if in_block.is_some() {
                counts.comments += 1;
            } else {
                counts.blank += 1;
            }
            continue;
        }

        let mut has_code = false;
        let mut has_comment = false;
        let mut in_string: Option<u8> = None;
        let bytes = line.as_bytes();
        let mut i = 0usize;

        while i < bytes.len() {
            let rest = &line[i..];

            // Inside a block comment: look only for its terminator.
            if let Some(end) = in_block {
                has_comment = true;
                if rest.starts_with(end) {
                    in_block = None;
                    i += end.len();
                } else {
                    i += step(bytes, i);
                }
                continue;
            }

            // Inside a string literal: look only for its closing quote.
            if let Some(q) = in_string {
                has_code = true;
                if bytes[i] == b'\\' {
                    i += 1 + step(bytes, i + 1);
                    continue;
                }
                if bytes[i] == q {
                    in_string = None;
                }
                i += step(bytes, i);
                continue;
            }

            if lang.line.iter().any(|t| rest.starts_with(*t)) {
                has_comment = true;
                break; // remainder of the line is commentary
            }

            if let Some((open, close)) = lang.block.iter().find(|(o, _)| rest.starts_with(o)) {
                has_comment = true;
                in_block = Some(close);
                i += open.len();
                continue;
            }

            if lang.quotes.contains(&bytes[i]) {
                in_string = Some(bytes[i]);
                has_code = true;
                i += 1;
                continue;
            }

            if !bytes[i].is_ascii_whitespace() {
                has_code = true;
            }
            i += step(bytes, i);
        }

        if has_code {
            counts.code += 1;
        } else if has_comment {
            counts.comments += 1;
        } else {
            counts.blank += 1;
        }
    }

    counts
}

/// Width in bytes of the UTF-8 sequence starting at `i`, so indexing always
/// lands on a character boundary.
#[inline]
fn step(bytes: &[u8], i: usize) -> usize {
    if i >= bytes.len() {
        return 1;
    }
    match bytes[i] {
        0x00..=0x7F => 1,
        0xC0..=0xDF => 2,
        0xE0..=0xEF => 3,
        0xF0..=0xF7 => 4,
        _ => 1,
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::lang;
    use std::path::Path;

    fn lang_for(name: &str) -> &'static lang::Language {
        lang::detect(Path::new(name)).expect("known language")
    }

    #[test]
    fn splits_code_comments_and_blanks() {
        let src = "// header\n\nint main() {\n    return 0; // done\n}\n";
        let c = count(src, lang_for("a.c"));
        assert_eq!(c, Counts { code: 3, comments: 1, blank: 1 });
    }

    #[test]
    fn tracks_multi_line_block_comments() {
        let src = "/*\n * doc\n */\nint x = 1;\n";
        let c = count(src, lang_for("a.c"));
        assert_eq!(c, Counts { code: 1, comments: 3, blank: 0 });
    }

    #[test]
    fn code_after_a_closing_block_counts_as_code() {
        let src = "/* lead */ int x = 1;\n";
        let c = count(src, lang_for("a.c"));
        assert_eq!(c.code, 1);
        assert_eq!(c.comments, 0);
    }

    #[test]
    fn url_in_a_string_is_not_a_comment() {
        let src = "const char* u = \"http://example.com\";\n";
        let c = count(src, lang_for("a.c"));
        assert_eq!(c, Counts { code: 1, comments: 0, blank: 0 });
    }

    #[test]
    fn escaped_quote_does_not_end_the_string() {
        let src = "s = \"a\\\" // not a comment\";\n";
        let c = count(src, lang_for("a.c"));
        assert_eq!(c.code, 1);
    }

    #[test]
    fn hash_comments() {
        let src = "# note\nx = 1\n\n";
        let c = count(src, lang_for("a.py"));
        assert_eq!(c, Counts { code: 1, comments: 1, blank: 1 });
    }

    #[test]
    fn multibyte_content_does_not_panic() {
        let src = "let s = \"日本語テキスト\"; // コメント\nlet t = 1;\n";
        let c = count(src, lang_for("a.rs"));
        assert_eq!(c.code, 2);
    }

    #[test]
    fn detects_binary() {
        assert!(is_binary(&[0x7f, b'E', b'L', b'F', 0x00, 0x01]));
        assert!(!is_binary(b"plain text"));
    }
}
