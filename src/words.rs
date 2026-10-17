//! Word counts, by the same rules as the editor's statistics sidebar
//! (`web/src/sidebars.ts`), so the history list and the sidebar agree: only
//! the words that print count, not `#`, dashes, image markup or notes.

use crate::md;

/// Words in a document: Markdown prose, or a Fountain script.
pub fn count(text: &str, script: bool) -> usize {
    if script { script_words(text) } else { prose_words(text) }
}

/// A word starts with a letter or digit and goes on through letters, digits,
/// apostrophes and hyphens (the sidebar's `[\p{L}\p{N}][\p{L}\p{N}'’-]*`).
fn words_in(s: &str) -> usize {
    let mut n = 0;
    let mut inside = false;
    for c in s.chars() {
        if c.is_alphanumeric() {
            n += !inside as usize;
            inside = true;
        } else if !(inside && matches!(c, '\'' | '’' | '-')) {
            inside = false;
        }
    }
    n
}

fn prose_words(text: &str) -> usize {
    let mut n = 0;
    for line in text.lines() {
        let raw = line.trim();
        let is_rule = raw.chars().all(|c| matches!(c, '-' | '*' | '_') || c.is_whitespace())
            && raw.chars().filter(|c| matches!(c, '-' | '*' | '_')).count() >= 3;
        let is_comment = raw.len() >= 7 && raw.starts_with("<!--") && raw.ends_with("-->");
        // Blank, an illustration, a scene break or a comment: nothing that prints as words.
        if raw.is_empty() || raw.starts_with("![") || is_rule || is_comment {
            continue;
        }
        let hashes = raw.chars().take_while(|&c| c == '#').count();
        let text = if (1..=6).contains(&hashes) && raw[hashes..].starts_with(char::is_whitespace) {
            raw[hashes..].trim_start().to_string()
        } else {
            let body = raw.strip_prefix('>').map_or(raw, str::trim_start);
            list_marker_end(body).map_or(body, |i| &body[i..]).to_string()
        };
        n += words_in(&plain_inline(&text));
    }
    n
}

/// Where the text after a `- `, `* `, `+ `, `1. ` or `1) ` list marker starts.
fn list_marker_end(s: &str) -> Option<usize> {
    let digits = s.bytes().take_while(u8::is_ascii_digit).count();
    let marker = match s.as_bytes().get(digits)? {
        b'-' | b'*' | b'+' if digits == 0 => 1,
        b'.' | b')' if digits > 0 => digits + 1,
        _ => return None,
    };
    let gap = s[marker..].len() - s[marker..].trim_start().len();
    (gap > 0).then_some(marker + gap)
}

/// Inline Markdown down to the words that print: images go, links keep their
/// text, comments and the tags Wit knows (`<u>`, `</u>`, `<br>`) go, any other
/// `<tag>` stays (it prints), `*_~`` ` marks go, escapes lose their backslash.
fn plain_inline(s: &str) -> String {
    let s = replace_links(s, true);
    let s = replace_links(&s, false);
    let s = strip_comments(&s);
    let s = strip_known_tags(&s);
    let mut out = String::with_capacity(s.len());
    let mut chars = s.chars();
    while let Some(c) = chars.next() {
        match c {
            '\\' => match chars.next() {
                Some(next) => out.push(next),
                None => out.push('\\'),
            },
            _ => out.push(c),
        }
    }
    out.retain(|c| !matches!(c, '*' | '_' | '~' | '`'));
    out
}

/// `![alt](url)` removed (`image`), or `[text](url)` replaced by its text.
fn replace_links(s: &str, image: bool) -> String {
    let open = if image { "![" } else { "[" };
    let mut out = String::with_capacity(s.len());
    let mut i = 0;
    while i < s.len() {
        if s[i..].starts_with(open) {
            let start = i + open.len();
            if let Some(close) = s[start..].find(']').map(|j| start + j) {
                if s[close + 1..].starts_with('(') {
                    if let Some(end) = s[close + 2..].find(')').map(|j| close + 2 + j) {
                        if !image {
                            out.push_str(&s[start..close]);
                        }
                        i = end + 1;
                        continue;
                    }
                }
            }
        }
        let c = s[i..].chars().next().unwrap();
        out.push(c);
        i += c.len_utf8();
    }
    out
}

/// `<!-- … -->` comments on the line removed (an unclosed one stays).
fn strip_comments(s: &str) -> String {
    let mut out = String::with_capacity(s.len());
    let mut rest = s;
    while let Some(i) = rest.find("<!--") {
        let Some(j) = rest[i + 4..].find("-->") else { break };
        out.push_str(&rest[..i]);
        rest = &rest[i + 4 + j + 3..];
    }
    out.push_str(rest);
    out
}

/// `<u>`, `</u>` and `<br>` (any case, `<br/>`, `<br />`) removed.
fn strip_known_tags(s: &str) -> String {
    let mut out = String::with_capacity(s.len());
    let mut i = 0;
    while i < s.len() {
        if s[i..].starts_with('<') {
            if let Some(j) = s[i..].find('>') {
                let tag = s[i..=i + j].to_ascii_lowercase();
                if tag == "<u>" || tag == "</u>" || md::is_br(&tag) {
                    i += j + 1;
                    continue;
                }
            }
        }
        let c = s[i..].chars().next().unwrap();
        out.push(c);
        i += c.len_utf8();
    }
    out
}

fn script_words(text: &str) -> usize {
    let text = strip_hidden(text);
    let lines: Vec<&str> = text.lines().collect();
    // The title page: "Title: …" up to the first blank line.
    let title = match lines.first() {
        Some(first) if is_title_key(first) => lines.iter().take_while(|l| !l.trim().is_empty()).count(),
        _ => 0,
    };
    let mut n = 0;
    for (i, line) in lines.iter().enumerate() {
        let raw = line.trim();
        let page_break = raw.len() >= 3 && raw.chars().all(|c| c == '=');
        // Not part of the script's words: title page, sections, synopses, page breaks.
        if raw.is_empty() || i < title || page_break || raw.starts_with('#') || raw.starts_with('=') {
            continue;
        }
        n += words_in(&plain_script(raw));
    }
    n
}

/// A title page key: a first line like `Title:` or `Draft date:`, whose key
/// has a lowercase letter (so `INT. HOUSE: DAY` is not one).
fn is_title_key(line: &str) -> bool {
    let Some((key, _)) = line.split_once(':') else { return false };
    let first = key.chars().next();
    first.is_some_and(|c| !c.is_whitespace()) && key.chars().count() <= 20 && key.chars().any(char::is_lowercase)
}

/// Fountain marks and element prefixes down to the printed words.
fn plain_script(raw: &str) -> String {
    let mut s = raw;
    // A scene number at the end: `INT. HOUSE - DAY #12#`.
    if let Some(body) = s.trim_end().strip_suffix('#') {
        if let Some(at) = body.rfind('#') {
            let num = &body[at + 1..];
            if !num.is_empty() && !num.contains(char::is_whitespace) {
                s = &body[..at];
            }
        }
    }
    let s = s.strip_prefix(['.', '@', '>', '!', '~']).unwrap_or(s);
    let s = s.strip_suffix('<').unwrap_or(s);
    s.chars().filter(|c| !matches!(c, '*' | '_')).collect()
}

/// Notes `[[ ]]` and boneyard `/* */`, even across lines, blanked out as the
/// script template does; the newlines stay so lines keep their place.
fn strip_hidden(src: &str) -> String {
    let mut out = String::with_capacity(src.len());
    let mut hide: Option<&str> = None;
    let mut i = 0;
    while i < src.len() {
        let rest = &src[i..];
        match hide {
            None if rest.starts_with("/*") => hide = Some("*/"),
            None if rest.starts_with("[[") => hide = Some("]]"),
            Some(end) if rest.starts_with(end) => {
                out.push_str(&" ".repeat(end.len()));
                i += end.len();
                hide = None;
                continue;
            }
            _ => {}
        }
        let c = rest.chars().next().unwrap();
        out.push(if hide.is_none() || c == '\n' { c } else { ' ' });
        i += c.len_utf8();
    }
    out
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn prose_counts_printed_words_only() {
        let md = "# Chapter 1\n\n— Bună, spuse **Nori**. Don't stop, well-known.\n\n\
                  ![a wolf](images/wolf.png \"50%\")\n\n---\n\n<!-- pagebreak -->\n\n\
                  > A [link](https://ex.com) here\n- one\n12. twelve\n\\# not a heading\n";
        // Chapter 1 | Bună spuse Nori Don't stop well-known | A link here | one | twelve | not a heading
        assert_eq!(count(md, false), 2 + 6 + 3 + 1 + 1 + 3);
        assert_eq!(count("####### seven\n", false), 1);
        // Only the tags Wit knows are markup; anything else prints, so counts.
        assert_eq!(count("a <b> c <u>d</u> <br/>e <!-- not this --> f\n", false), 6);
    }

    #[test]
    fn scripts_skip_title_notes_and_boneyard() {
        let f = "Title: Wolf\nAuthor: Me\n\n# ACT ONE\n\n= a synopsis\n\nEXT. FOREST - DAY #1#\n\n\
                 NORI\n(whispering)\nDo you [[a note]] hear?\n\n/* cut\nthis scene\n*/\n\n\
                 [[a long\nnote here]]\n\n===\n\n> CUT TO:\n";
        // EXT FOREST DAY | NORI | whispering | Do you hear | CUT TO
        assert_eq!(count(f, true), 3 + 1 + 1 + 3 + 2);
    }
}
