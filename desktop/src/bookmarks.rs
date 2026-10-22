//! Mac App Store build: the sandbox lets Wit into a folder only after the
//! person picks it in a dialog, and forgets that on quit. A security-scoped
//! bookmark for each picked folder, kept in `bookmarks.json` next to the
//! workspace file, brings the access back on the next launch. Books made
//! inside a picked folder are covered by its bookmark.
//!
//! Outside the sandbox (the notarized .dmg) this is harmless: the bookmarks
//! resolve and access was never restricted.

use std::collections::BTreeMap;
use std::path::{Path, PathBuf};

use base64::Engine;
use base64::engine::general_purpose::STANDARD;
use objc2::runtime::Bool;
use objc2_foundation::{NSData, NSString, NSURL, NSURLBookmarkCreationOptions, NSURLBookmarkResolutionOptions};

type Saved = BTreeMap<String, String>;

fn read(file: &Path) -> Saved {
    std::fs::read_to_string(file).ok().and_then(|t| serde_json::from_str(&t).ok()).unwrap_or_default()
}

fn bookmark(path: &Path) -> Option<String> {
    let url = NSURL::fileURLWithPath(&NSString::from_str(path.to_str()?));
    let data = url
        .bookmarkDataWithOptions_includingResourceValuesForKeys_relativeToURL_error(NSURLBookmarkCreationOptions::WithSecurityScope, None, None)
        .ok()?;
    Some(STANDARD.encode(data.to_vec()))
}

/// Keep a bookmark for a folder the person just picked.
pub fn remember(file: &Path, folder: &Path) {
    let Some(mark) = bookmark(folder) else { return };
    let mut saved = read(file);
    saved.insert(folder.to_string_lossy().into_owned(), mark);
    if let Some(dir) = file.parent() {
        let _ = std::fs::create_dir_all(dir);
    }
    let _ = std::fs::write(file, serde_json::to_string_pretty(&saved).unwrap_or_default());
}

/// At launch, before the workspace is read: get back into every remembered
/// folder. Access lasts until Wit quits. Bookmarks that no longer resolve
/// (folder deleted) are dropped; stale ones (folder moved) are renewed.
pub fn restore(file: &Path) {
    let saved = read(file);
    let mut kept = Saved::new();
    for (path, mark) in &saved {
        let Ok(bytes) = STANDARD.decode(mark) else { continue };
        let data = NSData::with_bytes(&bytes);
        let mut stale = Bool::NO;
        // SAFETY: `stale` is a valid pointer for the duration of the call.
        let Ok(url) = (unsafe {
            NSURL::URLByResolvingBookmarkData_options_relativeToURL_bookmarkDataIsStale_error(
                &data,
                NSURLBookmarkResolutionOptions::WithSecurityScope,
                None,
                &mut stale,
            )
        }) else {
            continue;
        };
        // SAFETY: plain method call on a resolved file URL; never stopped,
        // so the folder stays reachable while Wit runs.
        if !unsafe { url.startAccessingSecurityScopedResource() } {
            continue;
        }
        let now = url.path().map(|p| PathBuf::from(p.to_string())).unwrap_or_else(|| PathBuf::from(path));
        let mark = if stale.as_bool() { bookmark(&now).unwrap_or_else(|| mark.clone()) } else { mark.clone() };
        kept.insert(now.to_string_lossy().into_owned(), mark);
    }
    if kept != saved {
        let _ = std::fs::write(file, serde_json::to_string_pretty(&kept).unwrap_or_default());
    }
}
