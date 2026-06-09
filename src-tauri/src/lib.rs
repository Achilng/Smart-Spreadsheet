mod app;

pub mod db;
pub mod excel;
pub mod storage;

#[cfg_attr(mobile, tauri::mobile_entry_point)]
pub fn run() {
    tauri::Builder::default()
        .plugin(tauri_plugin_dialog::init())
        .setup(|app| {
            use tauri::Manager;

            let locator_path = app.path().app_config_dir()?.join("state.json");
            app.manage(app::AppRuntime::load(locator_path));
            Ok(())
        })
        .invoke_handler(tauri::generate_handler![
            app::commands::get_app_snapshot,
            app::commands::initialize_data_directory,
            app::commands::open_data_directory,
            app::commands::import_workbook,
        ])
        .run(tauri::generate_context!())
        .expect("error while running Smart Spreadsheet");
}
