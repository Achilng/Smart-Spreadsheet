mod app;
mod images;

pub mod db;
pub mod excel;
pub mod fsx;
pub mod pipeline;
pub mod storage;

#[cfg_attr(mobile, tauri::mobile_entry_point)]
pub fn run() {
    tauri::Builder::default()
        .plugin(tauri_plugin_dialog::init())
        .plugin(tauri_plugin_drag::init())
        .setup(|app| {
            use tauri::Manager;

            let locator_path = app.path().app_config_dir()?.join("state.json");
            let default_data_dir = app.path().app_local_data_dir()?.join("data");
            app.manage(app::AppRuntime::load(locator_path, default_data_dir));
            Ok(())
        })
        .invoke_handler(tauri::generate_handler![
            app::commands::get_app_snapshot,
            app::commands::reset_configuration,
            app::commands::reset_data,
            app::commands::initialize_data_directory,
            app::commands::open_data_directory,
            app::commands::set_rejected_images_directory,
            app::commands::import_images,
            app::commands::update_existing_images,
            app::commands::delete_rows,
            app::commands::undo_import_batch,
            app::commands::restore_mutable_row_states,
            app::commands::list_import_batches,
            app::commands::create_group,
            app::commands::restore_group,
            app::commands::rename_group,
            app::commands::delete_group,
            app::commands::delete_empty_groups,
            app::commands::list_groups,
            app::commands::assign_rows_to_group,
            app::commands::ungroup_rows,
            app::commands::get_group_members,
            app::commands::list_dedupe_clusters,
            app::commands::get_dedupe_cluster_members,
            app::commands::set_dedupe_alias,
            app::commands::list_distinct_artists,
            app::commands::row_ids_with_artists,
            app::commands::get_custom_artists,
            app::commands::set_custom_artists,
            app::commands::list_prompt_docs,
            app::commands::create_prompt_doc,
            app::commands::load_prompt_doc,
            app::commands::save_prompt_doc,
            app::commands::delete_prompt_doc,
            app::commands::import_prompt_doc_image_from_path,
            app::commands::import_prompt_doc_image_bytes,
            app::commands::suggest_groups,
            app::commands::update_positive_prompt,
            app::commands::update_character_prompt,
            app::commands::update_negative_prompt,
            app::commands::update_note,
            app::commands::find_replace_prompt,
            app::commands::prepend_artist,
            app::commands::query_rows,
            app::commands::get_rows_by_ids,
            app::commands::list_tags,
            app::commands::create_tag,
            app::commands::delete_tag,
            app::commands::count_selected_rows,
            app::commands::list_selection_tags,
            app::commands::selected_row_ids,
            app::commands::add_tags_to_selection,
            app::commands::remove_tags_from_selection,
            app::commands::set_tags_for_row,
            app::commands::get_row_thumbnail,
            app::commands::get_row_preview,
            app::commands::export_row_image,
            app::commands::export_xlsx,
            app::commands::inspect_zhihuiji_export_notes,
            app::commands::export_zhihuiji_json,
            app::commands::export_image_files,
            app::commands::inspect_zhihuiji_json,
            app::commands::dedupe_zhihuiji_json,
            app::commands::migrate_data_directory,
            app::commands::backfill_perceptual_hashes,
            app::commands::search_similar_images,
            app::commands::show_item_in_explorer,
            app::commands::open_rejected_images_directory,
            app::commands::prepare_file_drag,
            app::commands::get_row_vibe_status,
        ])
        .run(tauri::generate_context!())
        .expect("error while running Smart Spreadsheet");
}
