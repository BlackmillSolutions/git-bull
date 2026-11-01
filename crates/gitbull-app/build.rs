//! Embeds every translation file in `i18n/`, so that a language is added by
//! adding its file, without changing program code, and gives `git-bull.exe`
//! its icon on Windows.

use std::path::Path;

/// Embeds `packaging/icons/git-bull.ico` in the executable, which is the icon
/// that Explorer, the taskbar and the Start menu show for the file.
///
/// Only a Windows host does this: the packages are built there, and a build
/// for Windows from another system, as a check of the code does, then has no
/// icon rather than needing a resource compiler.
#[cfg(windows)]
fn embed_windows_icon(manifest: &Path) {
    if std::env::var("CARGO_CFG_TARGET_OS").as_deref() != Ok("windows") {
        return;
    }
    let icon = manifest.join("../../packaging/icons/git-bull.ico");
    println!("cargo:rerun-if-changed={}", icon.display());
    let mut resource = winresource::WindowsResource::new();
    resource.set_icon(icon.to_str().expect("a UTF-8 path to the icon"));
    resource
        .compile()
        .expect("the icon is embedded in the executable");
}

#[cfg(not(windows))]
fn embed_windows_icon(_manifest: &Path) {}

fn main() {
    // Read when the script runs, not when it is compiled: Cargo may reuse a
    // script compiled for another checkout of the same sources.
    let manifest = std::env::var("CARGO_MANIFEST_DIR").expect("Cargo sets the manifest folder");
    embed_windows_icon(Path::new(&manifest));
    let dir = Path::new(&manifest).join("i18n");
    println!("cargo:rerun-if-changed={}", dir.display());

    let mut languages: Vec<(String, String)> = std::fs::read_dir(&dir)
        .expect("the i18n folder exists")
        .map(|entry| entry.expect("readable i18n folder").path())
        .filter(|path| path.extension().is_some_and(|ext| ext == "ftl"))
        .map(|path| {
            let tag = path.file_stem().unwrap().to_string_lossy().into_owned();
            (tag, path.to_string_lossy().replace('\\', "/"))
        })
        .collect();
    languages.sort();

    let mut code =
        String::from("/// Every embedded translation, as (language tag, Fluent source).\n");
    code.push_str("pub const RESOURCES: &[(&str, &str)] = &[\n");
    for (tag, path) in &languages {
        code.push_str(&format!("    ({tag:?}, include_str!({path:?})),\n"));
    }
    code.push_str("];\n");

    let out = Path::new(&std::env::var("OUT_DIR").unwrap()).join("languages.rs");
    std::fs::write(out, code).expect("generated languages.rs");
}
