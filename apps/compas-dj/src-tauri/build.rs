use std::process::Command;
use std::time::{SystemTime, UNIX_EPOCH};

fn main() {
    tauri_build::build();

    // Short git SHA the binary was built from. Empty string if git isn't available
    // (tarball build) or this isn't a checkout — the UI falls back to "dev".
    let sha = Command::new("git")
        .args(["rev-parse", "--short", "HEAD"])
        .output()
        .ok()
        .filter(|o| o.status.success())
        .and_then(|o| String::from_utf8(o.stdout).ok())
        .map(|s| s.trim().to_string())
        .unwrap_or_default();
    println!("cargo:rustc-env=COMPAS_GIT_SHA={sha}");

    // Build timestamp as Unix seconds (UTC). Frontend formats it for display.
    let built_at = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map(|d| d.as_secs().to_string())
        .unwrap_or_default();
    println!("cargo:rustc-env=COMPAS_BUILD_TIMESTAMP={built_at}");

    // Re-run the build script whenever HEAD or the ref it points at moves, so the
    // baked-in SHA stays current without `cargo clean`.
    println!("cargo:rerun-if-changed=../.git/HEAD");
    println!("cargo:rerun-if-changed=../.git/refs");
}
