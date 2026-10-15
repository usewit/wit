//! A book on disk: a directory with a Markdown source, a book.yaml config and
//! an images/ folder. Wit's own files (history) live in `<dir>/.wit/`.

use anyhow::{Context, Result, anyhow};
use serde::{Deserialize, Serialize};
use std::path::{Path, PathBuf};
use std::time::Instant;

use crate::engine::{self, BookWorld};
use crate::revisions::{History, Revision};
use crate::{fountain, md, settings};
use std::collections::HashMap;
use typst_layout::PagedDocument;

const TEMPLATE: &str = include_str!("../template/book.typ");
const SCRIPT_TEMPLATE: &str = include_str!("../template/script.typ");

#[derive(Debug, Clone, Deserialize)]
pub struct BookEntry {
    pub id: String,
    pub dir: PathBuf,
    #[serde(default = "default_source")]
    pub source: String,
    #[serde(default = "default_config")]
    pub config: String,
}
fn default_source() -> String {
    "story.md".into()
}
fn default_config() -> String {
    "book.yaml".into()
}

/// Settings handed to the Typst template. Read from book.yaml, which is shared
/// with the older make_book.py pipeline, so unknown keys are ignored.
#[derive(Debug, Clone, Serialize)]
pub struct Settings {
    /// What is being written: "book", "essay", "screenplay" or "play". Scripts
    /// are Fountain files and use the script template.
    pub document_type: String,
    /// Preset name matching the trim size, or "custom" (for the settings panel).
    pub trim_preset: String,
    pub title: String,
    pub author: String,
    pub lang: String,
    pub trim_w_mm: f64,
    pub trim_h_mm: f64,
    pub bleed_mm: f64,
    pub margin_mm: f64,
    /// Spine-side margin; mirrored on facing pages.
    pub margin_inner_mm: f64,
    pub body_font_name: String,
    pub body_font_size_pt: f64,
    pub line_height_mult: f64,
    /// Extra space between paragraphs, as a fraction of a line.
    pub para_gap_lines: f64,
    pub justify: bool,
    pub hyphenate: bool,
    pub first_line_indent: bool,
    pub chapter_title_font_size_pt: f64,
    pub chapter_title_spacing_mult: f64,
    pub chapter_title_align: String,
    pub chapter_new_page: bool,
    pub chapter_recto: bool,
    pub include_title_page: bool,
    pub title_page_title_font_size_pt: f64,
    pub title_page_author_font_size_pt: f64,
    pub page_number_font_size_pt: f64,
    pub page_number_position: String,
    pub dark_background: bool,
    pub min_dpi: f64,
    /// Scripts: scene headings in bold, and scene numbers (`#12#`) in both margins.
    pub scene_heading_bold: bool,
    pub scene_numbers: bool,
    /// Preview only: plain page and text colours matching the app's dark
    /// theme ("" = the page's own colours). Never set for a PDF.
    pub preview_bg: String,
    pub preview_ink: String,
}

/// How preview pages look in the app: their print colours, or plain dark
/// pages to match a dark theme. PDFs always use the print colours.
#[derive(Debug, Clone, Copy, PartialEq, Default)]
pub enum PageLook {
    #[default]
    Print,
    Dark,
    Oled,
}

impl PageLook {
    pub fn parse(s: &str) -> Option<Self> {
        match s {
            "print" => Some(PageLook::Print),
            "dark" => Some(PageLook::Dark),
            "oled" => Some(PageLook::Oled),
            _ => None,
        }
    }
}

/// The kinds of document Wit makes, with the source file each one starts with.
pub const DOCUMENT_TYPES: &[(&str, &str)] =
    &[("book", "story.md"), ("essay", "essay.md"), ("screenplay", "script.fountain"), ("play", "play.fountain")];

pub fn is_script(document_type: &str) -> bool {
    matches!(document_type, "screenplay" | "play")
}

/// The open font each kind of document starts with.
pub fn default_font(document_type: &str) -> &'static str {
    match document_type {
        "screenplay" | "play" => "Courier Prime",
        "essay" => "Source Serif 4",
        _ => "EB Garamond",
    }
}

fn parse_wxh(s: &str) -> Option<(f64, f64)> {
    let lower = s.to_ascii_lowercase();
    let (w, h) = lower.split_once('x')?;
    Some((w.trim().parse().ok()?, h.trim().parse().ok()?))
}

impl Settings {
    pub fn from_yaml(y: &serde_yaml::Value) -> Result<Self> {
        let s = |k: &str, d: &str| y.get(k).and_then(|v| v.as_str()).unwrap_or(d).to_string();
        let f = |k: &str, d: f64| y.get(k).and_then(|v| v.as_f64()).unwrap_or(d);
        let b = |k: &str, d: bool| y.get(k).and_then(|v| v.as_bool()).unwrap_or(d);

        let kind = s("document_type", "book");
        if !DOCUMENT_TYPES.iter().any(|(k, _)| *k == kind) {
            return Err(anyhow!("document_type must be book, essay, screenplay or play"));
        }
        let script = is_script(&kind);
        let preset = s("trim_preset", if script { "8.5x11" } else { "a5" });
        let (trim_w_mm, trim_h_mm) = if preset == "custom" {
            parse_wxh(&s("trim_size_mm", "148x210"))
                .ok_or_else(|| anyhow!("trim_size_mm must look like \"148x210\""))?
        } else {
            let p = settings::preset(&preset).ok_or_else(|| anyhow!("unknown trim_preset '{preset}'"))?;
            (p.w_mm, p.h_mm)
        };

        Ok(Settings {
            document_type: kind.clone(),
            trim_preset: settings::preset_for(trim_w_mm, trim_h_mm).to_string(),
            title: s("book_title", "Untitled"),
            author: s("author", ""),
            lang: s("lang", "ro"),
            trim_w_mm,
            trim_h_mm,
            bleed_mm: f("bleed_mm", 3.0),
            margin_mm: f("margin_mm", 16.0),
            margin_inner_mm: f("margin_inner_mm", f("margin_mm", 16.0)),
            body_font_name: s("body_font_name", default_font(&kind)),
            body_font_size_pt: f("body_font_size_pt", if script { 12.0 } else { 14.0 }),
            line_height_mult: f("line_height_mult", 1.45),
            // make_book.py's para_spacing_mult is 1 + the gap in lines.
            para_gap_lines: f("para_gap_lines", f("para_spacing_mult", 1.5) - 1.0),
            justify: b("justify", false),
            hyphenate: b("hyphenate", false),
            first_line_indent: b("first_line_indent", false),
            chapter_title_font_size_pt: f("chapter_title_font_size_pt", 22.0),
            chapter_title_spacing_mult: f("chapter_title_spacing_mult", 2.0),
            chapter_title_align: s("chapter_title_align", "center"),
            chapter_new_page: b("chapter_new_page", true),
            chapter_recto: b("chapter_recto", false),
            include_title_page: b("include_title_page", true),
            title_page_title_font_size_pt: f("title_page_title_font_size_pt", 30.0),
            title_page_author_font_size_pt: f("title_page_author_font_size_pt", 16.0),
            page_number_font_size_pt: f("page_number_font_size_pt", 10.0),
            page_number_position: s("page_number_position", "bottom-outside"),
            dark_background: b("dark_background", false),
            min_dpi: f("min_dpi", 300.0),
            scene_heading_bold: b("scene_heading_bold", false),
            scene_numbers: b("scene_numbers", true),
            preview_bg: String::new(),
            preview_ink: String::new(),
        })
    }
}

#[derive(Debug, Clone, Serialize)]
pub struct PageRef {
    pub url: String,
}

#[derive(Debug, Clone, Serialize, Default)]
pub struct Preview {
    pub pages: Vec<PageRef>,
    /// `(markdown line, page)` pairs, for syncing the editor and the pages.
    pub lines: Vec<(u32, u32)>,
    pub error: Option<String>,
    pub ms: u128,
}

pub struct Book {
    pub entry: BookEntry,
    pub history: History,
    world: BookWorld,
    /// Rendered preview pages by name.
    pngs: HashMap<String, Vec<u8>>,
}

pub fn hash(s: &str) -> String {
    hash_bytes(s.as_bytes())
}

impl Book {
    pub fn new(entry: BookEntry) -> Self {
        // Before the rename to Wit, a book's own files (history) were in `.quire/`.
        let (old, new) = (entry.dir.join(".quire"), entry.dir.join(".wit"));
        if old.is_dir() && !new.exists() {
            let _ = std::fs::rename(&old, &new);
        }
        let history = History::new(new);
        if let Err(e) = history.migrate() {
            tracing::warn!("moving old history copies: {e:#}");
        }
        let world = BookWorld::new(entry.dir.clone());
        Book { entry, history, world, pngs: HashMap::new() }
    }

    pub fn dir(&self) -> &Path {
        &self.entry.dir
    }
    fn source_path(&self) -> PathBuf {
        self.dir().join(&self.entry.source)
    }

    pub fn read_source(&self) -> Result<String> {
        std::fs::read_to_string(self.source_path())
            .with_context(|| format!("reading {}", self.source_path().display()))
    }

    pub fn settings(&self) -> Result<Settings> {
        let path = self.dir().join(&self.entry.config);
        let y: serde_yaml::Value = match std::fs::read_to_string(&path) {
            Ok(t) => serde_yaml::from_str(&t).with_context(|| format!("parsing {}", path.display()))?,
            Err(_) => serde_yaml::Value::Null,
        };
        Settings::from_yaml(&y)
    }

    /// Apply settings-panel changes to book.yaml, editing it in place.
    pub fn save_settings(&self, changes: &serde_json::Map<String, serde_json::Value>) -> Result<Settings> {
        let path = self.dir().join(&self.entry.config);
        let text = std::fs::read_to_string(&path).unwrap_or_default();
        let updated = settings::apply_yaml_updates(&text, &settings::updates_from_json(changes)?);
        // Validate before touching the file.
        let parsed: serde_yaml::Value = serde_yaml::from_str(&updated)?;
        let new = Settings::from_yaml(&parsed)?;
        let tmp = path.with_extension("yaml.wit-tmp");
        std::fs::write(&tmp, updated)?;
        std::fs::rename(&tmp, &path)?;
        Ok(new)
    }

    pub fn image_checks(&self) -> Result<Vec<settings::ImageCheck>> {
        let s = self.settings()?;
        // Fountain scripts have no illustrations.
        if is_script(&s.document_type) {
            return Ok(vec![]);
        }
        Ok(settings::image_checks(self.dir(), &self.read_source()?, &s))
    }

    /// Save new source text. Refuses if the file changed since the client
    /// loaded it (`base_hash`), so edits made elsewhere are never clobbered.
    pub fn save_source(&mut self, content: &str, base_hash: &str) -> Result<String, SaveError> {
        let current = self.read_source().map_err(SaveError::Io)?;
        if hash(&current) != base_hash {
            return Err(SaveError::Conflict(hash(&current)));
        }
        if current == content {
            return Ok(base_hash.to_string());
        }
        let modified = std::fs::metadata(self.source_path()).and_then(|m| m.modified()).ok();
        self.history.before_save(&current, modified).map_err(SaveError::Io)?;
        self.write_source(content).map_err(SaveError::Io)?;
        Ok(hash(content))
    }

    /// Freeze the working draft (as saved on disk) as the next named revision.
    pub fn name_revision(&self, name: &str, base_hash: &str) -> Result<Revision, SaveError> {
        let current = self.read_source().map_err(SaveError::Io)?;
        if hash(&current) != base_hash {
            return Err(SaveError::Conflict(hash(&current)));
        }
        self.history.name(&current, name).map_err(SaveError::Io)
    }

    /// Make a revision or daily copy the working draft again. The draft it
    /// replaces is kept first, as a named revision.
    pub fn restore(&mut self, id: &str, base_hash: &str) -> Result<String, SaveError> {
        let text = self.history.read(id).map_err(SaveError::Io)?;
        let current = self.read_source().map_err(SaveError::Io)?;
        if hash(&current) != base_hash {
            return Err(SaveError::Conflict(hash(&current)));
        }
        if current != text {
            let what = match id.strip_prefix('v') {
                Some(n) => format!("v{n}"),
                None => match chrono::NaiveDate::parse_from_str(id, "%Y-%m-%d") {
                    Ok(d) => format!("the copy of {}", d.format("%-d %B %Y")),
                    Err(_) => format!("the copy of {id}"),
                },
            };
            self.history.name(&current, &format!("Before going back to {what}")).map_err(SaveError::Io)?;
            self.write_source(&text).map_err(SaveError::Io)?;
        }
        Ok(text)
    }

    fn write_source(&self, content: &str) -> Result<()> {
        let path = self.source_path();
        let tmp = path.with_extension("md.wit-tmp");
        std::fs::write(&tmp, content)?;
        std::fs::rename(&tmp, &path)?;
        Ok(())
    }

    /// Assemble the Typst main source: the template with the book content inlined
    /// (not #include'd, so content can use the template's functions).
    fn main_source(&self, source: &str, document_type: &str) -> (String, md::Converted, usize) {
        let (template, conv) = match document_type {
            "screenplay" => (SCRIPT_TEMPLATE, fountain::convert(source, fountain::Kind::Screenplay)),
            "play" => (SCRIPT_TEMPLATE, fountain::convert(source, fountain::Kind::Play)),
            _ => (TEMPLATE, md::convert(source)),
        };
        let marker = "#include \"content.typ\"";
        let at = template.find(marker).expect("template has the content marker");
        let main = format!("{}{}{}", &template[..at], conv.typst, &template[at + marker.len()..]);
        (main, conv, at)
    }

    /// Compile the book. `print` forces white pages regardless of book.yaml;
    /// `look` darkens preview pages to match the app (the book's own dark
    /// proofing pages, if on, take precedence).
    fn compile(&mut self, print: bool, look: PageLook) -> Result<(PagedDocument, Vec<(u32, u32)>)> {
        let source = self.read_source()?;
        let mut settings = self.settings()?;
        if print {
            settings.dark_background = false;
        } else if !settings.dark_background {
            let colours = match look {
                PageLook::Print => None,
                PageLook::Dark => Some(("#211f1c", "#e6e1d8")),
                PageLook::Oled => Some(("#000000", "#bfbab1")),
            };
            if let Some((bg, ink)) = colours {
                settings.preview_bg = bg.into();
                settings.preview_ink = ink.into();
            }
        }
        let (main, conv, content_at) = self.main_source(&source, &settings.document_type);
        self.world.update(&main, serde_json::to_vec(&settings)?);
        // Typst work is CPU-bound; let tokio move other tasks off this thread.
        let world = &self.world;
        match tokio::task::block_in_place(|| world.compile()) {
            Ok(doc) => {
                let lines = engine::line_pages(&doc);
                Ok((doc, lines))
            }
            Err(errs) => Err(anyhow!("{}", world.describe(&errs, &source, &conv, content_at))),
        }
    }

    /// Render preview pages as PNGs, kept in memory. A page's name is the hash
    /// of its content, so unchanged pages are neither re-rendered nor refetched.
    pub async fn render_preview(&mut self, ppi: u32, look: PageLook) -> Preview {
        let start = Instant::now();
        let result: Result<(Vec<PageRef>, Vec<(u32, u32)>)> = (|| {
            let (doc, lines) = self.compile(false, look)?;
            let pages = doc.pages();
            let names: Vec<String> =
                pages.iter().map(|p| format!("p-{:032x}.png", typst::utils::hash128(p))).collect();
            let todo: Vec<usize> = (0..pages.len()).filter(|&i| !self.pngs.contains_key(&names[i])).collect();
            let opts = typst_render::RenderOptions {
                pixel_per_pt: typst::utils::Scalar::new(ppi as f64 / 72.0),
                render_bleed: false,
            };
            let rendered: Vec<(usize, Vec<u8>)> = tokio::task::block_in_place(|| {
                std::thread::scope(|scope| {
                    let handles: Vec<_> = todo
                        .iter()
                        .map(|&i| {
                            let page = &pages[i];
                            let opts = &opts;
                            scope.spawn(move || (i, typst_render::render(page, opts).encode_png().unwrap_or_default()))
                        })
                        .collect();
                    handles.into_iter().map(|h| h.join().unwrap()).collect()
                })
            });
            for (i, png) in rendered {
                self.pngs.insert(names[i].clone(), png);
            }
            // Forget pages that are no longer part of the book.
            self.pngs.retain(|k, _| names.contains(k));
            let id = &self.entry.id;
            Ok((names.into_iter().map(|n| PageRef { url: format!("/api/books/{id}/pages/{n}") }).collect(), lines))
        })();
        let ms = start.elapsed().as_millis();
        match result {
            Ok((pages, lines)) => Preview { pages, lines, error: None, ms },
            Err(e) => Preview { pages: vec![], lines: vec![], error: Some(format!("{e:#}")), ms },
        }
    }

    pub fn page_png(&self, name: &str) -> Option<Vec<u8>> {
        self.pngs.get(name).cloned()
    }

    pub async fn render_pdf(&mut self, print: bool) -> Result<Vec<u8>> {
        let (doc, _) = self.compile(print, PageLook::Print)?;
        let pdf = tokio::task::block_in_place(|| typst_pdf::pdf(&doc, &typst_pdf::PdfOptions::default()));
        pdf.map_err(|errs| anyhow!("{}", errs.iter().map(|e| e.message.to_string()).collect::<Vec<_>>().join("\n")))
    }

    pub fn list_images(&self) -> Vec<String> {
        let dir = self.dir().join("images");
        let mut v: Vec<String> = std::fs::read_dir(dir)
            .into_iter()
            .flatten()
            .filter_map(|e| e.ok()?.file_name().into_string().ok())
            .filter(|n| {
                let l = n.to_ascii_lowercase();
                l.ends_with(".png") || l.ends_with(".jpg") || l.ends_with(".jpeg")
            })
            .map(|n| format!("images/{n}"))
            .collect();
        v.sort();
        v
    }
}

/// FNV-1a, enough to detect "this changed".
fn hash_bytes(b: &[u8]) -> String {
    let mut h: u64 = 0xcbf29ce484222325;
    for x in b {
        h ^= *x as u64;
        h = h.wrapping_mul(0x100000001b3);
    }
    format!("{h:016x}")
}

pub enum SaveError {
    Conflict(String),
    Io(anyhow::Error),
}
