//! Wit as a desktop app: the same server as `wit`, run in-process on a
//! private localhost port, shown in a native window (WebView2 on Windows).

#![cfg_attr(not(debug_assertions), windows_subsystem = "windows")]

use std::path::PathBuf;
use std::sync::Arc;

use tauri::webview::NewWindowResponse;
use tauri::{App, Manager, WebviewUrl, WebviewWindowBuilder};
use tauri_plugin_dialog::{DialogExt, MessageDialogKind};
use tauri_plugin_opener::OpenerExt;
use url::Url;

#[cfg(target_os = "macos")]
mod bookmarks;

fn main() {
    tauri::Builder::default()
        .plugin(tauri_plugin_dialog::init())
        .plugin(tauri_plugin_opener::init())
        .setup(|app| {
            if let Err(e) = start(app) {
                let handle = app.handle().clone();
                app.dialog()
                    .message(format!("{e:#}"))
                    .title("Wit could not start")
                    .kind(MessageDialogKind::Error)
                    .show(move |_| handle.exit(1));
            }
            Ok(())
        })
        .run(tauri::generate_context!())
        .expect("error while running Wit");
}

fn start(app: &mut App) -> anyhow::Result<()> {
    let router = wit::router(config(app)?)?;

    // Port 0: the OS picks a free port, so this never clashes with `wit` on 7700.
    let listener = tauri::async_runtime::block_on(tokio::net::TcpListener::bind("127.0.0.1:0"))?;
    let origin = Url::parse(&format!("http://{}/", listener.local_addr()?))?;
    tauri::async_runtime::spawn(async move {
        if let Err(e) = axum::serve(listener, router).await {
            eprintln!("server stopped: {e}");
        }
    });

    let nav_origin = origin.clone();
    let nav_handle = app.handle().clone();
    let new_handle = app.handle().clone();
    WebviewWindowBuilder::new(app, "main", WebviewUrl::External(origin))
        .title("Wit")
        .inner_size(1400.0, 900.0)
        .min_inner_size(800.0, 500.0)
        .maximized(true)
        // Links that leave the editor open in the default browser instead.
        .on_navigation(move |url| {
            let ours = url.origin() == nav_origin.origin();
            if !ours {
                let _ = nav_handle.opener().open_url(url.as_str(), None::<&str>);
            }
            ours
        })
        // `target=_blank` links (Proof PDF, Print PDF) open in the default
        // browser, which can show and save PDFs. The server outlives the request.
        .on_new_window(move |url, _| {
            let _ = new_handle.opener().open_url(url.as_str(), None::<&str>);
            NewWindowResponse::Deny
        })
        .build()?;
    Ok(())
}

fn copy_dir(from: &std::path::Path, to: &std::path::Path) -> std::io::Result<()> {
    std::fs::create_dir_all(to)?;
    for entry in std::fs::read_dir(from)? {
        let entry = entry?;
        let target = to.join(entry.file_name());
        if entry.file_type()?.is_dir() {
            copy_dir(&entry.path(), &target)?;
        } else if entry.file_name() != "workspace.json" {
            std::fs::copy(entry.path(), target)?;
        }
    }
    Ok(())
}

/// Bundled resources sit next to the app; the workspace file lives in the
/// user's config folder unless `WIT_WORKSPACE` points elsewhere.
fn config(app: &App) -> anyhow::Result<wit::Config> {
    let res = app.path().resource_dir()?;
    let workspace = match std::env::var_os("WIT_WORKSPACE").or_else(|| std::env::var_os("QUIRE_WORKSPACE")) {
        Some(p) => PathBuf::from(p),
        None => {
            let dir = app.path().app_config_dir()?;
            let workspace = dir.join("wit.json");
            // Bring the book list over from the app's old name (app.quire.desktop).
            let old = dir.with_file_name("app.quire.desktop").join("quire.json");
            if !workspace.exists() && old.is_file() {
                std::fs::create_dir_all(&dir)?;
                std::fs::copy(&old, &workspace)?;
            }
            // First run: copy the bundled samples where they can be edited,
            // and list them. Their paths are relative to wit.json.
            let samples = res.join("examples");
            if !workspace.exists() && samples.join("workspace.json").is_file() {
                copy_dir(&samples, &dir.join("examples"))?;
                std::fs::copy(samples.join("workspace.json"), &workspace)?;
            }
            workspace
        }
    };
    // Sandboxed Mac app: reopen the folders picked in earlier sessions.
    #[cfg(target_os = "macos")]
    let marks = {
        let marks = workspace.with_file_name("bookmarks.json");
        bookmarks::restore(&marks);
        marks
    };
    let handle = app.handle().clone();
    let pick_folder: wit::FolderPicker = Arc::new(move |start: Option<PathBuf>| {
        let mut dialog = handle.dialog().file().set_title("Choose a folder");
        if let Some(window) = handle.get_webview_window("main") {
            dialog = dialog.set_parent(&window);
        }
        if let Some(dir) = start.filter(|d| d.is_dir()) {
            dialog = dialog.set_directory(dir);
        }
        let folder = dialog.blocking_pick_folder()?.into_path().ok()?;
        #[cfg(target_os = "macos")]
        bookmarks::remember(&marks, &folder);
        Some(folder)
    });
    let handle = app.handle().clone();
    let save_file: wit::FileSaver = Arc::new(move |name: String| {
        let mut dialog = handle.dialog().file().set_title("Save").set_file_name(&name);
        if let Some(window) = handle.get_webview_window("main") {
            dialog = dialog.set_parent(&window);
        }
        if let Some(ext) = std::path::Path::new(&name).extension().and_then(|e| e.to_str()) {
            let label = match ext {
                "pdf" => "PDF",
                "docx" => "Word document",
                "odt" => "OpenDocument text",
                _ => ext,
            };
            dialog = dialog.add_filter(label, &[ext]);
        }
        if let Some(dir) = handle.path().document_dir().ok().filter(|d| d.is_dir()) {
            dialog = dialog.set_directory(dir);
        }
        dialog.blocking_save_file()?.into_path().ok()
    });
    let handle = app.handle().clone();
    let open_file: wit::FileOpener = Arc::new(move |path: PathBuf| {
        let _ = handle.opener().open_path(path.to_string_lossy(), None::<&str>);
    });
    Ok(wit::Config {
        workspace,
        fonts: res.join("fonts"),
        web: res.join("web"),
        pick_folder: Some(pick_folder),
        save_file: Some(save_file),
        open_file: Some(open_file),
    })
}
