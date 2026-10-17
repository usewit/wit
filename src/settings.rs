//! Page setup: trim presets, in-place book.yaml editing and the print checklist.
//!
//! book.yaml is shared with the older make_book.py, so edits keep comments and
//! unknown keys, and metric sizes are written as `trim_preset: custom` +
//! `trim_size_mm`, which make_book.py understands.

use anyhow::{Result, anyhow, bail};
use pulldown_cmark::{Event, Parser, Tag};
use serde::Serialize;
use serde_json::{Map, Value};
use std::path::Path;

use crate::book::Settings;

#[derive(Serialize, Clone, Copy)]
pub struct Preset {
    pub name: &'static str,
    pub label: &'static str,
    /// Heading the size is listed under in Page setup.
    pub group: &'static str,
    pub w_mm: f64,
    pub h_mm: f64,
    /// Amazon KDP prints this size, so the print checklist applies KDP's rules.
    pub kdp: bool,
    /// Written to book.yaml by name rather than in mm: make_book.py knows these.
    by_name: bool,
}

const IN: f64 = 25.4;
const US: &str = "US trade sizes (KDP, IngramSpark)";
const POCKET: &str = "Pocket and mass-market";
const UK: &str = "UK and Europe";
const ISO: &str = "ISO paper sizes";
const SQUARE: &str = "Square and landscape";

const fn inch(name: &'static str, label: &'static str, group: &'static str, w: f64, h: f64, kdp: bool, by_name: bool) -> Preset {
    Preset { name, label, group, w_mm: w * IN, h_mm: h * IN, kdp, by_name }
}
const fn mm(name: &'static str, label: &'static str, group: &'static str, w: f64, h: f64, kdp: bool) -> Preset {
    Preset { name, label, group, w_mm: w, h_mm: h, kdp, by_name: false }
}

/// Every trim size Page setup offers. KDP's whole paperback list is here, plus
/// IngramSpark's mass-market sizes and the common UK and European formats.
pub const PRESETS: &[Preset] = &[
    inch("5x8", "5 × 8 in · compact novel", US, 5.0, 8.0, true, true),
    inch("5.06x7.81", "5.06 × 7.81 in", US, 5.06, 7.81, true, false),
    inch("5.25x8", "5.25 × 8 in · novel", US, 5.25, 8.0, true, true),
    inch("5.5x8.5", "5.5 × 8.5 in · digest", US, 5.5, 8.5, true, true),
    inch("6x9", "6 × 9 in · trade paperback (most common)", US, 6.0, 9.0, true, true),
    inch("6.14x9.21", "6.14 × 9.21 in · royal", US, 6.14, 9.21, true, false),
    inch("6.69x9.61", "6.69 × 9.61 in · pinched crown", US, 6.69, 9.61, true, false),
    inch("7x10", "7 × 10 in · textbook", US, 7.0, 10.0, true, true),
    inch("7.44x9.69", "7.44 × 9.69 in · crown quarto", US, 7.44, 9.69, true, false),
    inch("7.5x9.25", "7.5 × 9.25 in", US, 7.5, 9.25, true, false),
    inch("8x10", "8 × 10 in · picture book, workbook", US, 8.0, 10.0, true, true),
    inch("8.5x11", "8.5 × 11 in · US Letter (scripts, workbooks)", US, 8.5, 11.0, true, true),
    inch("4.25x6.87", "4.25 × 6.87 in · US mass-market", POCKET, 4.25, 6.87, false, false),
    inch("4.37x7", "4.37 × 7 in · mass-market", POCKET, 4.37, 7.0, false, false),
    mm("uk-a-format", "A-format · 110 × 178 mm (UK mass-market)", POCKET, 110.0, 178.0, false),
    mm("a6", "A6 · 105 × 148 mm (pocket booklet)", POCKET, 105.0, 148.0, false),
    mm("uk-b-format", "B-format · 129 × 198 mm (UK paperback)", UK, 129.0, 198.0, false),
    mm("13x20", "13 × 20 cm · Romanian paperback", UK, 130.0, 200.0, false),
    mm("demy", "Demy · 138 × 216 mm (UK hardback)", UK, 138.0, 216.0, false),
    mm("royal", "Royal · 156 × 234 mm", UK, 156.0, 234.0, false),
    mm("a5", "A5 · 148 × 210 mm", ISO, 148.0, 210.0, false),
    mm("b6", "B6 · 125 × 176 mm", ISO, 125.0, 176.0, false),
    mm("b5", "B5 · 176 × 250 mm", ISO, 176.0, 250.0, false),
    mm("a4", "A4 · 210 × 297 mm (KDP too)", ISO, 210.0, 297.0, true),
    inch("8.5x8.5", "8.5 × 8.5 in · picture book", SQUARE, 8.5, 8.5, true, true),
    inch("8.25x8.25", "8.25 × 8.25 in", SQUARE, 8.25, 8.25, true, true),
    mm("square-210", "210 × 210 mm", SQUARE, 210.0, 210.0, false),
    mm("square-200", "200 × 200 mm", SQUARE, 200.0, 200.0, false),
    inch("8.25x6", "8.25 × 6 in · landscape", SQUARE, 8.25, 6.0, true, true),
];

pub fn preset(name: &str) -> Option<&'static Preset> {
    PRESETS.iter().find(|p| p.name.eq_ignore_ascii_case(name))
}

/// The preset matching a page size, or "custom".
pub fn preset_for(w_mm: f64, h_mm: f64) -> &'static str {
    PRESETS
        .iter()
        .find(|p| (p.w_mm - w_mm).abs() < 0.2 && (p.h_mm - h_mm).abs() < 0.2)
        .map(|p| p.name)
        .unwrap_or("custom")
}

// ---- style presets --------------------------------------------------------------

#[derive(Serialize)]
pub struct StylePreset {
    pub id: &'static str,
    pub name: &'static str,
    pub description: &'static str,
    /// The document types it suits: "book", "essay", "screenplay", "play".
    pub kinds: &'static [&'static str],
    /// Settings-panel values; the same keys the panel sends when saving.
    pub values: Value,
}

/// Whole-document looks. They set page size and typography, never the title,
/// author or the dark proofing switch. Edit freely: the panel picks them up.
/// Inside margins on KDP sizes are wide enough for up to 700 pages.
pub fn style_presets() -> Vec<StylePreset> {
    use serde_json::json;
    const BOOK: &[&str] = &["book"];
    const PROSE: &[&str] = &["book", "essay"];
    const ESSAY: &[&str] = &["essay"];
    const SCREEN: &[&str] = &["screenplay"];
    const PLAY: &[&str] = &["play"];
    let with = |base: &Value, over: Value| {
        let mut v = base.clone();
        v.as_object_mut().unwrap().extend(over.as_object().unwrap().clone());
        v
    };
    // Shared by the children's book presets. They leave the font alone.
    let kids = json!({
        "trim_preset": "a5", "bleed_mm": 3, "margin_mm": 18, "margin_inner_mm": 18,
        "body_font_size_pt": 15, "line_height_mult": 1.4, "para_gap_lines": 0.5,
        "justify": false, "hyphenate": false, "first_line_indent": false,
        "chapter_new_page": true, "chapter_recto": false,
        "chapter_title_font_size_pt": 22, "chapter_title_align": "center",
        "page_number_position": "bottom-outside", "page_number_font_size_pt": 11,
    });
    // A classic novel page: justified, hyphenated, indented, chapters on the right.
    let novel = json!({
        "trim_preset": "a5", "bleed_mm": 0, "margin_mm": 16, "margin_inner_mm": 19,
        "body_font_name": "EB Garamond", "body_font_size_pt": 11.5, "line_height_mult": 1.35, "para_gap_lines": 0,
        "justify": true, "hyphenate": true, "first_line_indent": true,
        "chapter_new_page": true, "chapter_recto": true,
        "chapter_title_font_size_pt": 18, "chapter_title_align": "center",
        "page_number_position": "bottom-outside", "page_number_font_size_pt": 9,
    });
    let essay = with(&novel, json!({
        "body_font_name": "Source Serif 4", "body_font_size_pt": 11, "line_height_mult": 1.4,
        "chapter_new_page": false, "chapter_recto": false, "chapter_title_font_size_pt": 15,
    }));
    // Standard manuscript format for agents and publishers: 12 pt Courier,
    // double-spaced, one-inch margins, ragged right.
    let manuscript = json!({
        "trim_preset": "8.5x11", "bleed_mm": 0, "margin_mm": 25.4, "margin_inner_mm": 25.4,
        "body_font_name": "Courier Prime", "body_font_size_pt": 12, "line_height_mult": 2, "para_gap_lines": 0,
        "justify": false, "hyphenate": false, "first_line_indent": true,
        "chapter_new_page": true, "chapter_recto": false,
        "chapter_title_font_size_pt": 12, "chapter_title_align": "center",
        "page_number_position": "bottom-center", "page_number_font_size_pt": 12,
    });
    let script = json!({ "trim_preset": "8.5x11", "body_font_name": "Courier Prime", "body_font_size_pt": 12 });
    let p = |id, name, description, kinds, values| StylePreset { id, name, description, kinds, values };
    vec![
        p("novel-6x9", "Novel · 6 × 9 in", "The most common trade paperback (KDP, IngramSpark): EB Garamond, justified and hyphenated, chapters on a right-hand page.", BOOK,
            with(&novel, json!({ "trim_preset": "6x9", "margin_mm": 16, "margin_inner_mm": 19, "body_font_size_pt": 11.5 }))),
        p("novel-5.5x8.5", "Novel · 5.5 × 8.5 in digest", "A smaller trade paperback, popular for genre fiction.", BOOK,
            with(&novel, json!({ "trim_preset": "5.5x8.5", "margin_mm": 15, "margin_inner_mm": 19, "body_font_size_pt": 11 }))),
        p("novel-5x8", "Novel · 5 × 8 in", "Compact novel in Crimson Pro, light in the hand.", BOOK,
            with(&novel, json!({ "trim_preset": "5x8", "margin_mm": 14, "margin_inner_mm": 19, "body_font_name": "Crimson Pro", "body_font_size_pt": 11.5 }))),
        p("mass-market", "Pocket paperback · 4.25 × 6.87 in", "US mass-market size (IngramSpark): small type, tight margins, many words to the page.", BOOK,
            with(&novel, json!({ "trim_preset": "4.25x6.87", "margin_mm": 12, "margin_inner_mm": 16, "body_font_name": "Crimson Pro", "body_font_size_pt": 10, "line_height_mult": 1.25, "chapter_title_font_size_pt": 15, "page_number_font_size_pt": 8 }))),
        p("uk-b-format", "UK paperback · B-format", "129 × 198 mm, the usual British paperback novel, set in Literata.", BOOK,
            with(&novel, json!({ "trim_preset": "uk-b-format", "margin_mm": 14, "margin_inner_mm": 18, "body_font_name": "Literata", "body_font_size_pt": 10.5 }))),
        p("hardback-demy", "Hardback · Demy", "138 × 216 mm, the British hardback size: roomy margins and EB Garamond at 12 pt.", BOOK,
            with(&novel, json!({ "trim_preset": "demy", "margin_mm": 18, "margin_inner_mm": 20, "body_font_size_pt": 12, "chapter_title_font_size_pt": 20 }))),
        p("novel-a5", "Novel · A5", "Classic book page on A5, for printers in Europe.", BOOK, novel.clone()),
        p("novel-13x20", "Novel · 13 × 20 cm", "The common Romanian paperback format, set like a classic novel.", BOOK,
            with(&novel, json!({ "trim_preset": "13x20", "margin_mm": 15, "margin_inner_mm": 17 }))),
        p("large-print-6x9", "Large print · 6 × 9 in", "16 pt Atkinson Hyperlegible, made for readers with low vision. KDP's large-print rules ask for 16 pt or more.", PROSE,
            with(&novel, json!({ "trim_preset": "6x9", "margin_mm": 16, "margin_inner_mm": 19, "body_font_name": "Atkinson Hyperlegible", "body_font_size_pt": 16, "line_height_mult": 1.4, "justify": false, "hyphenate": false, "para_gap_lines": 0.3, "chapter_title_font_size_pt": 24, "page_number_font_size_pt": 12 }))),
        p("nonfiction-6x9", "Non-fiction · 6 × 9 in", "Source Serif 4, block paragraphs with space between, chapters on any page.", PROSE,
            with(&novel, json!({ "trim_preset": "6x9", "body_font_name": "Source Serif 4", "body_font_size_pt": 11, "line_height_mult": 1.4, "first_line_indent": false, "para_gap_lines": 0.6, "chapter_recto": false }))),
        p("workbook-8.5x11", "Workbook · 8.5 × 11 in", "US Letter pages with wide margins, ragged right, for workbooks and manuals.", PROSE,
            with(&novel, json!({ "trim_preset": "8.5x11", "margin_mm": 19, "margin_inner_mm": 22, "body_font_name": "Source Serif 4", "body_font_size_pt": 12, "line_height_mult": 1.45, "justify": false, "hyphenate": false, "first_line_indent": false, "para_gap_lines": 0.6, "chapter_recto": false, "chapter_title_font_size_pt": 22 }))),
        p("manuscript-letter", "Manuscript · US Letter", "Standard manuscript format for agents and publishers: 12 pt Courier, double-spaced, one-inch margins.", PROSE, manuscript.clone()),
        p("manuscript-a4", "Manuscript · A4", "Standard manuscript format on A4.", PROSE, with(&manuscript, json!({ "trim_preset": "a4" }))),
        p("essay-a5", "Essay · A5", "Source Serif 4, justified, sections flowing on without new pages.", ESSAY, essay.clone()),
        p("essay-6x9", "Essay · 6 × 9 in", "A trade-paperback essay or collection.", ESSAY, with(&essay, json!({ "trim_preset": "6x9" }))),
        p("essay-letter", "Essay · US Letter", "For printing at home or handing in: wide margins, 12 pt.", ESSAY,
            with(&essay, json!({ "trim_preset": "8.5x11", "margin_mm": 25.4, "margin_inner_mm": 25.4, "body_font_size_pt": 12, "line_height_mult": 1.5 }))),
        p("essay-a4", "Essay · A4", "For printing at home or handing in: wide margins, 12 pt.", ESSAY,
            with(&essay, json!({ "trim_preset": "a4", "margin_mm": 25, "margin_inner_mm": 25, "body_font_size_pt": 12, "line_height_mult": 1.5 }))),
        p("bedtime-a5", "Bedtime story · A5", "Large, calm text with ragged right edges, comfortable for tired parents and children aged 6–9 reading at night.", BOOK, kids.clone()),
        p("booklet-a5", "Booklet · A5 stapled", "A4 sheets folded in half and stapled. Wider spine margin for the fold; the page count must be a multiple of 4.", BOOK,
            with(&kids, json!({ "margin_mm": 20, "margin_inner_mm": 22 }))),
        p("pocket-a6", "Pocket booklet · A6 stapled", "A5 sheets folded in half: small and light to carry. The page count must be a multiple of 4.", BOOK,
            with(&kids, json!({
                "trim_preset": "a6", "margin_mm": 12, "margin_inner_mm": 14,
                "body_font_size_pt": 12, "line_height_mult": 1.35,
                "chapter_title_font_size_pt": 17, "page_number_font_size_pt": 9,
            }))),
        p("early-reader-a5", "Early reader · A5 large print", "Extra-large type and generous spacing for children starting to read on their own.", BOOK,
            with(&kids, json!({
                "body_font_size_pt": 17, "line_height_mult": 1.5, "para_gap_lines": 0.6,
                "chapter_title_font_size_pt": 24,
            }))),
        p("picture-square-210", "Picture book · square 210", "210 × 210 mm square pages with big type, made for full-page illustrations.", BOOK,
            with(&kids, json!({
                "trim_preset": "square-210", "margin_mm": 15, "margin_inner_mm": 15,
                "body_font_size_pt": 18, "line_height_mult": 1.5, "chapter_title_font_size_pt": 26,
            }))),
        p("picture-8.5", "Picture book · 8.5 × 8.5 in", "KDP's square picture-book size with bleed, in Andika, the typeface made for children learning to read.", BOOK,
            with(&kids, json!({
                "trim_preset": "8.5x8.5", "bleed_mm": 3.2, "margin_mm": 16, "margin_inner_mm": 16,
                "body_font_name": "Andika", "body_font_size_pt": 18, "line_height_mult": 1.5, "chapter_title_font_size_pt": 26,
            }))),
        p("picture-8x10", "Picture book · 8 × 10 in", "Portrait picture book with bleed, in Andika.", BOOK,
            with(&kids, json!({
                "trim_preset": "8x10", "bleed_mm": 3.2, "margin_mm": 16, "margin_inner_mm": 16,
                "body_font_name": "Andika", "body_font_size_pt": 18, "line_height_mult": 1.5, "chapter_title_font_size_pt": 26,
            }))),
        p("screenplay-letter", "Screenplay · US Letter", "Industry format: Courier Prime 12 pt, 1.5 in left margin, about a minute of screen time a page.", SCREEN, script.clone()),
        p("screenplay-a4", "Screenplay · A4", "The same layout on A4, for Europe.", SCREEN, with(&script, json!({ "trim_preset": "a4" }))),
        p("play-letter", "Stage play · US Letter", "US manuscript format: names centred, dialogue across the page, stage directions indented in italics.", PLAY, script.clone()),
        p("play-a4", "Stage play · A4", "The same layout on A4.", PLAY, with(&script, json!({ "trim_preset": "a4" }))),
    ]
}

// ---- book.yaml editing --------------------------------------------------------

enum Kind {
    Num(f64, f64),
    Bool,
    Choice(&'static [&'static str]),
    /// A font family name.
    Font,
    /// A style preset's id, or "" for none.
    Style,
}

/// Languages a document can be written in, as Typst (ISO 639-1) codes. They
/// decide hyphenation and smart quotes, and the language of exported files.
pub const LANGUAGES: &[&str] = &["en", "ro", "de", "es", "fr", "it", "nl", "pt"];

/// Keys the settings panel may change, with their allowed values.
const EDITABLE: &[(&str, Kind)] = &[
    ("lang", Kind::Choice(LANGUAGES)),
    ("bleed_mm", Kind::Num(0.0, 10.0)),
    ("margin_mm", Kind::Num(5.0, 40.0)),
    ("margin_inner_mm", Kind::Num(5.0, 40.0)),
    ("body_font_size_pt", Kind::Num(8.0, 30.0)),
    ("line_height_mult", Kind::Num(1.0, 2.5)),
    ("para_gap_lines", Kind::Num(0.0, 2.0)),
    ("justify", Kind::Bool),
    ("hyphenate", Kind::Bool),
    ("first_line_indent", Kind::Bool),
    ("chapter_new_page", Kind::Bool),
    ("chapter_recto", Kind::Bool),
    ("chapter_title_font_size_pt", Kind::Num(10.0, 48.0)),
    ("chapter_title_align", Kind::Choice(&["center", "left"])),
    ("include_title_page", Kind::Bool),
    ("page_number_position", Kind::Choice(&["bottom-outside", "bottom-center"])),
    ("page_number_font_size_pt", Kind::Num(6.0, 16.0)),
    ("dark_background", Kind::Bool),
    ("body_font_name", Kind::Font),
    ("scene_heading_bold", Kind::Bool),
    ("scene_numbers", Kind::Bool),
    ("style_preset", Kind::Style),
];

fn fmt_num(n: f64) -> String {
    let s = format!("{:.3}", n);
    s.trim_end_matches('0').trim_end_matches('.').to_string()
}

/// Turn a JSON object from the settings panel into validated YAML `(key, value)` lines.
pub fn updates_from_json(map: &Map<String, Value>) -> Result<Vec<(String, String)>> {
    let mut out = vec![];
    for (key, kind) in EDITABLE {
        let Some(v) = map.get(*key) else { continue };
        let val = match kind {
            Kind::Num(lo, hi) => {
                let n = v.as_f64().ok_or_else(|| anyhow!("{key} must be a number"))?;
                if !(*lo..=*hi).contains(&n) {
                    bail!("{key} must be between {lo} and {hi}");
                }
                fmt_num(n)
            }
            Kind::Bool => v.as_bool().ok_or_else(|| anyhow!("{key} must be true or false"))?.to_string(),
            Kind::Choice(opts) => {
                let s = v.as_str().unwrap_or_default();
                if !opts.contains(&s) {
                    bail!("{key} must be one of {}", opts.join(", "));
                }
                format!("\"{s}\"")
            }
            Kind::Style => {
                let s = v.as_str().unwrap_or_default();
                if !s.is_empty() && !style_presets().iter().any(|p| p.id == s) {
                    bail!("{key} must be one of the Page setup presets");
                }
                format!("\"{s}\"")
            }
            Kind::Font => {
                let s = v.as_str().unwrap_or_default().trim();
                if s.is_empty() || s.len() > 80 || s.chars().any(|c| c.is_control() || c == '"' || c == '\\') {
                    bail!("{key} must be a font name");
                }
                format!("\"{s}\"")
            }
        };
        // make_book.py reads paragraph spacing as 1 + the gap in lines.
        if *key == "para_gap_lines" {
            out.push(("para_spacing_mult".into(), fmt_num(1.0 + v.as_f64().unwrap_or(0.0))));
        }
        out.push((key.to_string(), val));
    }

    if let Some(name) = map.get("trim_preset").and_then(|v| v.as_str()) {
        let (preset_val, size) = if name == "custom" {
            let w = map.get("trim_w_mm").and_then(|v| v.as_f64());
            let h = map.get("trim_h_mm").and_then(|v| v.as_f64());
            match (w, h) {
                (Some(w), Some(h)) if (50.0..=500.0).contains(&w) && (50.0..=500.0).contains(&h) => {
                    ("custom".to_string(), Some((w, h)))
                }
                _ => bail!("custom size needs width and height between 50 and 500 mm"),
            }
        } else {
            let p = preset(name).ok_or_else(|| anyhow!("unknown size '{name}'"))?;
            if p.by_name { (p.name.to_string(), None) } else { ("custom".to_string(), Some((p.w_mm, p.h_mm))) }
        };
        out.push(("trim_preset".into(), preset_val));
        if let Some((w, h)) = size {
            out.push(("trim_size_mm".into(), format!("\"{}x{}\"", fmt_num(w), fmt_num(h))));
        }
    }
    Ok(out)
}

/// Replace top-level `key: value` lines in place, keeping any trailing comment;
/// keys not present are appended at the end.
pub fn apply_yaml_updates(text: &str, updates: &[(String, String)]) -> String {
    let mut lines: Vec<String> = text.lines().map(str::to_string).collect();
    let mut missing = vec![];
    for (key, val) in updates {
        let prefix = format!("{key}:");
        let found = lines.iter_mut().find(|l| l.starts_with(&prefix));
        match found {
            Some(line) => {
                let rest = line[prefix.len()..].trim_end();
                // Keep the original gap before the comment so columns stay aligned.
                let (value_part, comment) = match comment_of(rest) {
                    Some(c) => (&rest[..rest.len() - c.len()], c),
                    None => (rest, ""),
                };
                if value_part.trim() == val {
                    continue;
                }
                let gap = &value_part[value_part.trim_end().len()..];
                let gap = if comment.is_empty() { "" } else if gap.is_empty() { "  " } else { gap };
                *line = format!("{key}: {val}{gap}{comment}");
            }
            None => missing.push(format!("{key}: {val}")),
        }
    }
    if !missing.is_empty() {
        // Files edited before the rename say "Added by Quire"; keep using that block.
        if !lines.iter().any(|l| l == "# Added by Wit" || l == "# Added by Quire") {
            if lines.last().is_some_and(|l| !l.trim().is_empty()) {
                lines.push(String::new());
            }
            lines.push("# Added by Wit".into());
        }
        lines.extend(missing);
    }
    let mut out = lines.join("\n");
    out.push('\n');
    out
}

/// The `# comment` part of a YAML scalar line, ignoring `#` inside quotes.
fn comment_of(rest: &str) -> Option<&str> {
    let mut quote: Option<char> = None;
    let mut prev_space = true;
    for (i, c) in rest.char_indices() {
        match (quote, c) {
            (Some(q), _) if c == q => quote = None,
            (Some(_), _) => {}
            (None, '"' | '\'') => quote = Some(c),
            (None, '#') if prev_space => return Some(rest[i..].trim_end()),
            _ => {}
        }
        prev_space = c.is_whitespace();
    }
    None
}

// ---- print checklist ------------------------------------------------------------

#[derive(Serialize)]
pub struct ImageCheck {
    pub path: String,
    pub placement: String,
    pub px: Option<(usize, usize)>,
    pub dpi: Option<f64>,
    pub ok: bool,
    pub note: String,
}

/// Effective resolution of every illustration at the size it is placed.
pub fn image_checks(book_dir: &Path, source: &str, s: &Settings) -> Vec<ImageCheck> {
    let mut out = vec![];
    for ev in Parser::new(source) {
        let Event::Start(Tag::Image { dest_url, title, .. }) = ev else { continue };
        let path = dest_url.trim_start_matches("./").trim_start_matches('/').to_string();
        let title = title.trim().to_string();
        let full = title.eq_ignore_ascii_case("full");
        let pct = title.strip_suffix('%').and_then(|n| n.trim().parse::<f64>().ok()).unwrap_or(70.0);
        let placement = if full { "full page".to_string() } else { format!("{}% width", fmt_num(pct)) };
        let px = imagesize::size(book_dir.join(&path)).ok().map(|d| (d.width, d.height));
        let dpi = px.map(|(w, h)| {
            if full {
                // "cover": the image fills trim + bleed on both axes.
                let pw = (s.trim_w_mm + 2.0 * s.bleed_mm) / 25.4;
                let ph = (s.trim_h_mm + 2.0 * s.bleed_mm) / 25.4;
                (w as f64 / pw).min(h as f64 / ph)
            } else {
                let text_w = (s.trim_w_mm - 2.0 * s.margin_mm) / 25.4;
                w as f64 / (text_w * pct.clamp(10.0, 100.0) / 100.0)
            }
        });
        let (ok, note) = match dpi {
            None => (false, "File not found".to_string()),
            Some(d) if d < s.min_dpi => (false, format!("{:.0} dpi, needs {:.0}", d, s.min_dpi)),
            Some(d) => (true, format!("{:.0} dpi", d)),
        };
        out.push(ImageCheck { path, placement, px, dpi, ok, note });
    }
    out
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn edits_keep_comments_and_unknown_keys() {
        let y = "# top\nbleed_mm: 3               # confirm with printer\nfoo: bar\njustify: true\n";
        let out = apply_yaml_updates(
            y,
            &[("bleed_mm".into(), "5".into()), ("justify".into(), "false".into()), ("hyphenate".into(), "true".into())],
        );
        assert_eq!(
            out,
            "# top\nbleed_mm: 5               # confirm with printer\nfoo: bar\njustify: false\n\n# Added by Wit\nhyphenate: true\n"
        );
    }

    #[test]
    fn metric_presets_write_custom_size() {
        let m = serde_json::json!({ "trim_preset": "a5" });
        let u = updates_from_json(m.as_object().unwrap()).unwrap();
        assert_eq!(u, vec![("trim_preset".into(), "custom".into()), ("trim_size_mm".into(), "\"148x210\"".into())]);
        let m = serde_json::json!({ "trim_preset": "6x9", "bleed_mm": 99 });
        assert!(updates_from_json(m.as_object().unwrap()).is_err());
    }

    #[test]
    fn style_presets_are_valid_settings() {
        for p in style_presets() {
            let u = updates_from_json(p.values.as_object().unwrap()).unwrap();
            let keys: Vec<_> = u.iter().map(|(k, _)| k.as_str()).collect();
            assert!(keys.contains(&"trim_preset"), "{}", p.id);
            let script = p.kinds.iter().all(|k| *k == "screenplay" || *k == "play");
            assert!(script || keys.contains(&"para_spacing_mult"), "{}", p.id);
            if let Some(name) = p.values.get("trim_preset").and_then(|v| v.as_str()) {
                assert!(preset(name).is_some(), "{}: unknown size {name}", p.id);
            }
        }
    }

    #[test]
    fn language_is_a_choice() {
        let m = serde_json::json!({ "lang": "de" });
        assert_eq!(updates_from_json(m.as_object().unwrap()).unwrap(), vec![("lang".into(), "\"de\"".into())]);
        let m = serde_json::json!({ "lang": "xx" });
        assert!(updates_from_json(m.as_object().unwrap()).is_err());
    }

    #[test]
    fn hash_in_quotes_is_not_a_comment() {
        assert_eq!(comment_of(" \"a#b\"  # c"), Some("# c"));
        assert_eq!(comment_of(" \"a#b\""), None);
    }
}
