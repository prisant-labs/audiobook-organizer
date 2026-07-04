// src-tauri build script (v0.1.0 spine, Phase 1).
//
// Runs the Tauri build step that generates the context, embeds the config,
// and produces the permission/capability schemas consumed at runtime.
fn main() {
    tauri_build::build()
}
