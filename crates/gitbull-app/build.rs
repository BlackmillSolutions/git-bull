//! Embeds every translation file in `i18n/`, so that a language is added by
//! adding its file, without changing program code.

use std::path::Path;

fn main() {
    // Read when the script runs, not when it is compiled: Cargo may reuse a
    // script compiled for another checkout of the same sources.
    let manifest = std::env::var("CARGO_MANIFEST_DIR").expect("Cargo sets the manifest folder");
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
