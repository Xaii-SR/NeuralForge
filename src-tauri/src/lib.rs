mod ai;
mod core;
mod filesystem;
mod hardware;
mod terminal;

use ai::health::HealthRegistry;
use core::state::AppState;
use tauri::Manager;
use terminal::TerminalRegistry;

#[cfg_attr(mobile, tauri::mobile_entry_point)]
pub fn run() {
  tauri::Builder::default()
    .manage(AppState::default())
    .manage(TerminalRegistry::default())
    .manage(HealthRegistry::default())
    .plugin(tauri_plugin_dialog::init())
    .invoke_handler(tauri::generate_handler![
      filesystem::open_workspace,
      filesystem::read_dir,
      filesystem::read_file,
      filesystem::write_file,
      filesystem::create_file,
      filesystem::create_dir,
      filesystem::delete_path,
      filesystem::rename_path,
      terminal::spawn_shell,
      terminal::write_to_pty,
      terminal::resize_pty,
      terminal::close_pty,
      core::logging::get_recent_logs,
      core::logging::export_logs,
      hardware::get_hardware_info,
      ai::ollama_health_check,
      ai::list_models,
      ai::pull_model,
      ai::remove_model,
      ai::list_providers,
      ai::get_provider_health,
      ai::check_vram_for_model,
      ai::chat_with_model,
    ])
    .setup(|app| {
      let log_dir = app.path().app_log_dir()?;
      let guard = core::logging::init(&log_dir)?;
      app.manage(guard);
      tracing::info!(target: "core", event = "app_started", "NeuralForge started");
      Ok(())
    })
    .build(tauri::generate_context!())
    .expect("error while building tauri application")
    .run(|app_handle, event| {
      if let tauri::RunEvent::ExitRequested { .. } = event {
        let registry = app_handle.state::<TerminalRegistry>();
        terminal::kill_all(&registry);
      }
    });
}
