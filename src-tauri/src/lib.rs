mod agent;
mod ai;
mod bootstrap;
mod core;
mod governance;
mod database;
mod extensions;
mod filesystem;
mod hardware;
mod intelligence;
#[cfg(test)]
mod release_validation;
mod planning;
mod terminal;

use ai::benchmarks::BenchmarkDbState;
use ai::composer::ComposerSessionState;
use ai::health::HealthRegistry;
use core::state::AppState;
use database::DbState;
use tauri::Manager;
use terminal::TerminalRegistry;

#[cfg_attr(mobile, tauri::mobile_entry_point)]
pub fn run() {
  tauri::Builder::default()
    .manage(AppState::default())
    .manage(TerminalRegistry::default())
    .manage(HealthRegistry::default())
    .manage(DbState::default())
    .manage(BenchmarkDbState::default())
    .manage(ComposerSessionState::default())
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
      ai::get_context_for_query,
      ai::get_enriched_context,
      ai::save_preferences,
      ai::get_preferences,
      ai::estimate_cost_for_prompt,
      ai::run_model_benchmark,
      ai::get_benchmarks,
      ai::get_benchmark_for_model,
      ai::clear_response_cache,
      ai::auto_select_model,
      ai::dispatch_inline_refactor,
      ai::completion::get_ghost_text_prediction,
      ai::completion::get_prediction_with_fim,
      ai::completion::store_prediction_result,
      ai::completion::request_async_completion,
      ai::composer::initialize_composer_session,
      ai::composer::add_composer_file,
      ai::composer::remove_composer_file,
      ai::composer::send_composer_message,
      ai::composer::get_composer_session,
      ai::composer::execute_composer_command,
      database::index_workspace,
      database::search_workspace,
      database::resolve_file_reference,
      governance::create_requirement,
      governance::update_requirement,
      governance::set_requirement_status,
      governance::get_requirement,
      governance::list_requirements,
      governance::get_requirement_history,
      agent::create_and_plan_task,
      agent::create_and_plan_code_task,
      agent::approve_task,
      agent::reject_task,
      agent::list_agent_tasks,
      governance::get_ledger,
      governance::get_ledger_for_correlation,
      governance::verify_ledger,
      governance::get_evidence_for_task,
      governance::get_promotions_for_task,
      planning::plan_requirement_dag,
      planning::get_dag,
      planning::get_dag_runnable_tasks,
      intelligence::list_worker_profiles,
      intelligence::upsert_worker_profile,
      intelligence::delete_worker_profile,
      intelligence::refresh_worker_reliability,
      intelligence::match_workers,
      intelligence::retry_failed_task,
      intelligence::get_task_confidence,
      intelligence::get_task_report,
      extensions::list_extensions,
      extensions::set_extension_enabled,
      extensions::uninstall_extension,
      extensions::run_extension,
      bootstrap::propose_self_improvement,
      bootstrap::apply_self_improvement,
    ])
    .setup(|app| {
      let log_dir = app.path().app_log_dir()?;
      let guard = core::logging::init(&log_dir)?;
      app.manage(guard);

      let data_dir = app.path().app_data_dir()?;
      std::fs::create_dir_all(&data_dir)?;
      let benchmark_conn = ai::benchmarks::open(&data_dir.join("model_benchmarks.db"))?;
      app.state::<BenchmarkDbState>().conn.lock().unwrap().replace(benchmark_conn);

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
