//! Fountain (https://fountain.io) -> Typst markup for the script template.
//!
//! Screenplays and stage plays are written in Fountain: plain text where the
//! layout follows from the writing. A line in capitals after a blank line is a
//! character; the lines under it are their dialogue; `INT.` / `EXT.` start a
//! scene; `CUT TO:` is a transition. Supported:
//!   title page (`Title:`, `Credit:`, `Author:`, `Source:`, `Draft date:`, `Contact:`),
//!   scene headings (and forced `.HEADING`, scene numbers `#12#`), action
//!   (forced `!`), characters (forced `@`, extensions, dual `^`), parentheticals,
//!   dialogue, transitions (forced `>`), centred text (`> text <`), lyrics (`~`),
//!   page breaks (`===`), `*italic*`, `**bold**`, `***both***`, `_underline_`,
//!   and `\` escapes. Notes `[[ ]]`, boneyard `/* */` and synopses (`=`) don't print.
//!   Sections (`#`) don't print in a screenplay; in a stage play they are the
//!   act and scene headings.

use crate::md::{Converted, escape};

/// Which layout the script uses; decides only what sections do.
#[derive(Clone, Copy, PartialEq)]
pub enum Kind {
    Screenplay,
    Play,
}

/// Blank out notes and boneyard, keeping every newline and byte offset, so
/// lines and positions still match the file.
fn strip_hidden(src: &str) -> String {
    let mut out = String::with_capacity(src.len());
    let b = src.as_bytes();
    let mut i = 0;
    let mut hide: Option<&str> = None;
    while i < b.len() {
        match hide {
            None if src[i..].starts_with("/*") => hide = Some("*/"),
            None if src[i..].starts_with("[[") => hide = Some("]]"),
            Some(end) if src[i..].starts_with(end) => {
                out.push_str(&" ".repeat(end.len()));
                i += end.len();
                hide = None;
                continue;
            }
            _ => {}
        }
        let ch = src[i..].chars().next().unwrap();
        if hide.is_some() && ch != '\n' {
            // Keep byte length: one space per byte.
            out.push_str(&" ".repeat(ch.len_utf8()));
        } else {
            out.push(ch);
        }
        i += ch.len_utf8();
    }
    out
}

const PH_STAR: char = '\u{E000}';
const PH_UNDER: char = '\u{E001}';
const PH_SLASH: char = '\u{E002}';

/// Inline text: emphasis to Typst, everything else literal. Runs of spaces
/// left by hidden notes collapse to one.
fn inline(text: &str) -> String {
    let text = text.replace("\\*", &PH_STAR.to_string()).replace("\\_", &PH_UNDER.to_string()).replace("\\\\", &PH_SLASH.to_string());
    let mut collapsed = String::with_capacity(text.len());
    for ch in text.chars() {
        if ch == ' ' && collapsed.ends_with(' ') {
            continue;
        }
        collapsed.push(ch);
    }
    let out = emphasis(collapsed.trim());
    out.replace(PH_STAR, "\\*").replace(PH_UNDER, "\\_").replace(PH_SLASH, "\\\\")
}

fn emphasis(s: &str) -> String {
    // The earliest marker that has a closing partner wins; longer markers first.
    let mut best: Option<(usize, &str, usize)> = None;
    for mark in ["***", "**", "*", "_"] {
        let mut from = 0;
        while let Some(rel) = s[from..].find(mark) {
            let open = from + rel;
            let inner_start = open + mark.len();
            if let Some(close_rel) = s[inner_start..].find(mark).filter(|&c| c > 0) {
                if best.is_none_or(|(o, _, _)| open < o) {
                    best = Some((open, mark, inner_start + close_rel));
                }
                break;
            }
            from = open + mark.len();
        }
    }
    let Some((open, mark, close)) = best else { return script_escape(s) };
    let inner = emphasis(&s[open + mark.len()..close]);
    let wrapped = match mark {
        "***" => format!("#strong[#emph[{inner}]]"),
        "**" => format!("#strong[{inner}]"),
        "*" => format!("#emph[{inner}]"),
        _ => format!("#underline[{inner}]"),
    };
    format!("{}{}{}", script_escape(&s[..open]), wrapped, emphasis(&s[close + mark.len()..]))
}

/// Like Markdown escaping, and "--" stays two hyphens: in scripts it is
/// typed on purpose and must not become a dash.
fn script_escape(s: &str) -> String {
    escape(s, false).replace('-', "\\-")
}

fn is_scene(t: &str) -> bool {
    let u = t.to_uppercase();
    ["INT.", "EXT.", "EST.", "INT./EXT.", "INT/EXT", "I/E", "INT ", "EXT ", "EST ", "INT./EXT ", "I/E "]
        .iter()
        .any(|p| u.starts_with(p))
}

/// A character cue: capitals (before any "(V.O.)"), with at least one letter.
fn is_cue(t: &str) -> bool {
    let name = t.trim_end_matches('^').split('(').next().unwrap_or("").trim();
    name.chars().any(char::is_alphabetic) && !name.chars().any(char::is_lowercase)
}

fn is_transition(t: &str) -> bool {
    t.ends_with("TO:") && !t.chars().any(char::is_lowercase)
}

/// The title page: `Key: value` lines at the very top, up to the first blank line.
fn title_page(lines: &[(usize, &str)]) -> (Vec<(String, Vec<String>)>, usize) {
    let mut fields: Vec<(String, Vec<String>)> = vec![];
    let mut i = 0;
    let first = lines.first().map(|(_, l)| *l).unwrap_or("");
    let looks_like_key = |l: &str| {
        let Some((k, _)) = l.split_once(':') else { return false };
        !k.is_empty() && k.len() <= 20 && k.chars().any(char::is_lowercase) && !l.starts_with(' ') && !l.starts_with('\t') && k.chars().all(|c| c.is_alphanumeric() || c == ' ')
    };
    if !looks_like_key(first) || is_transition(first.trim()) {
        return (fields, 0);
    }
    while i < lines.len() {
        let l = lines[i].1;
        if l.trim().is_empty() {
            i += 1;
            break;
        }
        if looks_like_key(l) {
            let (k, v) = l.split_once(':').unwrap();
            let mut vals = vec![];
            if !v.trim().is_empty() {
                vals.push(v.trim().to_string());
            }
            fields.push((k.trim().to_lowercase(), vals));
        } else if let Some(last) = fields.last_mut() {
            last.1.push(l.trim().to_string());
        }
        i += 1;
    }
    (fields, i)
}

/// Convert a Fountain script. Every element starts with a `#metadata(line)<q>`
/// marker, like the Markdown converter, so pages map back to lines.
pub fn convert(src: &str, kind: Kind) -> Converted {
    let clean = strip_hidden(src);
    let mut lines: Vec<(usize, &str)> = vec![];
    let mut off = 0;
    for l in clean.split('\n') {
        lines.push((off, l.strip_suffix('\r').unwrap_or(l)));
        off += l.len() + 1;
    }
    let line_no = |i: usize| i + 1;

    let mut out = String::new();
    let mut map = vec![];
    let (fields, start) = title_page(&lines);
    if !fields.is_empty() {
        map.push((out.len(), 0));
        let get = |keys: &[&str]| -> String {
            let vals: Vec<String> = fields
                .iter()
                .filter(|(k, _)| keys.contains(&k.as_str()))
                .flat_map(|(_, v)| v.iter().map(|x| inline(x)))
                // `#""` keeps "1. Draft" from starting a numbered list in Typst.
                .map(|x| if x.starts_with(|c: char| c.is_ascii_digit() || c == '+') { format!("#\"\"{x}") } else { x })
                .collect();
            if vals.is_empty() { "none".into() } else { format!("[{}]", vals.join("#linebreak()")) }
        };
        out.push_str(&format!(
            "#titlepage(title: {}, credit: {}, author: {}, source: {}, draft: {}, contact: {})\n\n",
            get(&["title"]),
            get(&["credit"]),
            get(&["author", "authors"]),
            get(&["source"]),
            get(&["draft date", "date"]),
            get(&["contact", "copyright"]),
        ));
    } else {
        out.push_str("#titlepage()\n\n");
    }

    let blank = |i: usize| lines.get(i).is_none_or(|(_, l)| l.trim().is_empty());
    let mut i = start;
    while i < lines.len() {
        let (at, raw) = lines[i];
        let t = raw.trim();
        if t.is_empty() {
            i += 1;
            continue;
        }
        map.push((out.len(), at));
        let mark = format!("#metadata({})<q>", line_no(i));
        let prev_blank = i == start || blank(i - 1);
        let next_blank = blank(i + 1);

        if t.len() >= 3 && t.chars().all(|c| c == '=') {
            out.push_str("#pagebreak(weak: true)\n\n");
            i += 1;
            continue;
        }
        if let Some(rest) = t.strip_prefix('#') {
            if kind == Kind::Play {
                let depth = 1 + rest.chars().take_while(|&c| c == '#').count();
                let title = rest.trim_start_matches('#').trim();
                out.push_str(&format!("#section({depth}, [{mark}{}])\n\n", inline(title)));
            }
            i += 1;
            continue;
        }
        if t.starts_with('=') {
            i += 1;
            continue;
        }
        if let Some(rest) = t.strip_prefix('!') {
            i = action(&lines, i, rest, &mark, &mut out);
            continue;
        }
        let forced_scene = t.starts_with('.') && !t.starts_with("..");
        if forced_scene || (prev_blank && is_scene(t)) {
            let text = if forced_scene { &t[1..] } else { t };
            // A scene number at the end: "INT. HOUSE - DAY #12#".
            let (heading, num) = match text.trim_end().strip_suffix('#').and_then(|x| x.rsplit_once('#')) {
                Some((h, n)) if !n.contains(' ') => (h.trim(), format!("[{}]", script_escape(n))),
                _ => (text.trim(), "none".to_string()),
            };
            out.push_str(&format!("#scene([{mark}{}], num: {num})\n\n", inline(heading)));
            i += 1;
            continue;
        }
        if t.starts_with('>') && t.ends_with('<') && t.len() > 1 {
            out.push_str(&format!("#centered[{mark}{}]\n\n", inline(t[1..t.len() - 1].trim())));
            i += 1;
            continue;
        }
        if let Some(rest) = t.strip_prefix('>') {
            out.push_str(&format!("#transition[{mark}{}]\n\n", inline(rest.trim())));
            i += 1;
            continue;
        }
        if prev_blank && next_blank && is_transition(t) {
            out.push_str(&format!("#transition[{mark}{}]\n\n", inline(t)));
            i += 1;
            continue;
        }
        if let Some(rest) = t.strip_prefix('~') {
            out.push_str(&format!("#lyric[{mark}{}]\n\n", inline(rest.trim())));
            i += 1;
            continue;
        }
        let forced_cue = t.starts_with('@');
        if forced_cue || (prev_blank && !next_blank && is_cue(t)) {
            let name = t.trim_start_matches('@').trim_end_matches('^').trim();
            out.push_str(&format!("#cue[{mark}{}]\n", inline(name)));
            i += 1;
            // Dialogue runs to the next blank line; a line of just spaces keeps it going.
            let mut speech: Vec<String> = vec![];
            let flush = |speech: &mut Vec<String>, out: &mut String| {
                if !speech.is_empty() {
                    out.push_str(&format!("#speech[{}]\n", speech.join(" \\\n")));
                    speech.clear();
                }
            };
            while i < lines.len() {
                let (_, l) = lines[i];
                if l.trim().is_empty() && l.len() < 2 {
                    break;
                }
                let lt = l.trim();
                let m = format!("#metadata({})<q>", line_no(i));
                if lt.starts_with('(') && lt.ends_with(')') {
                    flush(&mut speech, &mut out);
                    out.push_str(&format!("#paren[{m}{}]\n", inline(lt)));
                } else {
                    speech.push(format!("{m}{}", inline(lt)));
                }
                i += 1;
            }
            flush(&mut speech, &mut out);
            out.push('\n');
            continue;
        }
        i = action(&lines, i, t, &mark, &mut out);
    }
    Converted { typst: out, map }
}

/// Action: consecutive lines, each line break kept. Returns the next line index.
fn action(lines: &[(usize, &str)], mut i: usize, first: &str, mark: &str, out: &mut String) -> usize {
    let mut parts = vec![format!("{mark}{}", inline(first))];
    i += 1;
    while i < lines.len() && !lines[i].1.trim().is_empty() {
        parts.push(format!("#metadata({})<q>{}", i + 1, inline(lines[i].1.trim())));
        i += 1;
    }
    out.push_str(&format!("#action[{}]\n\n", parts.join(" \\\n")));
    i
}

#[cfg(test)]
mod tests {
    use super::*;

    fn t(src: &str) -> String {
        convert(src, Kind::Screenplay).typst
    }

    #[test]
    fn title_page_and_elements() {
        let src = "Title: Big Fish\nAuthor: John August\n\nEXT. FOREST - DAY\n\nNori runs.\nShe stops.\n\nNORI (V.O.)\n(whispering)\nDo you hear *that*?\n\nCUT TO:\n\n> THE END <\n";
        let out = t(src);
        assert!(out.contains("#titlepage(title: [Big Fish], credit: none, author: [John August]"), "{out}");
        assert!(out.contains("#scene([#metadata(4)<q>EXT. FOREST \\- DAY], num: none)"), "{out}");
        assert!(out.contains("#action[#metadata(6)<q>Nori runs. \\\n#metadata(7)<q>She stops.]"), "{out}");
        assert!(out.contains("#cue[#metadata(9)<q>NORI (V.O.)]"), "{out}");
        assert!(out.contains("#paren[#metadata(10)<q>(whispering)]"), "{out}");
        assert!(out.contains("#speech[#metadata(11)<q>Do you hear #emph[that]?]"), "{out}");
        assert!(out.contains("#transition[#metadata(13)<q>CUT TO:]"), "{out}");
        assert!(out.contains("#centered[#metadata(15)<q>THE END]"), "{out}");
    }

    #[test]
    fn forced_elements_notes_and_numbers() {
        let out = t(".flashback #3#\n\n@McCLANE\nYippee [[a note]] ki-yay.\n\n!LOUD NOISES\n\n===\n");
        assert!(out.contains("#scene([#metadata(1)<q>flashback], num: [3])"), "{out}");
        assert!(out.contains("#cue[#metadata(3)<q>McCLANE]"), "{out}");
        assert!(out.contains("Yippee ki\\-yay."), "{out}");
        assert!(out.contains("#action[#metadata(6)<q>LOUD NOISES]"), "{out}");
        assert!(out.contains("#pagebreak(weak: true)"), "{out}");
        assert!(!out.contains("note"));
    }

    #[test]
    fn emphasis_and_escapes() {
        assert_eq!(inline("***both*** **b** *i* _u_ \\*not\\*"), "#strong[#emph[both]] #strong[b] #emph[i] #underline[u] \\*not\\*");
        assert_eq!(inline("a -- b"), "a \\-\\- b");
    }

    #[test]
    fn sections_print_only_in_plays() {
        let src = "# ACT ONE\n\n## Scene 1\n\nMARIA\nHello.\n";
        assert!(!t(src).contains("#section"));
        let p = convert(src, Kind::Play).typst;
        assert!(p.contains("#section(1, [#metadata(1)<q>ACT ONE])"), "{p}");
        assert!(p.contains("#section(2, [#metadata(3)<q>Scene 1])"), "{p}");
    }

    #[test]
    fn fade_in_is_not_a_title_page() {
        let out = t("FADE IN:\n\nINT. ROOM - DAY\n");
        assert!(out.starts_with("#titlepage()"), "{out}");
        assert!(out.contains("#scene("), "{out}");
    }

    #[test]
    fn lowercase_line_after_blank_is_action_not_cue() {
        let out = t("\nShe waits.\nNothing.\n");
        assert!(out.contains("#action["), "{out}");
        assert!(!out.contains("#cue"));
    }
}
