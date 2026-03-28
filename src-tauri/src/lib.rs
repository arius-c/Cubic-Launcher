use tauri::Manager;

pub mod account_manager;
pub mod adoptium;
pub mod app_shell;
pub mod config_attribution;
mod database;
pub mod dependencies;
pub mod debug_trace;
pub mod editor_data;
pub mod instance_configs;
pub mod instance_mods;
pub mod java_runtime;
pub mod launch_command;
pub mod launch_preview;
mod launcher_paths;
pub mod loader_metadata;
pub mod microsoft_auth;
pub mod mod_cache;
pub mod modlist_assets;
pub mod minecraft_downloader;
pub mod modlist_manager;
pub mod modrinth;
pub mod offline_account;
pub mod process_streaming;
pub mod resolver;
pub mod rules;
pub mod token_storage;

#[cfg_attr(mobile, tauri::mobile_entry_point)]
pub fn run() {
    tauri::Builder::default()
        .setup(|app| {
            let launcher_root = app
                .path()
                .app_local_data_dir()
                .map_err(|error| std::io::Error::other(error.to_string()))?;
            let launcher_paths = launcher_paths::LauncherPaths::new(launcher_root);

            launcher_paths
                .create_required_directories()
                .map_err(|error| std::io::Error::other(error.to_string()))?;

            database::initialize_database(launcher_paths.database_path())
                .map_err(|error| std::io::Error::other(error.to_string()))?;

            app.manage(launcher_paths);

            Ok(())
        })
        .plugin(tauri_plugin_dialog::init())
        .plugin(tauri_plugin_opener::init())
        .invoke_handler(tauri::generate_handler![
            app_shell::load_shell_snapshot_command,
            app_shell::switch_active_account_command,
            app_shell::save_global_settings_command,
            app_shell::save_modlist_overrides_command,
            debug_trace::append_debug_trace_command,
            debug_trace::clear_debug_trace_command,
            editor_data::load_modlist_editor_command,
            editor_data::add_mod_rule_command,
            editor_data::delete_rules_command,
            editor_data::rename_rule_command,
            editor_data::reorder_rules_command,
            editor_data::save_alternative_order_command,
            editor_data::add_alternative_command,
            editor_data::add_nested_alternative_command,
            editor_data::remove_alternative_command,
            editor_data::save_incompatibilities_command,
            editor_data::save_rule_advanced_command,
            editor_data::save_advanced_batch_command,
            modlist_manager::create_modlist_command,
            modlist_manager::delete_modlist_command,
            modlist_manager::copy_local_jar_command,
            modlist_assets::load_modlist_presentation_command,
            modlist_assets::save_modlist_presentation_command,
            modlist_assets::load_modlist_groups_command,
            modlist_assets::save_modlist_groups_command,
            modlist_assets::export_modlist_command,
            modlist_assets::read_image_as_data_url_command,
            minecraft_downloader::fetch_minecraft_versions_command,
            minecraft_downloader::start_minecraft_predownload_command,
            launch_preview::start_launch_command
        ])
        .run(tauri::generate_context!())
        .expect("error while running tauri application");
}
