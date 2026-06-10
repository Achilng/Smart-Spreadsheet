mod app;
mod images;

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
            app::commands::query_rows,
            app::commands::list_tags,
            app::commands::create_tag,
            app::commands::count_selected_rows,
            app::commands::add_tags_to_selection,
            app::commands::remove_tags_from_selection,
            app::commands::set_tags_for_row,
            app::commands::get_row_thumbnail,
            app::commands::get_row_preview,
            app::commands::export_workbook,
            app::commands::migrate_data_directory,
        ])
        .run(tauri::generate_context!())
        .expect("error while running Smart Spreadsheet");
}
