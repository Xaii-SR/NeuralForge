mod core;
mod filesystem;
mod terminal;

use core::state::AppState;
use tauri::Manager;
use terminal::TerminalRegistry;

#[cfg_attr(mobile, tauri::mobile_entry_point)]
pub fn run() {
  tauri::Builder::default()
    .manage(AppState::default())
    .manage(TerminalRegistry::default())
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
    ])
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
    .build(tauri::generate_context!())
    .expect("error while building tauri application")
    .run(|app_handle, event| {
      if let tauri::RunEvent::ExitRequested { .. } = event {
        let registry = app_handle.state::<TerminalRegistry>();
        terminal::kill_all(&registry);
      }
    });
}
