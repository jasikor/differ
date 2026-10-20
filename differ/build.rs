//! Stamp the binary with the commit it was built from.

use std::path::{Path, PathBuf};
use std::process::Command;

fn main() {
    let manifest = std::env::var("CARGO_MANIFEST_DIR").unwrap_or_else(|_| ".".to_string());
    let hash =
        git(&manifest, &["rev-parse", "--short", "HEAD"]).unwrap_or_else(|| "unknown".to_string());
    let time =
        git(&manifest, &["log", "-1", "--format=%cI"]).unwrap_or_else(|| "unknown".to_string());

    println!("cargo:rustc-env=DIFFER_GIT_HASH={hash}");
    println!("cargo:rustc-env=DIFFER_COMMIT_TIME={time}");
    watch_git(&manifest);
}

fn git(dir: &str, args: &[&str]) -> Option<String> {
    let output = Command::new("git")
        .current_dir(dir)
        .args(args)
        .output()
        .ok()?;
    if !output.status.success() {
        return None;
    }
    let text = String::from_utf8(output.stdout).ok()?;
    let text = text.trim();
    (!text.is_empty()).then(|| text.to_string())
}

/// Re-run when the checked-out commit changes, but not on every build.
fn watch_git(manifest: &str) {
    println!("cargo:rerun-if-changed=build.rs");
    let Some(git_dir) = git(manifest, &["rev-parse", "--absolute-git-dir"]) else {
        return;
    };
    let git_dir = PathBuf::from(git_dir);
    watch(&git_dir.join("HEAD"));

    let Ok(head) = std::fs::read_to_string(git_dir.join("HEAD")) else {
        return;
    };
    let Some(reference) = head.strip_prefix("ref: ") else {
        return;
    };
    let common = git(manifest, &["rev-parse", "--git-common-dir"])
        .map(|dir| {
            let dir = PathBuf::from(dir);
            if dir.is_absolute() {
                dir
            } else {
                Path::new(manifest).join(dir)
            }
        })
        .unwrap_or_else(|| git_dir.clone());
    watch(&common.join(reference.trim()));
    watch(&common.join("packed-refs"));
}

fn watch(path: &Path) {
    if path.exists() {
        println!("cargo:rerun-if-changed={}", path.display());
    }
}
