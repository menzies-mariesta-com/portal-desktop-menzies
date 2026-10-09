//! Unlink previously copied gpclient binaries before Tauri resource copy.
//! A connected elevated `gpclient` keeps the target binary mapped; overwriting
//! it fails with ETXTBSY (os error 26). Removing the path frees the name while
//! the running process keeps its inode until exit.

use std::path::{Path, PathBuf};

fn unlink_if_present(path: &Path) {
    if path.is_file() {
        let _ = std::fs::remove_file(path);
    }
}

fn unlink_gp_bins_under(root: &Path) {
    if !root.is_dir() {
        return;
    }
    let Ok(entries) = std::fs::read_dir(root) else {
        return;
    };
    for entry in entries.flatten() {
        let path = entry.path();
        if path.is_dir() {
            unlink_gp_bins_under(&path);
            continue;
        }
        let name = path.file_name().and_then(|n| n.to_str()).unwrap_or("");
        if name == "gpclient" || name == "gpauth" || name == "gpclient.exe" || name == "gpauth.exe"
        {
            unlink_if_present(&path);
        }
    }
}

fn candidate_resource_roots(manifest: &Path) -> Vec<PathBuf> {
    let mut roots = Vec::new();
    let target_dir = std::env::var_os("CARGO_TARGET_DIR")
        .map(PathBuf::from)
        .unwrap_or_else(|| manifest.join("target"));
    let profile = std::env::var("PROFILE").unwrap_or_else(|_| "debug".into());

    roots.push(target_dir.join(&profile).join("gpclient"));
    if let Ok(triple) = std::env::var("TARGET") {
        roots.push(target_dir.join(&triple).join(&profile).join("gpclient"));
    }
    roots
}

fn main() {
    let manifest =
        PathBuf::from(std::env::var("CARGO_MANIFEST_DIR").unwrap_or_else(|_| ".".into()));
    for root in candidate_resource_roots(&manifest) {
        unlink_gp_bins_under(&root);
    }

    tauri_build::build()
}
