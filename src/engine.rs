//! In-process Typst compiler for one book directory.
//!
//! The world is kept between compiles, so Typst's memoization (comemo) reuses
//! everything that didn't change: typing a word re-lays-out only what moved.

use std::collections::HashMap;
use std::path::{Path, PathBuf};
use std::sync::{Mutex, OnceLock};
use std::time::SystemTime;

use typst::diag::{FileError, FileResult, Severity, SourceDiagnostic};
use typst::foundations::{Bytes, Datetime, Duration, Label, Selector, Value};
use typst::introspection::Introspector;
use typst_layout::PagedDocument;
use typst::syntax::{FileId, RootedPath, Source, VirtualPath, VirtualRoot};
use typst::text::{Font, FontBook};
use typst::utils::{LazyHash, PicoStr};
use typst::{Library, LibraryExt, World, WorldExt};
use typst_kit::fonts::FontStore;

use crate::md::Converted;

static FONTS: OnceLock<FontStore> = OnceLock::new();

/// Load fonts once per process: the bundled ones first so they win, then the
/// system's, then Typst's embedded defaults.
pub fn init_fonts(dir: &Path) {
    FONTS.get_or_init(|| {
        let mut store = FontStore::new();
        store.extend(typst_kit::fonts::scan(dir));
        store.extend(typst_kit::fonts::system());
        store.extend(typst_kit::fonts::embedded());
        store
    });
}

/// The open (SIL OFL) fonts shipped in `fonts/`, with what each is good for.
pub const OPEN_FONTS: &[(&str, &str)] = &[
    ("EB Garamond", "Classic book face for novels"),
    ("Crimson Pro", "Book text, a little more modern"),
    ("Libre Baskerville", "Baskerville, large and very readable"),
    ("Literata", "Made for long reading"),
    ("Lora", "Contemporary and warm"),
    ("Alegreya", "Literary, with a calligraphic touch"),
    ("Source Serif 4", "Essays and non-fiction"),
    ("Andika", "For children learning to read"),
    ("Atkinson Hyperlegible", "For low vision and large print"),
    ("Courier Prime", "Screenplays, stage plays and manuscripts"),
];

/// Every other font family Typst can use here, sorted.
pub fn other_families() -> Vec<String> {
    let mut v: Vec<String> = fonts()
        .book()
        .families()
        .map(|(f, _)| f.to_string())
        .filter(|f| !OPEN_FONTS.iter().any(|(o, _)| o.eq_ignore_ascii_case(f)))
        .collect();
    v.sort_by_key(|f| f.to_lowercase());
    v.dedup();
    v
}

fn fonts() -> &'static FontStore {
    FONTS.get().expect("init_fonts not called")
}

fn project_file(path: &str) -> FileId {
    RootedPath::new(VirtualRoot::Project, VirtualPath::new(path).expect("valid path")).intern()
}

pub struct BookWorld {
    root: PathBuf,
    library: LazyHash<Library>,
    main: Source,
    config_id: FileId,
    config: Bytes,
    /// File contents by id, reloaded when the file's mtime changes. Reusing the
    /// same `Bytes` keeps hashes cached, which matters for large images.
    files: Mutex<HashMap<FileId, (Option<SystemTime>, Bytes)>>,
}

impl BookWorld {
    pub fn new(root: PathBuf) -> Self {
        BookWorld {
            root,
            library: LazyHash::new(Library::builder().build()),
            main: Source::new(project_file("/.wit/main.typ"), String::new()),
            config_id: project_file("/.wit/config.json"),
            config: Bytes::new(Vec::<u8>::new()),
            files: Mutex::new(HashMap::new()),
        }
    }

    /// Set the main source (incrementally reparsed) and the config JSON.
    pub fn update(&mut self, main: &str, config: Vec<u8>) {
        self.main.replace(main);
        if self.config.as_slice() != config.as_slice() {
            self.config = Bytes::new(config);
        }
    }

    pub fn compile(&self) -> Result<PagedDocument, Vec<SourceDiagnostic>> {
        let result = typst::compile::<PagedDocument>(self).output;
        // Bound memory used by memoization across many edits.
        comemo::evict(10);
        result.map_err(|errs| errs.into_iter().filter(|d| d.severity == Severity::Error).collect())
    }

    /// Human-readable errors, pointing at Markdown lines when the error is in
    /// the converted content.
    pub fn describe(&self, errs: &[SourceDiagnostic], md: &str, conv: &Converted, content_at: usize) -> String {
        let root = self.root.display().to_string();
        errs.iter()
            .map(|d| {
                // Errors inside template functions (e.g. a missing image) carry a
                // trace of call sites; the first one in the content is the culprit.
                let in_content = std::iter::once(d.span)
                    .chain(d.trace.iter().map(|t| t.span.into()))
                    .filter(|sp| sp.id() == Some(self.main.id()))
                    .filter_map(|sp| self.range(sp))
                    .find(|r| r.start >= content_at);
                let msg = d.message.replace(&root, "");
                let mut msg = match in_content {
                    Some(r) => format!("Line {}: {msg}", conv.md_line(md, r.start - content_at)),
                    None => format!("Template: {msg}"),
                };
                for h in &d.hints {
                    msg.push_str(&format!("\n  hint: {}", h.v));
                }
                msg
            })
            .collect::<Vec<_>>()
            .join("\n")
    }

    fn read(&self, id: FileId) -> FileResult<Bytes> {
        if id == self.config_id {
            return Ok(self.config.clone());
        }
        let path = id.vpath().realize(&self.root).map_err(FileError::Realize)?;
        let mtime = std::fs::metadata(&path).and_then(|m| m.modified()).ok();
        let mut files = self.files.lock().unwrap();
        if let Some((t, bytes)) = files.get(&id) {
            if *t == mtime && mtime.is_some() {
                return Ok(bytes.clone());
            }
        }
        let data = std::fs::read(&path).map_err(|e| FileError::from_io(e, &path))?;
        let bytes = Bytes::new(data);
        files.insert(id, (mtime, bytes.clone()));
        Ok(bytes)
    }
}

impl World for BookWorld {
    fn library(&self) -> &LazyHash<Library> {
        &self.library
    }
    fn book(&self) -> &LazyHash<FontBook> {
        fonts().book()
    }
    fn main(&self) -> FileId {
        self.main.id()
    }
    fn source(&self, id: FileId) -> FileResult<Source> {
        if id == self.main.id() {
            return Ok(self.main.clone());
        }
        let bytes = self.read(id)?;
        let text = std::str::from_utf8(bytes.as_slice()).map_err(|_| FileError::InvalidUtf8)?;
        Ok(Source::new(id, text.into()))
    }
    fn file(&self, id: FileId) -> FileResult<Bytes> {
        self.read(id)
    }
    fn font(&self, index: usize) -> Option<Font> {
        fonts().font(index)
    }
    fn today(&self, _offset: Option<Duration>) -> Option<Datetime> {
        None
    }
}

/// `(markdown line, page number)` for every paragraph/heading marker.
pub fn line_pages(doc: &PagedDocument) -> Vec<(u32, u32)> {
    let intro = doc.introspector();
    let mut out: Vec<(u32, u32)> = intro
        .query(&Selector::Label(Label::new(PicoStr::intern("q")).unwrap()))
        .iter()
        .filter_map(|c| {
            let line = match c.get_by_name("value").ok()? {
                Value::Int(n) => n as u32,
                _ => return None,
            };
            let page = intro.page(c.location()?)?.get() as u32;
            Some((line, page))
        })
        .collect();
    out.sort();
    out
}
