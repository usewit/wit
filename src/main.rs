//! `wit`: the editor as a local web server (http://localhost:7700).

use std::path::PathBuf;

#[tokio::main]
async fn main() -> anyhow::Result<()> {
    tracing_subscriber::fmt::init();
    let root = PathBuf::from(env!("CARGO_MANIFEST_DIR"));
    let workspace = env("WORKSPACE").map(PathBuf::from).unwrap_or_else(|| {
        // A fresh checkout starts with the sample books, essay and screenplay. wit.json
        // is this computer's own list and is not committed.
        let ws = root.join("wit.json");
        if !ws.exists() && !root.join("quire.json").exists() {
            let _ = std::fs::copy(root.join("examples/workspace.json"), &ws);
        }
        ws
    });
    let cfg = wit::Config {
        workspace,
        fonts: root.join("fonts"),
        web: root.join("web/dist"),
        pick_folder: None,
        save_file: None,
        open_file: None,
    };
    let app = wit::router(cfg)?;

    let port = env("PORT").unwrap_or_else(|| "7700".into());
    // Only this machine by default: the API reads and writes the books.
    // WIT_HOST=0.0.0.0 opens it to the network, for example to use a tablet.
    let host = env("HOST").unwrap_or_else(|| "127.0.0.1".into());
    let listener = tokio::net::TcpListener::bind(format!("{host}:{port}")).await?;
    println!("Wit running at http://localhost:{port} (listening on {host})");
    axum::serve(listener, app).await?;
    Ok(())
}

/// `WIT_<name>`, or `QUIRE_<name>` from before the app was renamed.
fn env(name: &str) -> Option<String> {
    std::env::var(format!("WIT_{name}")).or_else(|_| std::env::var(format!("QUIRE_{name}"))).ok()
}
