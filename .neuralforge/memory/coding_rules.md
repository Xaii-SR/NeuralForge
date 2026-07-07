# Coding Rules

- Rust commands return `AppResult<T>` (`Result<T, AppError>`); `AppError`
  serializes to a string for the frontend. Don't `unwrap()`/`expect()` in
  command handlers — propagate via `?`.
- Any filesystem path coming from the frontend must go through
  `validate_within_workspace` (existing paths) or
  `validate_new_path_in_workspace` (not-yet-existing paths) before touching
  disk. Never trust a path string directly.
- Testable logic goes in plain functions; `#[tauri::command]` wrappers stay
  thin (extract state, call the pure function, map errors). This is what
  makes `cargo test` able to cover path-validation and PTY spawn logic
  without spinning up a Tauri runtime.
- No SSR, API routes, middleware, or server actions in the Next.js app — it's
  a static export loaded by Tauri, full stop.
- Log with `tracing`, not `println!`/`eprintln!`, in any subsystem module.
