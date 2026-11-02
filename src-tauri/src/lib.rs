// Learn more about Tauri commands at https://tauri.app/develop/calling-rust/

pub mod commands;
pub mod core;
pub mod db;

use std::sync::Arc;
use tauri::Manager;

use crate::commands::{
    AppState, add_download, cancel_download, delete_download, get_default_download_dir,
    list_downloads, pause_download, resume_download,
};
use crate::core::{DownloadConfig, DownloadEngine, NetworkClient};
use crate::db::repository::Database;

/// Returns a greeting message for the specified name.
pub fn format_greeting(name: &str) -> String {
    format!("Hello, {}! You've been greeted from Rust!", name)
}

#[tauri::command]
fn greet(name: &str) -> String {
    format_greeting(name)
}

#[cfg_attr(mobile, tauri::mobile_entry_point)]
pub fn run() {
    tauri::Builder::default()
        .plugin(tauri_plugin_opener::init())
        .setup(|app| {
            // Determine application data directory for local SQLite database
            let db_dir = app
                .path()
                .app_data_dir()
                .unwrap_or_else(|_| std::env::current_dir().unwrap_or_default());

            let _ = std::fs::create_dir_all(&db_dir);
            let db_path = db_dir.join("yf_download.db");

            let db = Database::open(&db_path).unwrap_or_else(|_| {
                Database::open_in_memory().expect("Failed to initialize in-memory SQLite fallback")
            });

            let client = NetworkClient::default();
            let config = DownloadConfig::default();
            let engine = Arc::new(DownloadEngine::with_db(client, config, db.clone()));

            // Recover any interrupted tasks from previous sessions
            let _ = engine.recover_interrupted_tasks();

            let state = AppState { engine, db };
            app.manage(state);

            Ok(())
        })
        .invoke_handler(tauri::generate_handler![
            greet,
            list_downloads,
            get_default_download_dir,
            add_download,
            pause_download,
            resume_download,
            cancel_download,
            delete_download
        ])
        .run(tauri::generate_context!())
        .expect("error while running tauri application");
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_greeting() {
        let result = format_greeting("World");
        assert_eq!(result, "Hello, World! You've been greeted from Rust!");
    }
}
