//! The working draft as a Word (.docx) or OpenDocument (.odt) manuscript, for
//! an editor or anyone who works in a word processor.
//!
//! It's a manuscript, not the printed book: A4, Times New Roman 12 pt, one and
//! a half line spacing, indented paragraphs, each chapter on a new page. The
//! book's own fonts and page size stay in the PDFs. Illustrations go in at
//! their width on the page.

use anyhow::Result;
use pulldown_cmark::{Event, HeadingLevel, Options, Parser, Tag, TagEnd};
use std::io::Write;
use std::path::Path;
use crate::md::{Html, html_block, inline_html};
use zip::write::SimpleFileOptions;

#[derive(Clone, Copy, PartialEq)]
pub enum Format {
    Docx,
    Odt,
}

impl Format {
    pub fn parse(s: &str) -> Option<Self> {
        match s {
            "docx" => Some(Format::Docx),
            "odt" => Some(Format::Odt),
            _ => None,
        }
    }
    pub fn ext(self) -> &'static str {
        match self {
            Format::Docx => "docx",
            Format::Odt => "odt",
        }
    }
    pub fn mime(self) -> &'static str {
        match self {
            Format::Docx => "application/vnd.openxmlformats-officedocument.wordprocessingml.document",
            Format::Odt => "application/vnd.oasis.opendocument.text",
        }
    }
}

#[derive(Clone, Default, PartialEq)]
struct Style {
    bold: bool,
    italic: bool,
    underline: bool,
    strike: bool,
}

#[derive(Clone)]
enum Run {
    Text(String, Style, Option<String>),
    Break,
}

enum Block {
    Heading(u8, Vec<Run>),
    Para(Vec<Run>),
    Quote(Vec<Run>),
    Item { label: String, depth: usize, runs: Vec<Run> },
    SceneBreak,
    PageBreak,
    Image { file: String, width_pct: f64 },
}

/// The book's Markdown as blocks, with the same rules as the PDF.
fn blocks(md: &str) -> Vec<Block> {
    let mut opts = Options::empty();
    opts.insert(Options::ENABLE_STRIKETHROUGH);
    let mut out = vec![];
    let mut runs: Vec<Run> = vec![];
    let mut style = Style::default();
    let mut link: Option<String> = None;
    let mut heading: Option<u8> = None;
    let mut quote = 0usize;
    // Each open list: the next number, or None for bullets.
    let mut lists: Vec<Option<u64>> = vec![];
    let mut item: Option<String> = None;
    let mut in_image = false;
    // Inside an HTML block: its text so far.
    let mut html: Option<String> = None;

    let flush = |out: &mut Vec<Block>, runs: &mut Vec<Run>, heading: Option<u8>, quote: usize, item: &mut Option<String>, depth: usize| {
        if runs.is_empty() && item.is_none() {
            return;
        }
        let r = std::mem::take(runs);
        out.push(if let Some(label) = item.take() {
            Block::Item { label, depth, runs: r }
        } else if let Some(l) = heading {
            Block::Heading(l, r)
        } else if quote > 0 {
            Block::Quote(r)
        } else {
            Block::Para(r)
        });
    };

    for ev in Parser::new_ext(md, opts) {
        if in_image {
            if let Event::End(TagEnd::Image) = ev {
                in_image = false;
            }
            continue;
        }
        if let Some(text) = &mut html {
            match ev {
                Event::Html(h) => text.push_str(&h),
                Event::End(TagEnd::HtmlBlock) => {
                    for part in html_block(text) {
                        match part {
                            Html::PageBreak => out.push(Block::PageBreak),
                            Html::Text(t) => {
                                let t = t.lines().map(str::trim).collect::<Vec<_>>().join(" ");
                                runs.push(Run::Text(t, style.clone(), None));
                                flush(&mut out, &mut runs, heading, quote, &mut item, lists.len());
                            }
                            _ => {}
                        }
                    }
                    html = None;
                }
                _ => {}
            }
            continue;
        }
        match ev {
            Event::Start(Tag::HtmlBlock) => html = Some(String::new()),
            Event::Start(Tag::Heading { level, .. }) => heading = Some(if level == HeadingLevel::H1 { 1 } else { 2 }),
            Event::End(TagEnd::Heading(_)) => {
                flush(&mut out, &mut runs, heading, quote, &mut item, lists.len());
                heading = None;
            }
            Event::End(TagEnd::Paragraph) => flush(&mut out, &mut runs, heading, quote, &mut item, lists.len()),
            Event::Start(Tag::BlockQuote(_)) => quote += 1,
            Event::End(TagEnd::BlockQuote(_)) => quote = quote.saturating_sub(1),
            Event::Start(Tag::List(start)) => {
                // A list inside an item: the item's own text comes first.
                flush(&mut out, &mut runs, heading, quote, &mut item, lists.len());
                lists.push(start);
            }
            Event::End(TagEnd::List(_)) => {
                lists.pop();
            }
            Event::Start(Tag::Item) => {
                let label = match lists.last_mut() {
                    Some(Some(n)) => {
                        *n += 1;
                        format!("{}.", *n - 1)
                    }
                    _ => "•".to_string(),
                };
                item = Some(label);
            }
            Event::End(TagEnd::Item) => flush(&mut out, &mut runs, heading, quote, &mut item, lists.len()),
            Event::Start(Tag::Strong) => style.bold = true,
            Event::End(TagEnd::Strong) => style.bold = false,
            Event::Start(Tag::Emphasis) => style.italic = true,
            Event::End(TagEnd::Emphasis) => style.italic = false,
            Event::Start(Tag::Strikethrough) => style.strike = true,
            Event::End(TagEnd::Strikethrough) => style.strike = false,
            // No address yet (`[text]()`): plain text.
            Event::Start(Tag::Link { dest_url, .. }) => {
                link = Some(dest_url.to_string()).filter(|u| !u.trim().is_empty())
            }
            Event::End(TagEnd::Link) => link = None,
            Event::Start(Tag::Image { dest_url, title, .. }) => {
                flush(&mut out, &mut runs, heading, quote, &mut item, lists.len());
                let t = title.trim();
                let width_pct = if t.eq_ignore_ascii_case("full") {
                    100.0
                } else {
                    t.strip_suffix('%').and_then(|n| n.trim().parse::<f64>().ok()).unwrap_or(70.0).clamp(10.0, 100.0)
                };
                out.push(Block::Image { file: dest_url.to_string(), width_pct });
                in_image = true;
            }
            Event::Text(t) | Event::Code(t) => runs.push(Run::Text(t.to_string(), style.clone(), link.clone())),
            Event::SoftBreak => runs.push(Run::Text(" ".into(), style.clone(), link.clone())),
            Event::HardBreak => runs.push(Run::Break),
            Event::Rule => out.push(Block::SceneBreak),
            Event::Html(h) | Event::InlineHtml(h) => match inline_html(&h) {
                Html::PageBreak => out.push(Block::PageBreak),
                Html::Underline => style.underline = true,
                Html::EndUnderline => style.underline = false,
                Html::Break => runs.push(Run::Break),
                Html::Hidden => {}
                Html::Text(t) => runs.push(Run::Text(t.replace('\n', " "), style.clone(), link.clone())),
            },
            _ => {}
        }
    }
    flush(&mut out, &mut runs, heading, quote, &mut item, lists.len());
    out
}

fn esc(s: &str) -> String {
    s.replace('&', "&amp;").replace('<', "&lt;").replace('>', "&gt;").replace('"', "&quot;")
}

/// Text width on the manuscript page: A4 less 25 mm margins.
const TEXT_W_MM: f64 = 160.0;

struct Picture {
    name: String,
    bytes: Vec<u8>,
    w_mm: f64,
    h_mm: f64,
}

fn picture(dir: &Path, file: &str, width_pct: f64, n: usize) -> Option<Picture> {
    let rel = file.trim_start_matches("./").trim_start_matches('/');
    if rel.split(['/', '\\']).any(|part| part == "..") {
        return None;
    }
    let path = dir.join(rel);
    let bytes = std::fs::read(&path).ok()?;
    let size = imagesize::blob_size(&bytes).ok()?;
    let ext = path.extension()?.to_str()?.to_ascii_lowercase();
    let ext = match ext.as_str() {
        "jpeg" | "jpg" => "jpg",
        "png" | "gif" => ext.as_str(),
        _ => return None,
    }
    .to_string();
    let w_mm = TEXT_W_MM * width_pct / 100.0;
    let h_mm = w_mm * size.height as f64 / size.width.max(1) as f64;
    Some(Picture { name: format!("image{n}.{ext}"), bytes, w_mm, h_mm })
}

pub struct Book<'a> {
    pub title: &'a str,
    pub author: &'a str,
    /// "ro", "en"…
    pub lang: &'a str,
    pub source: &'a str,
    /// The book's folder, where illustrations live.
    pub dir: &'a Path,
}

pub fn export(book: &Book, format: Format) -> Result<Vec<u8>> {
    match format {
        Format::Docx => docx(book),
        Format::Odt => odt(book),
    }
}

// ---------------------------------------------------------------- Word

fn docx(book: &Book) -> Result<Vec<u8>> {
    let mut body = String::new();
    let mut rels: Vec<(String, String, bool)> = vec![]; // (id, target, external)
    let mut pictures: Vec<Picture> = vec![];

    let para = |style: &str, extra: &str, inner: &str| {
        format!("<w:p><w:pPr><w:pStyle w:val=\"{style}\"/>{extra}</w:pPr>{inner}</w:p>")
    };
    let runs_xml = |runs: &[Run], rels: &mut Vec<(String, String, bool)>| -> String {
        let mut s = String::new();
        for r in runs {
            match r {
                Run::Break => s.push_str("<w:r><w:br/></w:r>"),
                Run::Text(t, st, link) => {
                    let mut pr = String::new();
                    if st.bold { pr.push_str("<w:b/>"); }
                    if st.italic { pr.push_str("<w:i/>"); }
                    if st.underline || link.is_some() { pr.push_str("<w:u w:val=\"single\"/>"); }
                    if st.strike { pr.push_str("<w:strike/>"); }
                    let run = format!("<w:r><w:rPr>{pr}</w:rPr><w:t xml:space=\"preserve\">{}</w:t></w:r>", esc(t));
                    match link {
                        Some(url) => {
                            let id = format!("rLink{}", rels.len() + 1);
                            rels.push((id.clone(), url.clone(), true));
                            s.push_str(&format!("<w:hyperlink r:id=\"{id}\">{run}</w:hyperlink>"));
                        }
                        None => s.push_str(&run),
                    }
                }
            }
        }
        s
    };
    let plain = |t: &str| format!("<w:r><w:t xml:space=\"preserve\">{}</w:t></w:r>", esc(t));

    body.push_str(&para("Title", "", &plain(book.title)));
    if !book.author.is_empty() {
        body.push_str(&para("Subtitle", "", &plain(book.author)));
    }
    for b in blocks(book.source) {
        match b {
            // Chapters start a new page, which also ends the title page.
            Block::Heading(1, r) => body.push_str(&para("Heading1", "", &runs_xml(&r, &mut rels))),
            Block::Heading(_, r) => body.push_str(&para("Heading2", "", &runs_xml(&r, &mut rels))),
            Block::Para(r) => body.push_str(&para("BodyText", "", &runs_xml(&r, &mut rels))),
            Block::Quote(r) => body.push_str(&para("Quote", "", &runs_xml(&r, &mut rels))),
            Block::Item { label, depth, runs } => {
                let indent = 360 * depth.max(1);
                let inner = format!("{}<w:r><w:tab/></w:r>{}", plain(&label), runs_xml(&runs, &mut rels));
                body.push_str(&para("ListText", &format!("<w:ind w:left=\"{}\" w:hanging=\"360\"/>", indent + 360), &inner));
            }
            Block::SceneBreak => body.push_str(&para("SceneBreak", "", &plain("*  *  *"))),
            Block::PageBreak => body.push_str("<w:p><w:r><w:br w:type=\"page\"/></w:r></w:p>"),
            Block::Image { file, width_pct } => {
                let n = pictures.len() + 1;
                match picture(book.dir, &file, width_pct, n) {
                    Some(p) => {
                        let id = format!("rImg{n}");
                        rels.push((id.clone(), format!("media/{}", p.name), false));
                        let (cx, cy) = ((p.w_mm * 36000.0) as u64, (p.h_mm * 36000.0) as u64);
                        let drawing = format!(
                            "<w:r><w:drawing><wp:inline distT=\"0\" distB=\"0\" distL=\"0\" distR=\"0\">\
                             <wp:extent cx=\"{cx}\" cy=\"{cy}\"/><wp:docPr id=\"{n}\" name=\"{}\"/>\
                             <a:graphic xmlns:a=\"http://schemas.openxmlformats.org/drawingml/2006/main\">\
                             <a:graphicData uri=\"http://schemas.openxmlformats.org/drawingml/2006/picture\">\
                             <pic:pic xmlns:pic=\"http://schemas.openxmlformats.org/drawingml/2006/picture\">\
                             <pic:nvPicPr><pic:cNvPr id=\"{n}\" name=\"{}\"/><pic:cNvPicPr/></pic:nvPicPr>\
                             <pic:blipFill><a:blip r:embed=\"{id}\"/><a:stretch><a:fillRect/></a:stretch></pic:blipFill>\
                             <pic:spPr><a:xfrm><a:off x=\"0\" y=\"0\"/><a:ext cx=\"{cx}\" cy=\"{cy}\"/></a:xfrm>\
                             <a:prstGeom prst=\"rect\"><a:avLst/></a:prstGeom></pic:spPr></pic:pic>\
                             </a:graphicData></a:graphic></wp:inline></w:drawing></w:r>",
                            esc(&file), esc(&p.name)
                        );
                        body.push_str(&para("Picture", "", &drawing));
                        pictures.push(p);
                    }
                    None => body.push_str(&para("Picture", "", &plain(&format!("[Illustration: {file}]")))),
                }
            }
        }
    }
    let document = format!(
        "<?xml version=\"1.0\" encoding=\"UTF-8\" standalone=\"yes\"?>\
         <w:document xmlns:w=\"http://schemas.openxmlformats.org/wordprocessingml/2006/main\" \
         xmlns:r=\"http://schemas.openxmlformats.org/officeDocument/2006/relationships\" \
         xmlns:wp=\"http://schemas.openxmlformats.org/drawingml/2006/wordprocessingDrawing\">\
         <w:body>{body}<w:sectPr><w:pgSz w:w=\"11906\" w:h=\"16838\"/>\
         <w:pgMar w:top=\"1418\" w:right=\"1418\" w:bottom=\"1418\" w:left=\"1418\" w:header=\"709\" w:footer=\"709\" w:gutter=\"0\"/>\
         </w:sectPr></w:body></w:document>"
    );

    let lang = esc(book.lang);
    let styles = format!(
        "<?xml version=\"1.0\" encoding=\"UTF-8\" standalone=\"yes\"?>\
         <w:styles xmlns:w=\"http://schemas.openxmlformats.org/wordprocessingml/2006/main\">\
         <w:docDefaults><w:rPrDefault><w:rPr><w:rFonts w:ascii=\"Times New Roman\" w:hAnsi=\"Times New Roman\" w:cs=\"Times New Roman\" w:eastAsia=\"Times New Roman\"/>\
         <w:sz w:val=\"24\"/><w:szCs w:val=\"24\"/><w:lang w:val=\"{lang}\"/></w:rPr></w:rPrDefault>\
         <w:pPrDefault><w:pPr><w:spacing w:after=\"0\" w:line=\"360\" w:lineRule=\"auto\"/></w:pPr></w:pPrDefault></w:docDefaults>\
         <w:style w:type=\"paragraph\" w:default=\"1\" w:styleId=\"Normal\"><w:name w:val=\"Normal\"/></w:style>\
         <w:style w:type=\"paragraph\" w:styleId=\"BodyText\"><w:name w:val=\"Body Text\"/><w:basedOn w:val=\"Normal\"/><w:qFormat/><w:pPr><w:ind w:firstLine=\"709\"/></w:pPr></w:style>\
         <w:style w:type=\"paragraph\" w:styleId=\"Title\"><w:name w:val=\"Title\"/><w:basedOn w:val=\"Normal\"/><w:qFormat/><w:pPr><w:spacing w:before=\"4800\" w:after=\"480\"/><w:jc w:val=\"center\"/></w:pPr><w:rPr><w:sz w:val=\"48\"/></w:rPr></w:style>\
         <w:style w:type=\"paragraph\" w:styleId=\"Subtitle\"><w:name w:val=\"Subtitle\"/><w:basedOn w:val=\"Normal\"/><w:qFormat/><w:pPr><w:jc w:val=\"center\"/></w:pPr><w:rPr><w:sz w:val=\"28\"/></w:rPr></w:style>\
         <w:style w:type=\"paragraph\" w:styleId=\"Heading1\"><w:name w:val=\"heading 1\"/><w:basedOn w:val=\"Normal\"/><w:next w:val=\"BodyText\"/><w:qFormat/><w:pPr><w:keepNext/><w:pageBreakBefore/><w:spacing w:before=\"1440\" w:after=\"480\"/><w:jc w:val=\"center\"/><w:outlineLvl w:val=\"0\"/></w:pPr><w:rPr><w:b/><w:sz w:val=\"32\"/></w:rPr></w:style>\
         <w:style w:type=\"paragraph\" w:styleId=\"Heading2\"><w:name w:val=\"heading 2\"/><w:basedOn w:val=\"Normal\"/><w:next w:val=\"BodyText\"/><w:qFormat/><w:pPr><w:keepNext/><w:spacing w:before=\"360\" w:after=\"240\"/><w:outlineLvl w:val=\"1\"/></w:pPr><w:rPr><w:b/><w:sz w:val=\"26\"/></w:rPr></w:style>\
         <w:style w:type=\"paragraph\" w:styleId=\"Quote\"><w:name w:val=\"Quote\"/><w:basedOn w:val=\"Normal\"/><w:qFormat/><w:pPr><w:spacing w:before=\"120\" w:after=\"120\"/><w:ind w:left=\"709\" w:right=\"709\"/></w:pPr><w:rPr><w:i/></w:rPr></w:style>\
         <w:style w:type=\"paragraph\" w:styleId=\"ListText\"><w:name w:val=\"List Paragraph\"/><w:basedOn w:val=\"Normal\"/><w:qFormat/></w:style>\
         <w:style w:type=\"paragraph\" w:styleId=\"SceneBreak\"><w:name w:val=\"Scene Break\"/><w:basedOn w:val=\"Normal\"/><w:qFormat/><w:pPr><w:spacing w:before=\"240\" w:after=\"240\"/><w:jc w:val=\"center\"/></w:pPr></w:style>\
         <w:style w:type=\"paragraph\" w:styleId=\"Picture\"><w:name w:val=\"Illustration\"/><w:basedOn w:val=\"Normal\"/><w:qFormat/><w:pPr><w:spacing w:before=\"240\" w:after=\"240\" w:line=\"240\" w:lineRule=\"auto\"/><w:jc w:val=\"center\"/></w:pPr></w:style>\
         </w:styles>"
    );

    let mut doc_rels = String::from(
        "<?xml version=\"1.0\" encoding=\"UTF-8\" standalone=\"yes\"?>\
         <Relationships xmlns=\"http://schemas.openxmlformats.org/package/2006/relationships\">\
         <Relationship Id=\"rStyles\" Type=\"http://schemas.openxmlformats.org/officeDocument/2006/relationships/styles\" Target=\"styles.xml\"/>",
    );
    for (id, target, external) in &rels {
        if *external {
            doc_rels.push_str(&format!(
                "<Relationship Id=\"{id}\" Type=\"http://schemas.openxmlformats.org/officeDocument/2006/relationships/hyperlink\" Target=\"{}\" TargetMode=\"External\"/>",
                esc(target)
            ));
        } else {
            doc_rels.push_str(&format!(
                "<Relationship Id=\"{id}\" Type=\"http://schemas.openxmlformats.org/officeDocument/2006/relationships/image\" Target=\"{}\"/>",
                esc(target)
            ));
        }
    }
    doc_rels.push_str("</Relationships>");

    let core = format!(
        "<?xml version=\"1.0\" encoding=\"UTF-8\" standalone=\"yes\"?>\
         <cp:coreProperties xmlns:cp=\"http://schemas.openxmlformats.org/package/2006/metadata/core-properties\" \
         xmlns:dc=\"http://purl.org/dc/elements/1.1/\"><dc:title>{}</dc:title><dc:creator>{}</dc:creator><dc:language>{lang}</dc:language></cp:coreProperties>",
        esc(book.title),
        esc(book.author)
    );

    let mut z = zip::ZipWriter::new(std::io::Cursor::new(Vec::new()));
    let opt = SimpleFileOptions::default();
    let file = |z: &mut zip::ZipWriter<_>, name: &str, data: &[u8]| -> Result<()> {
        z.start_file(name, opt)?;
        z.write_all(data)?;
        Ok(())
    };
    file(&mut z, "[Content_Types].xml", b"<?xml version=\"1.0\" encoding=\"UTF-8\" standalone=\"yes\"?>\
        <Types xmlns=\"http://schemas.openxmlformats.org/package/2006/content-types\">\
        <Default Extension=\"rels\" ContentType=\"application/vnd.openxmlformats-package.relationships+xml\"/>\
        <Default Extension=\"xml\" ContentType=\"application/xml\"/>\
        <Default Extension=\"png\" ContentType=\"image/png\"/>\
        <Default Extension=\"jpg\" ContentType=\"image/jpeg\"/>\
        <Default Extension=\"gif\" ContentType=\"image/gif\"/>\
        <Override PartName=\"/word/document.xml\" ContentType=\"application/vnd.openxmlformats-officedocument.wordprocessingml.document.main+xml\"/>\
        <Override PartName=\"/word/styles.xml\" ContentType=\"application/vnd.openxmlformats-officedocument.wordprocessingml.styles+xml\"/>\
        <Override PartName=\"/docProps/core.xml\" ContentType=\"application/vnd.openxmlformats-package.core-properties+xml\"/>\
        </Types>")?;
    file(&mut z, "_rels/.rels", b"<?xml version=\"1.0\" encoding=\"UTF-8\" standalone=\"yes\"?>\
        <Relationships xmlns=\"http://schemas.openxmlformats.org/package/2006/relationships\">\
        <Relationship Id=\"rDoc\" Type=\"http://schemas.openxmlformats.org/officeDocument/2006/relationships/officeDocument\" Target=\"word/document.xml\"/>\
        <Relationship Id=\"rCore\" Type=\"http://schemas.openxmlformats.org/package/2006/relationships/metadata/core-properties\" Target=\"docProps/core.xml\"/>\
        </Relationships>")?;
    file(&mut z, "word/document.xml", document.as_bytes())?;
    file(&mut z, "word/styles.xml", styles.as_bytes())?;
    file(&mut z, "word/_rels/document.xml.rels", doc_rels.as_bytes())?;
    file(&mut z, "docProps/core.xml", core.as_bytes())?;
    for p in &pictures {
        file(&mut z, &format!("word/media/{}", p.name), &p.bytes)?;
    }
    Ok(z.finish()?.into_inner())
}

// ---------------------------------------------------------------- OpenDocument

/// Text for ODF: runs of spaces and tabs need their own elements.
fn odf_text(t: &str) -> String {
    let mut s = String::new();
    let mut spaces = 0usize;
    let flush = |s: &mut String, spaces: &mut usize| {
        if *spaces > 0 {
            s.push(' ');
            if *spaces > 1 {
                s.push_str(&format!("<text:s text:c=\"{}\"/>", *spaces - 1));
            }
            *spaces = 0;
        }
    };
    for c in t.chars() {
        match c {
            ' ' => spaces += 1,
            '\t' => {
                flush(&mut s, &mut spaces);
                s.push_str("<text:tab/>");
            }
            _ => {
                flush(&mut s, &mut spaces);
                s.push_str(&esc(&c.to_string()));
            }
        }
    }
    flush(&mut s, &mut spaces);
    s
}

fn odt_runs(runs: &[Run]) -> String {
    let mut s = String::new();
    for r in runs {
        match r {
            Run::Break => s.push_str("<text:line-break/>"),
            Run::Text(t, st, link) => {
                let bits = st.bold as u8 | (st.italic as u8) << 1 | (st.underline as u8) << 2 | (st.strike as u8) << 3;
                let inner = if bits == 0 {
                    odf_text(t)
                } else {
                    format!("<text:span text:style-name=\"T{bits}\">{}</text:span>", odf_text(t))
                };
                match link {
                    Some(url) => s.push_str(&format!("<text:a xlink:type=\"simple\" xlink:href=\"{}\">{inner}</text:a>", esc(url))),
                    None => s.push_str(&inner),
                }
            }
        }
    }
    s
}

fn odt(book: &Book) -> Result<Vec<u8>> {
    let mut body = String::new();
    let mut pictures: Vec<Picture> = vec![];
    let p = |style: &str, inner: &str| format!("<text:p text:style-name=\"{style}\">{inner}</text:p>");

    body.push_str(&p("Title", &odf_text(book.title)));
    if !book.author.is_empty() {
        body.push_str(&p("Subtitle", &odf_text(book.author)));
    }
    for b in blocks(book.source) {
        match b {
            Block::Heading(level, r) => {
                let style = if level == 1 { "Heading_20_1" } else { "Heading_20_2" };
                body.push_str(&format!(
                    "<text:h text:style-name=\"{style}\" text:outline-level=\"{level}\">{}</text:h>",
                    odt_runs(&r)
                ));
            }
            Block::Para(r) => body.push_str(&p("Text_20_body", &odt_runs(&r))),
            Block::Quote(r) => body.push_str(&p("Quotations", &odt_runs(&r))),
            Block::Item { label, depth, runs } => {
                let style = format!("List{}", depth.clamp(1, 4));
                body.push_str(&p(&style, &format!("{}<text:tab/>{}", odf_text(&label), odt_runs(&runs))));
            }
            Block::SceneBreak => body.push_str(&p("Scene_20_break", "*  *  *")),
            Block::PageBreak => body.push_str(&p("Page_20_break", "")),
            Block::Image { file, width_pct } => {
                let n = pictures.len() + 1;
                match picture(book.dir, &file, width_pct, n) {
                    Some(pic) => {
                        body.push_str(&p(
                            "Illustration",
                            &format!(
                                "<draw:frame draw:name=\"{}\" text:anchor-type=\"as-char\" svg:width=\"{:.1}mm\" svg:height=\"{:.1}mm\">\
                                 <draw:image xlink:href=\"Pictures/{}\" xlink:type=\"simple\" xlink:show=\"embed\" xlink:actuate=\"onLoad\"/></draw:frame>",
                                esc(&file), pic.w_mm, pic.h_mm, pic.name
                            ),
                        ));
                        pictures.push(pic);
                    }
                    None => body.push_str(&p("Illustration", &odf_text(&format!("[Illustration: {file}]")))),
                }
            }
        }
    }

    // One automatic style per mix of bold, italic, underline and strike.
    let mut auto = String::new();
    for bits in 1u8..16 {
        let mut a = String::new();
        if bits & 1 != 0 { a.push_str(" fo:font-weight=\"bold\""); }
        if bits & 2 != 0 { a.push_str(" fo:font-style=\"italic\""); }
        if bits & 4 != 0 { a.push_str(" style:text-underline-style=\"solid\" style:text-underline-width=\"auto\" style:text-underline-color=\"font-color\""); }
        if bits & 8 != 0 { a.push_str(" style:text-line-through-style=\"solid\""); }
        auto.push_str(&format!("<style:style style:name=\"T{bits}\" style:family=\"text\"><style:text-properties{a}/></style:style>"));
    }
    const NS: &str = "xmlns:office=\"urn:oasis:names:tc:opendocument:xmlns:office:1.0\" \
        xmlns:style=\"urn:oasis:names:tc:opendocument:xmlns:style:1.0\" \
        xmlns:text=\"urn:oasis:names:tc:opendocument:xmlns:text:1.0\" \
        xmlns:draw=\"urn:oasis:names:tc:opendocument:xmlns:drawing:1.0\" \
        xmlns:fo=\"urn:oasis:names:tc:opendocument:xmlns:xsl-fo-compatible:1.0\" \
        xmlns:xlink=\"http://www.w3.org/1999/xlink\" \
        xmlns:dc=\"http://purl.org/dc/elements/1.1/\" \
        xmlns:meta=\"urn:oasis:names:tc:opendocument:xmlns:meta:1.0\" \
        xmlns:svg=\"urn:oasis:names:tc:opendocument:xmlns:svg-compatible:1.0\" office:version=\"1.3\"";
    let content = format!(
        "<?xml version=\"1.0\" encoding=\"UTF-8\"?><office:document-content {NS}>\
         <office:font-face-decls><style:font-face style:name=\"Times New Roman\" svg:font-family=\"'Times New Roman'\" style:font-family-generic=\"roman\"/></office:font-face-decls>\
         <office:automatic-styles>{auto}</office:automatic-styles>\
         <office:body><office:text>{body}</office:text></office:body></office:document-content>"
    );

    let lang = esc(book.lang);
    let list_styles: String = (1..=4)
        .map(|d| {
            format!(
                "<style:style style:name=\"List{d}\" style:display-name=\"List {d}\" style:family=\"paragraph\" style:parent-style-name=\"Standard\" style:class=\"list\">\
                 <style:paragraph-properties fo:margin-left=\"{}cm\" fo:text-indent=\"-0.63cm\"><style:tab-stops><style:tab-stop style:position=\"0cm\"/></style:tab-stops></style:paragraph-properties></style:style>",
                0.63 * (d as f64 + 1.0)
            )
        })
        .collect();
    let styles = format!(
        "<?xml version=\"1.0\" encoding=\"UTF-8\"?><office:document-styles {NS}>\
         <office:font-face-decls><style:font-face style:name=\"Times New Roman\" svg:font-family=\"'Times New Roman'\" style:font-family-generic=\"roman\"/></office:font-face-decls>\
         <office:styles>\
         <style:default-style style:family=\"paragraph\"><style:paragraph-properties fo:line-height=\"150%\"/>\
         <style:text-properties style:font-name=\"Times New Roman\" fo:font-size=\"12pt\" fo:language=\"{lang}\"/></style:default-style>\
         <style:style style:name=\"Standard\" style:family=\"paragraph\" style:class=\"text\"/>\
         <style:style style:name=\"Text_20_body\" style:display-name=\"Text body\" style:family=\"paragraph\" style:parent-style-name=\"Standard\" style:class=\"text\">\
         <style:paragraph-properties fo:text-indent=\"1.25cm\" fo:margin-top=\"0cm\" fo:margin-bottom=\"0cm\"/></style:style>\
         <style:style style:name=\"Title\" style:family=\"paragraph\" style:parent-style-name=\"Standard\" style:class=\"chapter\">\
         <style:paragraph-properties fo:text-align=\"center\" fo:margin-top=\"8cm\" fo:margin-bottom=\"0.85cm\"/><style:text-properties fo:font-size=\"24pt\"/></style:style>\
         <style:style style:name=\"Subtitle\" style:family=\"paragraph\" style:parent-style-name=\"Standard\" style:class=\"chapter\">\
         <style:paragraph-properties fo:text-align=\"center\"/><style:text-properties fo:font-size=\"14pt\"/></style:style>\
         <style:style style:name=\"Heading_20_1\" style:display-name=\"Heading 1\" style:family=\"paragraph\" style:parent-style-name=\"Standard\" style:next-style-name=\"Text_20_body\" style:default-outline-level=\"1\" style:class=\"text\">\
         <style:paragraph-properties fo:break-before=\"page\" fo:text-align=\"center\" fo:margin-top=\"2.5cm\" fo:margin-bottom=\"0.85cm\" fo:keep-with-next=\"always\"/>\
         <style:text-properties fo:font-size=\"16pt\" fo:font-weight=\"bold\"/></style:style>\
         <style:style style:name=\"Heading_20_2\" style:display-name=\"Heading 2\" style:family=\"paragraph\" style:parent-style-name=\"Standard\" style:next-style-name=\"Text_20_body\" style:default-outline-level=\"2\" style:class=\"text\">\
         <style:paragraph-properties fo:margin-top=\"0.6cm\" fo:margin-bottom=\"0.4cm\" fo:keep-with-next=\"always\"/>\
         <style:text-properties fo:font-size=\"13pt\" fo:font-weight=\"bold\"/></style:style>\
         <style:style style:name=\"Quotations\" style:family=\"paragraph\" style:parent-style-name=\"Standard\" style:class=\"html\">\
         <style:paragraph-properties fo:margin-left=\"1.25cm\" fo:margin-right=\"1.25cm\" fo:margin-top=\"0.2cm\" fo:margin-bottom=\"0.2cm\"/><style:text-properties fo:font-style=\"italic\"/></style:style>\
         <style:style style:name=\"Scene_20_break\" style:display-name=\"Scene break\" style:family=\"paragraph\" style:parent-style-name=\"Standard\">\
         <style:paragraph-properties fo:text-align=\"center\" fo:margin-top=\"0.4cm\" fo:margin-bottom=\"0.4cm\"/></style:style>\
         <style:style style:name=\"Page_20_break\" style:display-name=\"Page break\" style:family=\"paragraph\" style:parent-style-name=\"Standard\">\
         <style:paragraph-properties fo:break-after=\"page\"/></style:style>\
         <style:style style:name=\"Illustration\" style:family=\"paragraph\" style:parent-style-name=\"Standard\">\
         <style:paragraph-properties fo:text-align=\"center\" fo:line-height=\"100%\" fo:margin-top=\"0.4cm\" fo:margin-bottom=\"0.4cm\"/></style:style>\
         {list_styles}\
         </office:styles>\
         <office:automatic-styles><style:page-layout style:name=\"pm1\">\
         <style:page-layout-properties fo:page-width=\"210mm\" fo:page-height=\"297mm\" fo:margin-top=\"25mm\" fo:margin-bottom=\"25mm\" fo:margin-left=\"25mm\" fo:margin-right=\"25mm\"/>\
         </style:page-layout></office:automatic-styles>\
         <office:master-styles><style:master-page style:name=\"Standard\" style:page-layout-name=\"pm1\"/></office:master-styles>\
         </office:document-styles>"
    );
    let meta = format!(
        "<?xml version=\"1.0\" encoding=\"UTF-8\"?><office:document-meta {NS}><office:meta>\
         <dc:title>{}</dc:title><dc:creator>{}</dc:creator><dc:language>{lang}</dc:language><meta:generator>Wit</meta:generator>\
         </office:meta></office:document-meta>",
        esc(book.title),
        esc(book.author)
    );
    let mut manifest = String::from(
        "<?xml version=\"1.0\" encoding=\"UTF-8\"?>\
         <manifest:manifest xmlns:manifest=\"urn:oasis:names:tc:opendocument:xmlns:manifest:1.0\" manifest:version=\"1.3\">\
         <manifest:file-entry manifest:full-path=\"/\" manifest:media-type=\"application/vnd.oasis.opendocument.text\"/>\
         <manifest:file-entry manifest:full-path=\"content.xml\" manifest:media-type=\"text/xml\"/>\
         <manifest:file-entry manifest:full-path=\"styles.xml\" manifest:media-type=\"text/xml\"/>\
         <manifest:file-entry manifest:full-path=\"meta.xml\" manifest:media-type=\"text/xml\"/>",
    );
    for pic in &pictures {
        let mime = match pic.name.rsplit('.').next() {
            Some("jpg") => "image/jpeg",
            Some("gif") => "image/gif",
            _ => "image/png",
        };
        manifest.push_str(&format!("<manifest:file-entry manifest:full-path=\"Pictures/{}\" manifest:media-type=\"{mime}\"/>", pic.name));
    }
    manifest.push_str("</manifest:manifest>");

    let mut z = zip::ZipWriter::new(std::io::Cursor::new(Vec::new()));
    // The mimetype comes first and uncompressed, so tools can recognise the file.
    z.start_file("mimetype", SimpleFileOptions::default().compression_method(zip::CompressionMethod::Stored))?;
    z.write_all(Format::Odt.mime().as_bytes())?;
    let opt = SimpleFileOptions::default();
    for (name, data) in [
        ("content.xml", content.as_bytes()),
        ("styles.xml", styles.as_bytes()),
        ("meta.xml", meta.as_bytes()),
        ("META-INF/manifest.xml", manifest.as_bytes()),
    ] {
        z.start_file(name, opt)?;
        z.write_all(data)?;
    }
    for pic in &pictures {
        z.start_file(format!("Pictures/{}", pic.name), opt)?;
        z.write_all(&pic.bytes)?;
    }
    Ok(z.finish()?.into_inner())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn blocks_from_markdown() {
        let b = blocks("# Unu\n\nText **bold** și <u>sub</u>.\n\n---\n\n- a\n- b\n\n![](images/x.png \"50%\")\n");
        assert!(matches!(b[0], Block::Heading(1, _)));
        assert!(matches!(b[1], Block::Para(_)));
        assert!(matches!(b[2], Block::SceneBreak));
        assert!(matches!(&b[3], Block::Item { label, .. } if label == "•"));
        assert!(matches!(b[5], Block::Image { width_pct, .. } if width_pct == 50.0));
    }

    #[test]
    fn empty_links_and_unknown_html() {
        let b = blocks("a [x]() <b> c<br>d <!-- note -->\n\n<tag>\n\n<!-- pagebreak -->\n");
        let Block::Para(runs) = &b[0] else { panic!() };
        let text: String = runs.iter().map(|r| match r { Run::Text(t, _, l) => { assert!(l.is_none()); t.as_str() } Run::Break => "|" }).collect();
        assert_eq!(text, "a x <b> c|d ");
        assert!(matches!(&b[1], Block::Para(r) if matches!(&r[0], Run::Text(t, ..) if t == "<tag>")));
        assert!(matches!(b[2], Block::PageBreak));
    }

    #[test]
    fn both_formats_zip() {
        let dir = std::env::temp_dir();
        let book = Book { title: "T & co", author: "A", lang: "ro", source: "# Cap\n\nUnu  doi *trei*.\n", dir: &dir };
        for f in [Format::Docx, Format::Odt] {
            let bytes = export(&book, f).unwrap();
            assert_eq!(&bytes[..2], b"PK");
        }
        assert_eq!(odf_text("a  b"), "a <text:s text:c=\"1\"/>b");
    }
}
