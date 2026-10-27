//! Assembles the single-page UI from `src/ui/` (see `build.rs` and `src/ui/manifest.json`).
//! The document is built once and served from memory, so there is one request and no disk access.
use std::sync::OnceLock;

mod files {
    include!(concat!(env!("OUT_DIR"), "/ui_bundle.rs"));
}

const INDEX: &str = include_str!("ui/index.html");

fn join(parts: &[(&str, &str)]) -> String {
    parts.iter().map(|(_, body)| *body).collect()
}

/// The full UI document: `index.html` with styles, pages and scripts inserted at its markers.
pub fn embedded_ui_html() -> &'static str {
    static DOC: OnceLock<String> = OnceLock::new();
    DOC.get_or_init(|| {
        INDEX
            .replacen("/*@STYLES*/\n", &join(files::STYLES), 1)
            .replacen("<!--@PAGES-->\n", &join(files::PAGES), 1)
            .replacen("/*@SCRIPTS*/\n", &join(files::SCRIPTS), 1)
    })
}

/// File names in load order, for tests.
pub fn ui_files() -> Vec<&'static str> {
    files::STYLES.iter().chain(files::PAGES).chain(files::SCRIPTS).map(|(name, _)| *name).collect()
}
