//! Fountain (https://fountain.io) -> Typst markup for the script template.
//!
//! Screenplays and stage plays are written in Fountain: plain text where the
//! layout follows from the writing. A line in capitals after a blank line is a
//! character; the lines under it are their dialogue; `INT.` / `EXT.` start a
//! scene; `CUT TO:` is a transition. Supported:
//!   title page (`Title:`, `Credit:`, `Author:`, `Source:`, `Draft date:`, `Contact:`;
//!   the first line must be one of the known keys),
//!   scene headings (and forced `.HEADING`, scene numbers `#12#`), action
//!   (forced `!`), characters (forced `@`, extensions, dual `^`), parentheticals,
//!   dialogue, transitions (forced `>`), centred text (`> text <`), lyrics (`~`,
//!   also inside dialogue), indented action (leading spaces kept, a tab is four),
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

/// Title page keys a script may start with (any case). Only these open a title
/// page, so a first line like "Fade in:" stays part of the script.
pub const TITLE_KEYS: &[&str] = &[
    "title", "credit", "author", "authors", "source", "draft date", "date", "contact", "contact info",
    "copyright", "notes", "revision", "format", "watermark", "font",
];

/// The title page: `Key: value` lines at the very top, up to the first blank line.
fn title_page(lines: &[(usize, &str)]) -> (Vec<(String, Vec<String>)>, usize) {
    let mut fields: Vec<(String, Vec<String>)> = vec![];
    let mut i = 0;
    let first = lines.first().map(|(_, l)| *l).unwrap_or("");
    let looks_like_key = |l: &str| {
        let Some((k, _)) = l.split_once(':') else { return false };
        !k.is_empty() && k.len() <= 20 && !l.starts_with(' ') && !l.starts_with('\t') && k.chars().all(|c| c.is_alphanumeric() || c == ' ')
    };
    let known = first.split_once(':').is_some_and(|(k, _)| TITLE_KEYS.contains(&k.trim().to_lowercase().as_str()));
    if !looks_like_key(first) || !known {
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

/// Convert a Fountain script: the title page, then `#script((…))` with the
/// script's parts in order, so the template can keep count of the lines on a
/// page as it sets them. Every part starts with a `#metadata(line)<q>` marker,
/// like the Markdown converter, so pages map back to lines.
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
            "#titlepage(title: {}, credit: {}, author: {}, source: {}, draft: {}, contact: {})\n\n#script((\n",
            get(&["title"]),
            get(&["credit"]),
            get(&["author", "authors"]),
            get(&["source"]),
            get(&["draft date", "date"]),
            get(&["contact", "copyright"]),
        ));
    } else {
        out.push_str("#titlepage()\n\n#script((\n");
    }

    let blank = |i: usize| lines.get(i).is_none_or(|(_, l)| l.trim().is_empty());
    // The last dialogue block: where it starts and ends in `out`, and its cue's line.
    let mut last_dialogue: Option<(usize, usize, usize)> = None;
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
            out.push_str("newpage,\n\n");
            i += 1;
            continue;
        }
        if let Some(rest) = t.strip_prefix('#') {
            if kind == Kind::Play {
                let depth = 1 + rest.chars().take_while(|&c| c == '#').count();
                let title = rest.trim_start_matches('#').trim();
                out.push_str(&format!("section({depth}, [{mark}{}]),\n\n", inline(title)));
            }
            i += 1;
            continue;
        }
        if t.starts_with('=') {
            i += 1;
            continue;
        }
        if let Some(rest) = t.strip_prefix('!') {
            i = action(&lines, i, rest, &mark, &mut out, None);
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
            out.push_str(&format!("scene([{mark}{}], num: {num}),\n\n", inline(heading)));
            i += 1;
            continue;
        }
        if t.starts_with('>') && t.ends_with('<') && t.len() > 1 {
            out.push_str(&format!("centered[{mark}{}],\n\n", inline(t[1..t.len() - 1].trim())));
            i += 1;
            continue;
        }
        if let Some(rest) = t.strip_prefix('>') {
            out.push_str(&format!("transition[{mark}{}],\n\n", inline(rest.trim())));
            i += 1;
            continue;
        }
        if prev_blank && next_blank && is_transition(t) {
            out.push_str(&format!("transition[{mark}{}],\n\n", inline(t)));
            i += 1;
            continue;
        }
        if t.starts_with('~') {
            // Lyrics: consecutive `~` lines are one block, each line break kept.
            let mut parts = vec![];
            while let Some(rest) = lines.get(i).and_then(|(_, l)| l.trim().strip_prefix('~')) {
                parts.push(format!("#metadata({})<q>{}", line_no(i), inline(rest.trim())));
                i += 1;
            }
            out.push_str(&format!("lyric[{}],\n\n", parts.join(" \\\n")));
            continue;
        }
        let forced_cue = t.starts_with('@');
        if forced_cue || (prev_blank && !next_blank && is_cue(t)) {
            // `^` after a name: this speech and the one before it are spoken
            // at the same time and print side by side.
            match last_dialogue.filter(|&(_, end, _)| end == out.len() && t.ends_with('^')) {
                Some((start, _, first)) => {
                    out.truncate(start);
                    map.retain(|&(o, _)| o < start);
                    out.push_str("dual(\n");
                    map.push((out.len(), lines[first].0));
                    dialogue(&lines, first, &mut out);
                    map.push((out.len(), at));
                    i = dialogue(&lines, i, &mut out);
                    out.push_str("),\n\n");
                    last_dialogue = None;
                }
                None => {
                    let (start, cue_line) = (out.len(), i);
                    i = dialogue(&lines, i, &mut out);
                    out.push('\n');
                    last_dialogue = Some((start, out.len(), cue_line));
                }
            }
            continue;
        }
        i = action(&lines, i, t, &mark, &mut out, Some(src));
    }
    out.push_str("))\n");
    Converted { typst: out, map }
}

/// A character's cue and what they say, from the cue at line `i` to the next
/// blank line, as one `dialogue(line, [name], (…)),` call: the parentheticals
/// and paragraphs in order, each paragraph as its lines and each line as its
/// sentences, so the template can carry a speech across a page at the end of
/// a sentence. Returns the next line index.
fn dialogue(lines: &[(usize, &str)], mut i: usize, out: &mut String) -> usize {
    let m = |i: usize| format!("#metadata({})<q>", i + 1);
    let t = lines[i].1.trim();
    let name = t.trim_start_matches('@').trim_end_matches('^').trim();
    // A name already written as continuing isn't marked a second time.
    let upper = name.to_uppercase().replace('’', "'");
    let contd = ["(CONT'D)", "(CONT.)", "(CONTINUING)"].iter().any(|x| upper.contains(x));
    out.push_str(&format!(
        "dialogue({}, [{}],{}{} (\n",
        i + 1,
        inline(name),
        // A forced name prints as written ("McCLANE"); others in capitals.
        if t.starts_with('@') { " forced: true," } else { "" },
        if contd { " contd: true," } else { "" }
    ));
    i += 1;
    // Dialogue runs to the next blank line; a line of just spaces keeps it going.
    let mut speech: Vec<String> = vec![];
    let flush = |speech: &mut Vec<String>, out: &mut String| {
        if !speech.is_empty() {
            out.push_str(&format!("(say: ({},)),\n", speech.join(", ")));
            speech.clear();
        }
    };
    while i < lines.len() {
        let (_, l) = lines[i];
        if l.trim().is_empty() && l.len() < 2 {
            break;
        }
        let lt = l.trim();
        if lt.starts_with('(') && lt.ends_with(')') {
            flush(&mut speech, out);
            out.push_str(&format!("(paren: [{}{}]),\n", m(i), inline(lt)));
        } else {
            // A sung line (`~`) is in italics, where the dialogue goes.
            let mut text = match lt.strip_prefix('~') {
                Some(sung) => vec![format!("#emph[{}]", inline(sung.trim()))],
                None => sentences(&inline(lt)),
            };
            if text.is_empty() {
                text.push(String::new());
            }
            let parts: Vec<String> = text.iter().enumerate().map(|(k, x)| format!("[{}{x}]", if k == 0 { m(i) } else { String::new() })).collect();
            speech.push(format!("({},)", parts.join(", ")));
        }
        i += 1;
    }
    flush(&mut speech, out);
    out.push_str(")),\n");
    i
}

/// A line of dialogue, already Typst markup, as its sentences. A sentence
/// ends with `.`, `?`, `!` or `…` (and any closing quotes, brackets or dashes
/// after it) before a space, when what follows doesn't start in lower case
/// and the full stop isn't an initial's or a title's ("J. Smith", "Mr. Fox").
/// Nothing inside emphasis is split.
fn sentences(markup: &str) -> Vec<String> {
    const TITLES: &[&str] = &["mr", "mrs", "ms", "dr", "st", "prof", "sr", "jr", "vs", "mt", "lt", "col", "gen", "sgt", "capt", "rev"];
    let chars: Vec<(usize, char)> = markup.char_indices().collect();
    let mut out = vec![];
    let (mut start, mut depth, mut k) = (0, 0usize, 0);
    while k < chars.len() {
        let (at, ch) = chars[k];
        k += 1;
        match ch {
            '\\' => k += 1,
            '[' => depth += 1,
            ']' => depth = depth.saturating_sub(1),
            '.' | '?' | '!' | '…' => {
                // The rest of the stop ("?!", "...") and what closes after
                // it, the emphasis it is in too.
                let (mut end, mut open) = (k, depth);
                while let Some(&(_, c)) = chars.get(end) {
                    match c {
                        '.' | '?' | '!' | '…' | '"' | '\'' | '”' | '’' | '»' | ')' | '—' | '–' => end += 1,
                        ']' if open > 0 => {
                            open -= 1;
                            end += 1;
                        }
                        '\\' if matches!(chars.get(end + 1), Some((_, '-' | ']'))) => end += 2,
                        _ => break,
                    }
                }
                let next = chars.get(end + 1).map(|x| x.1);
                if open > 0 || chars.get(end).map(|x| x.1) != Some(' ') || next.is_none_or(char::is_lowercase) {
                    continue;
                }
                let word = markup[start..at].rsplit(|c: char| !c.is_alphabetic()).next().unwrap_or("");
                let one_stop = ch == '.' && end == k;
                if one_stop && (TITLES.contains(&word.to_lowercase().as_str()) || (word.chars().count() == 1 && word.chars().all(char::is_uppercase))) {
                    continue;
                }
                out.push(markup[start..chars[end].0].to_string());
                start = chars[end].0 + 1;
                k = end + 1;
                depth = 0;
            }
            _ => {}
        }
    }
    if start < markup.len() {
        out.push(markup[start..].to_string());
    }
    out
}

/// Action: consecutive lines, each line break kept, and the spaces or tabs a
/// line starts with. `src` is the file before notes were blanked out, so a
/// note at the start of a line doesn't indent it; `None` (forced `!` action)
/// leaves the first line unindented. Returns the next line index.
fn action(lines: &[(usize, &str)], mut i: usize, first: &str, mark: &str, out: &mut String, src: Option<&str>) -> usize {
    let mut parts = vec![format!("{mark}{}{}", src.map_or(String::new(), |s| indent(s, lines[i])), inline(first))];
    i += 1;
    // A `~` line is a lyric even here.
    while i < lines.len() && !lines[i].1.trim().is_empty() && !lines[i].1.trim().starts_with('~') {
        let lead = src.map_or(String::new(), |s| indent(s, lines[i]));
        parts.push(format!("#metadata({})<q>{lead}{}", i + 1, inline(lines[i].1.trim())));
        i += 1;
    }
    out.push_str(&format!("action[{}],\n\n", parts.join(" \\\n")));
    i
}

/// A line's indentation as non-breaking spaces (a tab is four): the leading
/// spaces and tabs of the original line, up to where its text starts.
fn indent(src: &str, (at, line): (usize, &str)) -> String {
    let text_at = line.len() - line.trim_start().len();
    let orig = src.get(at..at + text_at).unwrap_or("");
    let n: usize = orig.chars().take_while(|c| *c == ' ' || *c == '\t').map(|c| if c == '\t' { 4 } else { 1 }).sum();
    "~".repeat(n)
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
        assert!(out.contains("scene([#metadata(4)<q>EXT. FOREST \\- DAY], num: none)"), "{out}");
        assert!(out.contains("action[#metadata(6)<q>Nori runs. \\\n#metadata(7)<q>She stops.]"), "{out}");
        assert!(
            out.contains("dialogue(9, [NORI (V.O.)], (\n(paren: [#metadata(10)<q>(whispering)]),\n(say: (([#metadata(11)<q>Do you hear #emph[that]?],),)),\n)),\n\n"),
            "{out}"
        );
        assert!(out.contains("transition[#metadata(13)<q>CUT TO:]"), "{out}");
        assert!(out.contains("centered[#metadata(15)<q>THE END]"), "{out}");
    }

    #[test]
    fn forced_elements_notes_and_numbers() {
        let out = t(".flashback #3#\n\n@McCLANE\nYippee [[a note]] ki-yay.\n\n!LOUD NOISES\n\n===\n");
        assert!(out.contains("scene([#metadata(1)<q>flashback], num: [3])"), "{out}");
        assert!(out.contains("dialogue(3, [McCLANE], forced: true, (\n"), "{out}");
        assert!(out.contains("Yippee ki\\-yay."), "{out}");
        assert!(out.contains("action[#metadata(6)<q>LOUD NOISES]"), "{out}");
        assert!(out.contains("\n\nnewpage,\n\n"), "{out}");
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
        assert!(p.contains("section(1, [#metadata(1)<q>ACT ONE])"), "{p}");
        assert!(p.contains("section(2, [#metadata(3)<q>Scene 1])"), "{p}");
    }

    #[test]
    fn fade_in_is_not_a_title_page() {
        let out = t("FADE IN:\n\nINT. ROOM - DAY\n");
        assert!(out.starts_with("#titlepage()"), "{out}");
        assert!(out.contains("scene("), "{out}");
    }

    #[test]
    fn only_known_keys_open_a_title_page() {
        let out = t("Fade in:\n\nINT. ROOM - DAY\n");
        assert!(out.starts_with("#titlepage()"), "{out}");
        assert!(out.contains("action[#metadata(1)<q>Fade in:]"), "{out}");
        let out = t("TITLE: Big Fish\nAUTHOR: John August\n\nINT. ROOM - DAY\n");
        assert!(out.contains("#titlepage(title: [Big Fish], credit: none, author: [John August]"), "{out}");
        assert!(out.contains("scene([#metadata(4)<q>INT. ROOM \\- DAY]"), "{out}");
    }

    #[test]
    fn lyrics_in_blocks_and_in_dialogue() {
        let out = t("~Sing a song\n~of sixpence\n\nMARIA\nHello.\n~La la la\n");
        assert!(out.contains("lyric[#metadata(1)<q>Sing a song \\\n#metadata(2)<q>of sixpence]"), "{out}");
        assert!(out.contains("(say: (([#metadata(5)<q>Hello.],), ([#metadata(6)<q>#emph[La la la]],),)),"), "{out}");
        assert!(!out.contains('~'), "{out}");
        let out = t("\nShe sings:\n~La la\n");
        assert!(out.contains("action[#metadata(2)<q>She sings:]"), "{out}");
        assert!(out.contains("lyric[#metadata(3)<q>La la]"), "{out}");
    }

    #[test]
    fn forced_action_runs_on() {
        let out = t("!BOOM\nThe walls shake.\n");
        assert!(out.contains("action[#metadata(1)<q>BOOM \\\n#metadata(2)<q>The walls shake.]"), "{out}");
        assert!(!out.contains("#cue"), "{out}");
    }

    #[test]
    fn action_keeps_its_indentation() {
        let out = t("\nShe waits.\n    Indented.\n\tTabbed.\n[[note]] Not indented.\n");
        assert!(out.contains("#metadata(3)<q>~~~~Indented."), "{out}");
        assert!(out.contains("#metadata(4)<q>~~~~Tabbed."), "{out}");
        assert!(out.contains("#metadata(5)<q>Not indented."), "{out}");
    }

    #[test]
    fn lowercase_line_after_blank_is_action_not_cue() {
        let out = t("\nShe waits.\nNothing.\n");
        assert!(out.contains("action["), "{out}");
        assert!(!out.contains("#cue"));
    }

    #[test]
    fn dual_dialogue_side_by_side() {
        let c = convert("BRICK\nScrew retirement.\n\nSTEEL ^\n(at once)\nScrew retirement.\n\nThey grin.\n", Kind::Screenplay);
        let out = &c.typst;
        assert!(out.contains(
            "dual(\ndialogue(1, [BRICK], (\n(say: (([#metadata(2)<q>Screw retirement.],),)),\n)),\n\
             dialogue(4, [STEEL], (\n(paren: [#metadata(5)<q>(at once)]),\n(say: (([#metadata(6)<q>Screw retirement.],),)),\n)),\n),\n\n\
             action[#metadata(8)<q>They grin.],"
        ), "{out}");
        assert!(!out.contains('^'), "{out}");
        // The source map still runs in order and points at both cues.
        assert!(c.map.windows(2).all(|w| w[0].0 <= w[1].0), "{:?}", c.map);
        let at = |o: &str| c.map.iter().find(|(t, _)| *t == out.find(o).unwrap()).map(|m| m.1);
        assert_eq!(at("dialogue(1, "), Some(0));
        assert_eq!(at("dialogue(4, "), Some(25));
    }

    #[test]
    fn caret_without_dialogue_before_is_a_plain_cue() {
        let out = t("\nThey sit.\n\n@McCLANE ^\nHi.\n");
        assert!(!out.contains("dual("), "{out}");
        assert!(out.contains("dialogue(4, [McCLANE], forced: true, (\n(say: (([#metadata(5)<q>Hi.],),)),\n)),\n"), "{out}");
        // Only the speech right before pairs up; a third ^ starts no new pair.
        let out = t("A\nOne.\n\nB ^\nTwo.\n\nC ^\nThree.\n");
        assert_eq!(out.matches("dual(").count(), 1, "{out}");
        assert!(out.contains("dialogue(7, [C], (\n"), "{out}");
    }

    #[test]
    fn dialogue_is_one_call_the_template_can_carry_over_a_page() {
        // Name, parentheticals and speech travel together, in order: each
        // paragraph as its lines, each line as its sentences.
        let c = convert("\nNORI\nOne. Two?\nThree.\n(beat)\n~La la. La.\n\nShe waits.\n", Kind::Screenplay);
        let out = &c.typst;
        assert!(
            out.contains(
                "dialogue(2, [NORI], (\n(say: (([#metadata(3)<q>One.], [Two?],), ([#metadata(4)<q>Three.],),)),\n\
                 (paren: [#metadata(5)<q>(beat)]),\n(say: (([#metadata(6)<q>#emph[La la. La.]],),)),\n)),\n\naction[#metadata(8)<q>She waits.],\n\n))\n"
            ),
            "{out}"
        );
        // The name's own line marker is left to the template: it prints the
        // name again on the next page, and the marker must not come twice.
        assert!(!out.contains("#metadata(2)<q>"), "{out}");
        assert!(c.map.windows(2).all(|w| w[0].0 <= w[1].0), "{:?}", c.map);
        assert_eq!(c.map.iter().find(|(t, _)| *t == out.find("dialogue(2").unwrap()).map(|m| m.1), Some(1));
        // A name the writer already marked as continuing is not marked again.
        for name in ["NORI (CONT'D)", "NORI (cont’d)", "@Nori (V.O.) (CONT'D)", "NORI (continuing)"] {
            assert!(t(&format!("\n{name}\nHi.\n")).contains(" contd: true, (\n"), "{name}");
        }
        assert!(!t("\nNORI (V.O.)\nHi.\n").contains("contd"));
        // A name with nothing said, and a line of spaces inside a speech.
        assert!(t("\n@Nori\n\nShe waits.\n").contains("dialogue(2, [Nori], forced: true, (\n)),\n"));
        assert!(t("\nNORI\nOne.\n  \nTwo.\n").contains("([#metadata(4)<q>],)"));
    }

    #[test]
    fn speech_splits_into_sentences() {
        let s = |x: &str| sentences(&inline(x));
        assert_eq!(s("Do you hear that? I do. So we wait."), ["Do you hear that?", "I do.", "So we wait."]);
        // Closing quotes, brackets and dashes stay with their sentence.
        assert_eq!(s("\"Stop!\" (He did.) What--? Well..."), ["\"Stop!\"", "(He did.)", "What\\-\\-?", "Well..."]);
        assert_eq!(s("No?! Really… Yes."), ["No?!", "Really…", "Yes."]);
        // Not at a pause, an initial, a title or a number.
        assert_eq!(s("Well... maybe. Mr. Fox and J. Smith paid 3.50 each."), ["Well... maybe.", "Mr. Fox and J. Smith paid 3.50 each."]);
        // Emphasis is kept whole, and a sentence may end where it ends.
        assert_eq!(s("*Stop. Now.* Go **on.** Please."), ["#emph[Stop. Now.]", "Go #strong[on.]", "Please."]);
        assert_eq!(s("One."), ["One."]);
        assert!(s("").is_empty());
    }
}
