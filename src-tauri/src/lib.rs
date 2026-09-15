mod commands;

#[cfg_attr(mobile, tauri::mobile_entry_point)]
pub fn run() {
  tauri::Builder::default()
    .setup(|app| {
      if cfg!(debug_assertions) {
        app.handle().plugin(
          tauri_plugin_log::Builder::default()
            .level(log::LevelFilter::Info)
            .build(),
        )?;
      }
      Ok(())
    })
    .invoke_handler(tauri::generate_handler![
      commands::get_hive_key,
      commands::set_hive_key,
      commands::delete_hive_key,
      commands::list_hive_models,
      commands::check_services,
      commands::launch_service
    ])
    .run(tauri::generate_context!())
    .expect("error while running tauri application");
}
