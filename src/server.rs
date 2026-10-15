//! The HTTP API and static web UI, shared by the `wit` web server and the
//! desktop app (which runs it in-process on a private localhost port).

use axum::{
    Json, Router,
    body::Body,
    extract::{Path, State},
    http::{StatusCode, header},
    response::{IntoResponse, Response},
    routing::get,
};
use crate::book::{self, Book, PageLook, SaveError};
use crate::library::Library;
use crate::{engine, export, settings};
use serde::Deserialize;
use serde_json::json;
use std::{path::PathBuf, sync::Arc};
use tokio::sync::Mutex;
use tower_http::services::{ServeDir, ServeFile};

/// Preview resolution. High enough to read comfortably when zoomed.
const PREVIEW_PPI: u32 = 110;

/// Shows a native "choose folder" dialog, starting in the given folder.
/// Blocking; the desktop app provides one, the web server does not.
pub type FolderPicker = Arc<dyn Fn(Option<PathBuf>) -> Option<PathBuf> + Send + Sync>;

/// Shows a native "save as" dialog for the suggested file name. Blocking.
pub type FileSaver = Arc<dyn Fn(String) -> Option<PathBuf> + Send + Sync>;

/// Opens a file in the app the system uses for it (Preview, Word…).
pub type FileOpener = Arc<dyn Fn(PathBuf) + Send + Sync>;

struct App {
    lib: Library,
    /// How preview pages look, set by the app to match its theme.
    look: std::sync::Mutex<PageLook>,
    picker: Option<FolderPicker>,
    saver: Option<FileSaver>,
    opener: Option<FileOpener>,
}

type Books = Arc<App>;

/// Where Wit finds its books and its own files.
pub struct Config {
    /// `wit.json`, listing the books. Relative book dirs resolve against its
    /// folder. Created when the first book is opened, if missing.
    pub workspace: PathBuf,
    /// Fonts preferred over system fonts.
    pub fonts: PathBuf,
    /// The built web UI (`web/dist`).
    pub web: PathBuf,
    pub pick_folder: Option<FolderPicker>,
    /// The desktop app saves PDFs and exports through a native dialog and
    /// opens them itself, instead of handing them to the browser.
    pub save_file: Option<FileSaver>,
    pub open_file: Option<FileOpener>,
}

/// Load the workspace and build the app. Call once per process (fonts are global).
pub fn router(cfg: Config) -> anyhow::Result<Router> {
    let books: Books = Arc::new(App { lib: Library::load(cfg.workspace)?, look: Default::default(), picker: cfg.pick_folder, saver: cfg.save_file, opener: cfg.open_file });
    engine::init_fonts(&cfg.fonts);

    let web = &cfg.web;
    Ok(Router::new()
        .route("/api/app", get(get_app))
        .route("/api/pick-folder", axum::routing::post(pick_folder))
        .route("/api/books", get(list_books))
        .route("/api/fonts", get(list_fonts))
        .route("/api/view", axum::routing::post(set_view))
        .route("/api/books/open", axum::routing::post(open_book))
        .route("/api/books/create", axum::routing::post(create_book))
        .route("/api/books/{id}", get(get_book))
        .route("/api/books/{id}/source", axum::routing::put(put_source))
        .route("/api/books/{id}/settings", get(get_settings).put(put_settings))
        .route("/api/books/{id}/history", get(get_history).post(name_revision))
        .route("/api/books/{id}/history/{file}", get(get_snapshot))
        .route("/api/books/{id}/history/{file}/restore", axum::routing::post(restore_snapshot))
        .route("/api/books/{id}/preview", get(get_preview))
        .route("/api/books/{id}/pages/{file}", get(get_page))
        .route("/api/books/{id}/pdf/{kind}", get(get_pdf))
        .route("/api/books/{id}/export/{format}", get(get_export))
        .route("/api/books/{id}/save/{kind}", axum::routing::post(save_file))
        .route("/api/books/{id}/asset/{*path}", get(get_asset))
        .route("/api/{*rest}", axum::routing::any(|| async { err(StatusCode::NOT_FOUND, "no such endpoint") }))
        .fallback_service(ServeDir::new(web).fallback(ServeFile::new(web.join("index.html"))))
        .layer(axum::middleware::from_fn(check_host))
        .with_state(books))
}

/// Refuse requests addressed to a domain name. A web page can point a domain
/// it controls at 127.0.0.1 (DNS rebinding) and then use this API as if it
/// were Wit, reading and changing books. Such requests carry that domain in
/// `Host`; Wit's own are addressed to `localhost` or an IP address, including
/// a LAN address when started with `WIT_HOST=0.0.0.0`.
async fn check_host(req: axum::extract::Request, next: axum::middleware::Next) -> Response {
    let host = req.headers().get(header::HOST).and_then(|h| h.to_str().ok());
    if host.is_some_and(host_allowed) {
        next.run(req).await
    } else {
        err(StatusCode::FORBIDDEN, "Wit only answers requests addressed to localhost or an IP address")
    }
}

fn host_allowed(host: &str) -> bool {
    let name = match host.strip_prefix('[') {
        // [::1]:7700
        Some(rest) => return rest.split_once(']').is_some_and(|(ip, _port)| ip.parse::<std::net::Ipv6Addr>().is_ok()),
        None => host.rsplit_once(':').map_or(host, |(name, _port)| name),
    };
    name.eq_ignore_ascii_case("localhost") || name.parse::<std::net::Ipv4Addr>().is_ok()
}

#[cfg(test)]
mod tests {
    use super::host_allowed;

    #[test]
    fn host_check() {
        for ok in ["localhost", "localhost:7700", "LOCALHOST:7700", "127.0.0.1:7700", "192.168.1.20:7700", "[::1]:7700", "[::1]"] {
            assert!(host_allowed(ok), "{ok}");
        }
        for bad in ["evil.example:7700", "evil.example", "localhost.evil.example", "127.0.0.1.nip.io:7700", "[::1", ""] {
            assert!(!host_allowed(bad), "{bad}");
        }
    }
}

fn err(status: StatusCode, msg: impl ToString) -> Response {
    (status, Json(json!({ "error": msg.to_string() }))).into_response()
}

fn find(books: &Books, id: &str) -> Result<Arc<Mutex<Book>>, Response> {
    books.lib.get(id).ok_or_else(|| err(StatusCode::NOT_FOUND, "no such book"))
}

async fn list_books(State(books): State<Books>) -> Response {
    let mut out = vec![];
    for slot in books.lib.list() {
        let b = slot.book.lock().await;
        let s = b.settings().ok();
        let title = s.as_ref().map(|s| s.title.clone()).unwrap_or_else(|| slot.id.clone());
        let kind = s.map(|s| s.document_type).unwrap_or_else(|| "book".into());
        out.push(json!({ "id": slot.id, "title": title, "dir": b.dir(), "kind": kind }));
    }
    Json(out).into_response()
}

/// The fonts Page setup offers: Wit's open fonts first, then every other
/// family on this computer (including any put in Wit's fonts folder).
async fn list_fonts() -> Response {
    Json(json!({ "open": engine::OPEN_FONTS.iter().map(|(f, note)| json!({ "family": f, "note": note })).collect::<Vec<_>>(),
                 "installed": engine::other_families() }))
        .into_response()
}

#[derive(Deserialize)]
struct ViewReq {
    /// "print", "dark" or "oled".
    pages: String,
}

/// The app's theme changed: draw preview pages to match (PDFs are unaffected).
async fn set_view(State(books): State<Books>, Json(req): Json<ViewReq>) -> Response {
    match PageLook::parse(&req.pages) {
        Some(look) => {
            *books.look.lock().unwrap() = look;
            Json(json!({ "pages": req.pages })).into_response()
        }
        None => err(StatusCode::BAD_REQUEST, "pages must be print, dark or oled"),
    }
}

fn look(books: &Books) -> PageLook {
    *books.look.lock().unwrap()
}

async fn get_app(State(books): State<Books>) -> Response {
    Json(json!({
        "native_picker": books.picker.is_some(),
        "native_save": books.saver.is_some(),
        "default_parent": books.lib.default_parent(),
        "sep": std::path::MAIN_SEPARATOR_STR,
    }))
    .into_response()
}

#[derive(Deserialize)]
struct PickReq {
    start: Option<PathBuf>,
}

/// Show the native folder dialog. `path` is null if the person cancelled.
async fn pick_folder(State(books): State<Books>, Json(req): Json<PickReq>) -> Response {
    let Some(picker) = books.picker.clone() else {
        return err(StatusCode::NOT_IMPLEMENTED, "no folder dialog here; type the path");
    };
    let start = req.start.or_else(|| books.lib.default_parent());
    match tokio::task::spawn_blocking(move || picker(start)).await {
        Ok(path) => Json(json!({ "path": path })).into_response(),
        Err(e) => err(StatusCode::INTERNAL_SERVER_ERROR, e),
    }
}

#[derive(Deserialize)]
struct OpenReq {
    dir: PathBuf,
}

async fn open_book(State(books): State<Books>, Json(req): Json<OpenReq>) -> Response {
    match books.lib.open(&req.dir) {
        Ok(id) => Json(json!({ "id": id })).into_response(),
        Err(e) => err(StatusCode::UNPROCESSABLE_ENTITY, format!("{e:#}")),
    }
}

#[derive(Deserialize)]
struct CreateReq {
    dir: PathBuf,
    title: String,
    #[serde(default)]
    author: String,
    /// "book", "essay", "screenplay" or "play".
    #[serde(default = "default_kind")]
    kind: String,
}
fn default_kind() -> String {
    "book".into()
}

async fn create_book(State(books): State<Books>, Json(req): Json<CreateReq>) -> Response {
    match books.lib.create(&req.dir, &req.title, &req.author, &req.kind) {
        Ok(id) => Json(json!({ "id": id })).into_response(),
        Err(e) => err(StatusCode::UNPROCESSABLE_ENTITY, format!("{e:#}")),
    }
}

async fn get_book(State(books): State<Books>, Path(id): Path<String>) -> Response {
    let book = match find(&books, &id) {
        Ok(b) => b,
        Err(r) => return r,
    };
    let b = book.lock().await;
    let source = match b.read_source() {
        Ok(s) => s,
        Err(e) => return err(StatusCode::INTERNAL_SERVER_ERROR, format!("{e:#}")),
    };
    let settings = b.settings().map_err(|e| format!("{e:#}"));
    Json(json!({
        "id": id,
        "source": source,
        "hash": book::hash(&source),
        "settings": settings.as_ref().ok(),
        "settings_error": settings.as_ref().err(),
        "images": b.list_images(),
        "revision": b.history.latest(),
    }))
    .into_response()
}

#[derive(Deserialize)]
struct SaveReq {
    content: String,
    base_hash: String,
}

/// Save, then re-render the preview so the client gets fresh pages in one trip.
async fn put_source(State(books): State<Books>, Path(id): Path<String>, Json(req): Json<SaveReq>) -> Response {
    let book = match find(&books, &id) {
        Ok(b) => b,
        Err(r) => return r,
    };
    let mut b = book.lock().await;
    match b.save_source(&req.content, &req.base_hash) {
        Ok(hash) => {
            let preview = b.render_preview(PREVIEW_PPI, look(&books)).await;
            Json(json!({ "hash": hash, "preview": preview, "revision": b.history.latest() })).into_response()
        }
        Err(SaveError::Conflict(disk_hash)) => (
            StatusCode::CONFLICT,
            Json(json!({ "error": "The file was changed outside Wit.", "disk_hash": disk_hash })),
        )
            .into_response(),
        Err(SaveError::Io(e)) => err(StatusCode::INTERNAL_SERVER_ERROR, format!("{e:#}")),
    }
}

async fn get_settings(State(books): State<Books>, Path(id): Path<String>) -> Response {
    let book = match find(&books, &id) {
        Ok(b) => b,
        Err(r) => return r,
    };
    let b = book.lock().await;
    match b.settings() {
        Ok(s) => Json(json!({
            "settings": s,
            "presets": settings::PRESETS,
            "styles": settings::style_presets(),
            "images": b.image_checks().unwrap_or_default(),
        }))
        .into_response(),
        Err(e) => err(StatusCode::UNPROCESSABLE_ENTITY, format!("{e:#}")),
    }
}

/// Save settings to book.yaml, then re-render so the client sees the result.
async fn put_settings(
    State(books): State<Books>,
    Path(id): Path<String>,
    Json(changes): Json<serde_json::Map<String, serde_json::Value>>,
) -> Response {
    let book = match find(&books, &id) {
        Ok(b) => b,
        Err(r) => return r,
    };
    let mut b = book.lock().await;
    match b.save_settings(&changes) {
        Ok(s) => {
            let preview = b.render_preview(PREVIEW_PPI, look(&books)).await;
            Json(json!({ "settings": s, "images": b.image_checks().unwrap_or_default(), "preview": preview }))
                .into_response()
        }
        Err(e) => err(StatusCode::UNPROCESSABLE_ENTITY, format!("{e:#}")),
    }
}

async fn get_history(State(books): State<Books>, Path(id): Path<String>) -> Response {
    match find(&books, &id) {
        Ok(b) => Json(b.lock().await.history.entries()).into_response(),
        Err(r) => r,
    }
}

#[derive(Deserialize)]
struct NameReq {
    name: String,
    base_hash: String,
}

/// Freeze the working draft as the next named revision.
async fn name_revision(State(books): State<Books>, Path(id): Path<String>, Json(req): Json<NameReq>) -> Response {
    let book = match find(&books, &id) {
        Ok(b) => b,
        Err(r) => return r,
    };
    match book.lock().await.name_revision(&req.name, &req.base_hash) {
        Ok(rev) => Json(json!({ "revision": rev })).into_response(),
        Err(SaveError::Conflict(_)) => err(StatusCode::CONFLICT, "The file was changed outside Wit."),
        Err(SaveError::Io(e)) => err(StatusCode::INTERNAL_SERVER_ERROR, format!("{e:#}")),
    }
}

async fn get_snapshot(State(books): State<Books>, Path((id, file)): Path<(String, String)>) -> Response {
    let book = match find(&books, &id) {
        Ok(b) => b,
        Err(r) => return r,
    };
    match book.lock().await.history.read(&file) {
        Ok(text) => Json(json!({ "text": text })).into_response(),
        Err(e) => err(StatusCode::NOT_FOUND, format!("{e:#}")),
    }
}

#[derive(Deserialize)]
struct RestoreReq {
    base_hash: String,
}

async fn restore_snapshot(
    State(books): State<Books>,
    Path((id, file)): Path<(String, String)>,
    Json(req): Json<RestoreReq>,
) -> Response {
    let book = match find(&books, &id) {
        Ok(b) => b,
        Err(r) => return r,
    };
    let mut b = book.lock().await;
    match b.restore(&file, &req.base_hash) {
        Ok(text) => {
            let preview = b.render_preview(PREVIEW_PPI, look(&books)).await;
            Json(json!({ "source": text, "hash": book::hash(&text), "preview": preview, "revision": b.history.latest() }))
                .into_response()
        }
        Err(SaveError::Conflict(_)) => err(StatusCode::CONFLICT, "The file was changed outside Wit."),
        Err(SaveError::Io(e)) => err(StatusCode::INTERNAL_SERVER_ERROR, format!("{e:#}")),
    }
}

async fn get_preview(State(books): State<Books>, Path(id): Path<String>) -> Response {
    let book = match find(&books, &id) {
        Ok(b) => b,
        Err(r) => return r,
    };
    let mut b = book.lock().await;
    Json(b.render_preview(PREVIEW_PPI, look(&books)).await).into_response()
}

fn safe_name(name: &str) -> bool {
    !name.is_empty() && !name.contains("..") && !name.starts_with('/') && !name.contains('\\')
}

async fn send_file(path: PathBuf, content_type: &str, extra: Option<(header::HeaderName, String)>) -> Response {
    match tokio::fs::read(&path).await {
        Ok(bytes) => {
            let mut r = Response::builder()
                .header(header::CONTENT_TYPE, content_type)
                .header(header::CACHE_CONTROL, "private, max-age=31536000, immutable");
            if let Some((k, v)) = extra {
                r = r.header(k, v);
            }
            r.body(Body::from(bytes)).unwrap()
        }
        Err(_) => err(StatusCode::NOT_FOUND, "not found"),
    }
}

async fn get_page(State(books): State<Books>, Path((id, file)): Path<(String, String)>) -> Response {
    let book = match find(&books, &id) {
        Ok(b) => b,
        Err(r) => return r,
    };
    if !safe_name(&file) || file.contains('/') {
        return err(StatusCode::BAD_REQUEST, "bad name");
    }
    match book.lock().await.page_png(&file) {
        Some(png) => Response::builder()
            .header(header::CONTENT_TYPE, "image/png")
            .header(header::CACHE_CONTROL, "private, max-age=31536000, immutable")
            .body(Body::from(png))
            .unwrap(),
        None => err(StatusCode::NOT_FOUND, "page not rendered"),
    }
}

async fn get_asset(State(books): State<Books>, Path((id, path)): Path<(String, String)>) -> Response {
    let book = match find(&books, &id) {
        Ok(b) => b,
        Err(r) => return r,
    };
    if !safe_name(&path) || !path.starts_with("images/") {
        return err(StatusCode::BAD_REQUEST, "bad path");
    }
    let full = book.lock().await.dir().join(&path);
    let ct = if path.to_ascii_lowercase().ends_with(".png") { "image/png" } else { "image/jpeg" };
    send_file(full, ct, None).await
}

/// `kind` is `print` (white pages, for the printer) or `proof` (as configured).
/// A file made from the book: "print" or "proof" PDF, "docx" or "odt".
struct Made {
    bytes: Vec<u8>,
    mime: &'static str,
    /// e.g. "Café Noir.docx"
    name: String,
}

async fn make_file(books: &Books, id: &str, kind: &str) -> Result<Made, Response> {
    let book = find(books, id)?;
    let mut b = book.lock().await;
    let settings = b.settings().map_err(|e| err(StatusCode::UNPROCESSABLE_ENTITY, format!("{e:#}")))?;
    let title = settings.title.trim();
    let base: String = if title.is_empty() { id.to_string() } else { title.replace(['/', '\\', ':', '"'], "-") };
    if let Some(format) = export::Format::parse(kind) {
        if book::is_script(&settings.document_type) {
            return Err(err(StatusCode::UNPROCESSABLE_ENTITY, "Word and OpenDocument files are for books and essays; scripts can't be exported yet."));
        }
        let source = b.read_source().map_err(|e| err(StatusCode::INTERNAL_SERVER_ERROR, format!("{e:#}")))?;
        let doc = export::Book { title: &settings.title, author: &settings.author, lang: &settings.lang, source: &source, dir: b.dir() };
        let bytes = export::export(&doc, format).map_err(|e| err(StatusCode::INTERNAL_SERVER_ERROR, format!("{e:#}")))?;
        return Ok(Made { bytes, mime: format.mime(), name: format!("{base}.{}", format.ext()) });
    }
    let print = match kind {
        "print" => true,
        "proof" => false,
        _ => return Err(err(StatusCode::NOT_FOUND, "use print, proof, docx or odt")),
    };
    let bytes = b.render_pdf(print).await.map_err(|e| err(StatusCode::UNPROCESSABLE_ENTITY, format!("{e:#}")))?;
    Ok(Made { bytes, mime: "application/pdf", name: format!("{base} - {}.pdf", if print { "print" } else { "proof" }) })
}

/// Download a Word or OpenDocument file (in a browser).
async fn get_export(State(books): State<Books>, Path((id, format)): Path<(String, String)>) -> Response {
    if export::Format::parse(&format).is_none() {
        return err(StatusCode::NOT_FOUND, "use docx or odt");
    }
    match make_file(&books, &id, &format).await {
        Ok(m) => Response::builder()
            .header(header::CONTENT_TYPE, m.mime)
            .header(header::CONTENT_DISPOSITION, content_disposition("attachment", &m.name))
            .header(header::CACHE_CONTROL, "no-store")
            .body(Body::from(m.bytes))
            .unwrap(),
        Err(r) => r,
    }
}

/// `filename` for plain ASCII, `filename*` for the real name ("Café Noir.docx").
fn content_disposition(kind: &str, name: &str) -> String {
    let ascii: String = name.chars().map(|c| if c.is_ascii() && c != '"' { c } else { '_' }).collect();
    let encoded: String = name
        .bytes()
        .map(|b| if b.is_ascii_alphanumeric() || b".-_ ".contains(&b) { (b as char).to_string().replace(' ', "%20") } else { format!("%{b:02X}") })
        .collect();
    format!("{kind}; filename=\"{ascii}\"; filename*=UTF-8''{encoded}")
}

/// Desktop app: make the file, ask where to save it, save it, and open PDFs
/// in the system viewer. `path` is null if the person cancelled.
async fn save_file(State(books): State<Books>, Path((id, kind)): Path<(String, String)>) -> Response {
    let Some(saver) = books.saver.clone() else {
        return err(StatusCode::NOT_IMPLEMENTED, "no save dialog here");
    };
    let made = match make_file(&books, &id, &kind).await {
        Ok(m) => m,
        Err(r) => return r,
    };
    let name = made.name.clone();
    let Ok(Some(path)) = tokio::task::spawn_blocking(move || saver(name)).await else {
        return Json(json!({ "path": null })).into_response();
    };
    if let Err(e) = std::fs::write(&path, &made.bytes) {
        return err(StatusCode::INTERNAL_SERVER_ERROR, format!("saving {}: {e}", path.display()));
    }
    if made.mime == "application/pdf" {
        if let Some(open) = books.opener.clone() {
            open(path.clone());
        }
    }
    Json(json!({ "path": path })).into_response()
}

async fn get_pdf(State(books): State<Books>, Path((id, kind)): Path<(String, String)>) -> Response {
    let book = match find(&books, &id) {
        Ok(b) => b,
        Err(r) => return r,
    };
    let print = match kind.as_str() {
        "print" => true,
        "proof" => false,
        _ => return err(StatusCode::NOT_FOUND, "use print or proof"),
    };
    let mut b = book.lock().await;
    match b.render_pdf(print).await {
        Ok(pdf) => Response::builder()
            .header(header::CONTENT_TYPE, "application/pdf")
            .header(header::CONTENT_DISPOSITION, format!("inline; filename=\"{id}-{kind}.pdf\""))
            .header(header::CACHE_CONTROL, "no-store")
            .body(Body::from(pdf))
            .unwrap(),
        Err(e) => err(StatusCode::UNPROCESSABLE_ENTITY, format!("{e:#}")),
    }
}
