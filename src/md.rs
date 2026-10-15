//! Markdown -> Typst markup for the book template.
//!
//! Supported: `# chapter`, `## section`, paragraphs, **bold**, *italic*,
//! `<u>underline</u>`, `[links](https://…)`, `---` scene breaks, `> quotes`,
//! bulleted and numbered lists, hard breaks, and images:
//!   `![alt](images/x.png)`          spot illustration, 70% width
//!   `![alt](images/x.png "50%")`    spot illustration at that width
//!   `![alt](images/x.png "full")`   full-bleed illustration page
//! plus `<!-- pagebreak -->` on its own line.

use pulldown_cmark::{Event, HeadingLevel, Options, Parser, Tag, TagEnd};

/// Escape text so Typst treats it literally. Quotes are left alone so smart
/// quotes still work, and `--` inside text still becomes a dash.
pub fn escape(text: &str, at_line_start: bool) -> String {
    let mut out = String::with_capacity(text.len() + 8);
    for (i, c) in text.chars().enumerate() {
        let first = at_line_start && i == 0;
        let special = matches!(
            c,
            '\\' | '#' | '*' | '_' | '`' | '$' | '<' | '>' | '@' | '[' | ']' | '~' | '/' | '='
        ) || (first && (matches!(c, '-' | '+') || c.is_ascii_digit()));
        if special {
            out.push('\\');
        }
        out.push(c);
    }
    out
}

fn image_call(path: &str, title: &str) -> String {
    // Paths are root-relative inside the book directory.
    let p = format!("/{}", path.trim_start_matches("./").trim_start_matches('/'));
    let p = p.replace('\\', "\\\\").replace('"', "\\\"");
    let t = title.trim();
    if t.eq_ignore_ascii_case("full") {
        format!("#fullbleed(\"{p}\")\n\n")
    } else if let Some(pct) = t.strip_suffix('%').and_then(|n| n.trim().parse::<f64>().ok()) {
        format!("#spot(\"{p}\", width: {}%)\n\n", pct.clamp(10.0, 100.0))
    } else {
        format!("#spot(\"{p}\")\n\n")
    }
}

/// Typst markup plus a map from output byte offsets to Markdown byte offsets,
/// used to point compile errors at the right Markdown line.
pub struct Converted {
    pub typst: String,
    /// `(typst_offset, markdown_offset)` pairs, ascending.
    pub map: Vec<(usize, usize)>,
}

impl Converted {
    /// 1-based Markdown line for an offset into the Typst output.
    pub fn md_line(&self, md: &str, typst_offset: usize) -> usize {
        let i = self.map.partition_point(|(t, _)| *t <= typst_offset);
        let md_off = if i == 0 { 0 } else { self.map[i - 1].1 };
        md[..md_off.min(md.len())].matches('\n').count() + 1
    }
}

#[cfg(test)]
pub fn to_typst(md: &str) -> String {
    convert(md).typst
}

/// Convert Markdown to Typst. Each paragraph and heading starts with an
/// invisible `#metadata(line)<q>` marker so the preview can map pages back to
/// Markdown lines.
pub fn convert(md: &str) -> Converted {
    let mut opts = Options::empty();
    opts.insert(Options::ENABLE_STRIKETHROUGH);
    let parser = Parser::new_ext(md, opts).into_offset_iter();

    let mut map = Vec::new();
    // Line of the paragraph/heading whose marker hasn't been written yet. It is
    // written before the first text, so image-only paragraphs get none.
    let mut pending_line: Option<usize> = None;
    let line_of = |off: usize| md[..off].matches('\n').count() + 1;

    let mut out = String::new();
    // When inside an image we swallow alt text events.
    let mut in_image: Option<(String, String)> = None;
    let mut line_start = true;
    let mut list_depth = 0usize;

    let push = |out: &mut String, s: &str, line_start: &mut bool| {
        out.push_str(s);
        *line_start = s.ends_with('\n');
    };

    for (ev, range) in parser {
        map.push((out.len(), range.start));
        if let Some((dest, title)) = &in_image {
            if let Event::End(TagEnd::Image) = ev {
                let call = image_call(dest, title);
                push(&mut out, &call, &mut line_start);
                in_image = None;
            }
            continue;
        }
        match ev {
            Event::Start(Tag::Heading { level, .. }) => {
                let f = if level == HeadingLevel::H1 { "#chapter[" } else { "#section[" };
                push(&mut out, f, &mut line_start);
                line_start = true; // first char inside [ ] is a line start too
                pending_line = Some(line_of(range.start));
            }
            Event::End(TagEnd::Heading(_)) => push(&mut out, "]\n\n", &mut line_start),
            Event::Start(Tag::Paragraph) => pending_line = Some(line_of(range.start)),
            Event::End(TagEnd::Paragraph) => push(&mut out, "\n\n", &mut line_start),
            Event::Start(Tag::Strong) => {
                push(&mut out, "#strong[", &mut line_start);
            }
            Event::Start(Tag::Emphasis) => {
                push(&mut out, "#emph[", &mut line_start);
            }
            Event::Start(Tag::Strikethrough) => {
                push(&mut out, "#strike[", &mut line_start);
            }
            Event::End(TagEnd::Strong | TagEnd::Emphasis | TagEnd::Strikethrough) => {
                push(&mut out, "]", &mut line_start)
            }
            Event::Start(Tag::BlockQuote(_)) => {
                push(&mut out, "#quoteblock[\n", &mut line_start);
            }
            Event::End(TagEnd::BlockQuote(_)) => push(&mut out, "]\n\n", &mut line_start),
            Event::Start(Tag::List(start)) => {
                list_depth += 1;
                let open = match start {
                    Some(n) => format!("#enum(tight: true, start: {n},\n"),
                    None => "#list(tight: true,\n".to_string(),
                };
                push(&mut out, &open, &mut line_start);
            }
            Event::End(TagEnd::List(_)) => {
                list_depth = list_depth.saturating_sub(1);
                let end = if list_depth == 0 { ")\n\n" } else { "),\n" };
                push(&mut out, end, &mut line_start);
            }
            Event::Start(Tag::Item) => {
                push(&mut out, "[", &mut line_start);
                line_start = true;
            }
            Event::End(TagEnd::Item) => push(&mut out, "],\n", &mut line_start),
            Event::Start(Tag::Link { dest_url, .. }) => {
                let url = dest_url.replace('\\', "\\\\").replace('"', "\\\"");
                push(&mut out, &format!("#link(\"{url}\")["), &mut line_start);
            }
            Event::End(TagEnd::Link) => push(&mut out, "]", &mut line_start),
            Event::Start(Tag::Image { dest_url, title, .. }) => {
                in_image = Some((dest_url.to_string(), title.to_string()));
            }
            Event::Text(t) | Event::Code(t) => {
                if let Some(l) = pending_line.take() {
                    push(&mut out, &format!("#metadata({l})<q>"), &mut line_start);
                }
                let s = escape(&t, line_start);
                push(&mut out, &s, &mut line_start);
            }
            Event::SoftBreak => push(&mut out, " ", &mut line_start),
            Event::HardBreak => push(&mut out, " \\\n", &mut line_start),
            Event::Rule => push(&mut out, "#scenebreak()\n\n", &mut line_start),
            Event::Html(h) | Event::InlineHtml(h) => {
                let tag = h.trim().to_ascii_lowercase();
                if tag == "<!-- pagebreak -->" {
                    push(&mut out, "#pagebreak()\n\n", &mut line_start);
                } else if tag == "<u>" {
                    push(&mut out, "#underline[", &mut line_start);
                } else if tag == "</u>" {
                    push(&mut out, "]", &mut line_start);
                }
            }
            _ => {}
        }
    }
    Converted { typst: out, map }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn chapters_and_dialogue() {
        let t = to_typst("# Capitolul I\n\n— Bună, spuse **Nori**.\n");
        assert!(t.contains("#chapter[#metadata(1)<q>Capitolul I]"));
        assert!(t.contains("#metadata(3)<q>— Bună, spuse #strong[Nori]."));
    }

    #[test]
    fn escapes_markup() {
        let t = to_typst("1. not a list? a #tag and // comment\n");
        // pulldown parses "1. ..." as a list; plain text with specials:
        let t2 = to_typst("price $5 [x] a/b\n");
        assert!(t2.contains("price \\$5 \\[x\\] a\\/b"));
        let c = convert("one\n\ntwo $\n");
        let at = c.typst.find("two").unwrap();
        assert_eq!(c.md_line("one\n\ntwo $\n", at), 3);
        assert!(t.contains("\\#tag"));
    }

    #[test]
    fn lists_underline_links() {
        let t = to_typst("- one\n- two\n");
        assert!(t.contains("#list(tight: true,"));
        let t = to_typst("3. three\n4. four\n");
        assert!(t.contains("#enum(tight: true, start: 3,"));
        let t = to_typst("a <u>marked</u> word\n");
        assert!(t.contains("a #underline[marked] word"));
        let t = to_typst("see [the site](https://ex.com/a?b=\"c\")\n");
        assert!(t.contains("#link(\"https://ex.com/a?b=\\\"c\\\"\")[the site]"), "{t}");
    }

    #[test]
    fn images() {
        assert!(to_typst("![a](images/x.png)").contains("#spot(\"/images/x.png\")"));
        assert!(!to_typst("![a](images/x.png)").contains("metadata"));
        assert!(to_typst("![a](images/x.png \"full\")").contains("#fullbleed(\"/images/x.png\")"));
        assert!(to_typst("![a](images/x.png \"50%\")").contains("width: 50%"));
    }
}
