#![cfg_attr(test, allow(clippy::unwrap_used, clippy::expect_used))]

//! Tauri shell: thin command layer over `fd-core`.

mod commands;
mod events;
mod jobs;
mod overrides;

pub fn run() -> tauri::Result<()> {
    tauri::Builder::default()
        .plugin(tauri_plugin_dialog::init())
        .manage(jobs::Jobs::default())
        .invoke_handler(tauri::generate_handler![
            commands::list_profiles,
            commands::list_games,
            commands::describe_image,
            commands::generate,
            commands::stop_generation,
            commands::set_generation_paused,
            commands::load_document,
            commands::render_document,
            commands::inject,
            commands::cancel_injection,
        ])
        .run(tauri::generate_context!())
}
