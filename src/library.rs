//! The books Wit knows about, listed in the workspace file (`wit.json`).
//! Opening or creating a book adds it to the list and to the file.

use anyhow::{Context, Result, bail};
use serde::Deserialize;
use std::path::{Path, PathBuf};
use std::sync::{Arc, RwLock};
use tokio::sync::Mutex;

use crate::book::{Book, BookEntry, DOCUMENT_TYPES, default_font, is_script};

#[derive(Deserialize)]
struct Workspace {
    books: Vec<BookEntry>,
}

pub struct Library {
    workspace: PathBuf,
    /// In workspace order, so the book menu matches the file.
    books: RwLock<Vec<Slot>>,
}

#[derive(Clone)]
pub struct Slot {
    pub id: String,
    /// Canonical folder, to spot a book opened twice without locking it.
    dir: PathBuf,
    pub book: Arc<Mutex<Book>>,
}

impl Slot {
    fn new(entry: BookEntry) -> Self {
        Slot { id: entry.id.clone(), dir: entry.dir.clone(), book: Arc::new(Mutex::new(Book::new(entry))) }
    }
}

impl Library {
    /// Read the workspace file. A missing file is an empty library; it is
    /// created when the first book is added.
    pub fn load(workspace: PathBuf) -> Result<Self> {
        // The app was called Quire: carry its workspace file over once.
        let old = workspace.with_file_name("quire.json");
        if !workspace.exists() && old.is_file() {
            std::fs::rename(&old, &workspace).with_context(|| format!("renaming {}", old.display()))?;
        }
        let entries = match std::fs::read_to_string(&workspace) {
            Ok(text) => serde_json::from_str::<Workspace>(&text)
                .with_context(|| format!("parsing {}", workspace.display()))?
                .books,
            Err(e) if e.kind() == std::io::ErrorKind::NotFound => vec![],
            Err(e) => return Err(e).with_context(|| format!("reading {}", workspace.display())),
        };
        let ws_dir = workspace.parent().unwrap_or(Path::new(".")).to_path_buf();
        let mut books = vec![];
        for mut e in entries {
            if e.dir.is_relative() {
                e.dir = ws_dir.join(&e.dir);
            }
            e.dir = e.dir.canonicalize().with_context(|| format!("book folder {}", e.dir.display()))?;
            books.push(Slot::new(e));
        }
        Ok(Library { workspace, books: RwLock::new(books) })
    }

    pub fn get(&self, id: &str) -> Option<Arc<Mutex<Book>>> {
        self.books.read().unwrap().iter().find(|s| s.id == id).map(|s| s.book.clone())
    }

    pub fn list(&self) -> Vec<Slot> {
        self.books.read().unwrap().clone()
    }

    /// Where "New book" suggests putting a book: next to the last one opened,
    /// or the Documents folder for the very first.
    pub fn default_parent(&self) -> Option<PathBuf> {
        let books = self.books.read().unwrap();
        if let Some(last) = books.last() {
            return Some(strip_verbatim(last.dir.parent()?.to_path_buf()));
        }
        let home = PathBuf::from(std::env::var_os("USERPROFILE").or_else(|| std::env::var_os("HOME"))?);
        let docs = home.join("Documents");
        Some(if docs.is_dir() { docs } else { home })
    }

    /// Add an existing document folder. Returns its id (the existing one if it
    /// is already in the library).
    pub fn open(&self, dir: &Path) -> Result<String> {
        if !dir.is_dir() {
            bail!("There is no folder at {}", dir.display());
        }
        let Some(source) = find_source(dir) else {
            bail!("This folder has no story.md, essay.md or .fountain script, so it is not a Wit document. Use New to start one.");
        };
        self.add(dir, &source)
    }

    /// Make a new document folder with a starter text and settings, then add it.
    pub fn create(&self, dir: &Path, title: &str, author: &str, kind: &str) -> Result<String> {
        let title = title.trim();
        if title.is_empty() {
            bail!("Give it a title.");
        }
        let Some((_, source)) = DOCUMENT_TYPES.iter().find(|(k, _)| *k == kind) else {
            bail!("Unknown kind of document: {kind}");
        };
        if dir.exists() && std::fs::read_dir(dir)?.next().is_some() {
            bail!("{} already exists and is not empty. Choose another name or place.", dir.display());
        }
        if is_script(kind) {
            std::fs::create_dir_all(dir).with_context(|| format!("creating {}", dir.display()))?;
        } else {
            std::fs::create_dir_all(dir.join("images")).with_context(|| format!("creating {}", dir.display()))?;
        }
        let trim = if is_script(kind) { "8.5x11" } else if kind == "essay" { "6x9" } else { "a5" };
        let yaml = format!(
            "# Settings. Wit's Page setup edits this file.\ndocument_type: {kind}\nbook_title: {}author: {}trim_preset: {trim}\nbody_font_name: \"{}\"\n",
            serde_yaml::to_string(title)?,
            serde_yaml::to_string(author.trim())?,
            default_font(kind),
        );
        std::fs::write(dir.join("book.yaml"), yaml)?;
        std::fs::write(dir.join(source), starter(kind, title, author.trim()))?;
        self.add(dir, source)
    }

    fn add(&self, dir: &Path, source: &str) -> Result<String> {
        let canon = dir.canonicalize()?;
        let mut books = self.books.write().unwrap();
        if let Some(s) = books.iter().find(|s| s.dir == canon) {
            return Ok(s.id.clone());
        }
        let base = slug(&canon.file_name().unwrap_or_default().to_string_lossy());
        let mut id = base.clone();
        let mut n = 2;
        while books.iter().any(|s| s.id == id) {
            id = format!("{base}-{n}");
            n += 1;
        }
        self.save_entry(&id, &strip_verbatim(canon.clone()), source)?;
        let entry = BookEntry { id: id.clone(), dir: canon, source: source.into(), config: "book.yaml".into() };
        books.push(Slot::new(entry));
        Ok(id)
    }

    /// Append the book to the workspace file, keeping everything else in it.
    fn save_entry(&self, id: &str, dir: &Path, source: &str) -> Result<()> {
        let mut ws: serde_json::Value = match std::fs::read_to_string(&self.workspace) {
            Ok(t) => serde_json::from_str(&t)?,
            Err(_) => serde_json::json!({ "books": [] }),
        };
        let list = ws
            .get_mut("books")
            .and_then(|b| b.as_array_mut())
            .context("wit.json has no \"books\" list")?;
        let mut entry = serde_json::json!({ "id": id, "dir": dir });
        if source != "story.md" {
            entry["source"] = source.into();
        }
        list.push(entry);
        if let Some(parent) = self.workspace.parent() {
            std::fs::create_dir_all(parent)?;
        }
        std::fs::write(&self.workspace, serde_json::to_string_pretty(&ws)? + "\n")
            .with_context(|| format!("writing {}", self.workspace.display()))
    }
}

/// The text file a document folder is written in: one of the starter names,
/// or else the only .fountain or .md file there.
fn find_source(dir: &Path) -> Option<String> {
    if let Some((_, name)) = DOCUMENT_TYPES.iter().find(|(_, name)| dir.join(name).is_file()) {
        return Some(name.to_string());
    }
    let files: Vec<String> = std::fs::read_dir(dir)
        .ok()?
        .filter_map(|e| e.ok()?.file_name().into_string().ok())
        .collect();
    for ext in [".fountain", ".md"] {
        let found: Vec<&String> = files.iter().filter(|f| f.to_ascii_lowercase().ends_with(ext)).collect();
        if found.len() == 1 {
            return Some(found[0].clone());
        }
    }
    None
}

/// The first text of a new document, showing how that kind is written.
fn starter(kind: &str, title: &str, author: &str) -> String {
    let by = if author.is_empty() { String::new() } else { format!("Author: {author}\n") };
    match kind {
        "essay" => "# Introduction\n\nStart writing here. A line starting with # is a section heading.\n".into(),
        "screenplay" => format!(
            "Title: {title}\nCredit: Written by\n{by}Draft date: First draft\n\n\
             FADE IN:\n\n\
             EXT. FOREST PATH - DAY\n\n\
             Tall trees, morning light. A young wolf, NORI, sniffs the air.\n\n\
             NORI\n(whispering)\nDo you hear that?\n\n\
             CUT TO:\n"
        ),
        "play" => format!(
            "Title: {title}\n{by}\n\
             # ACT ONE\n\n\
             ## Scene 1\n\n\
             A kitchen, early morning. MARIA stands at the window.\n\n\
             MARIA\nIs anyone there?\n\n\
             She listens. Nothing.\n\n\
             > BLACKOUT.\n"
        ),
        _ => "# Chapter 1\n\nStart writing here.\n".into(),
    }
}

/// `canonicalize` on Windows gives `\\?\C:\...`; show and store the plain form.
fn strip_verbatim(p: PathBuf) -> PathBuf {
    let s = p.to_string_lossy();
    if let Some(rest) = s.strip_prefix(r"\\?\UNC\") {
        PathBuf::from(format!(r"\\{rest}"))
    } else if let Some(rest) = s.strip_prefix(r"\\?\") {
        PathBuf::from(rest)
    } else {
        p
    }
}

/// A book id from a folder name: lowercase letters, digits and dashes.
fn slug(name: &str) -> String {
    let mut out = String::new();
    for c in name.chars().flat_map(char::to_lowercase) {
        if c.is_ascii_alphanumeric() {
            out.push(c);
        } else if !out.ends_with('-') && !out.is_empty() {
            out.push('-');
        }
    }
    let out = out.trim_end_matches('-').to_string();
    if out.is_empty() { "book".into() } else { out }
}
