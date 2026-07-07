use crate::core::errors::{AppError, AppResult};
use crate::core::events::emit_terminal_output;
use portable_pty::{native_pty_system, Child, CommandBuilder, MasterPty, PtySize};
use std::collections::HashMap;
use std::io::{Read, Write};
use std::sync::Mutex;
use tauri::{AppHandle, Emitter, State};
use uuid::Uuid;

pub struct PtySession {
    writer: Box<dyn Write + Send>,
    master: Box<dyn MasterPty + Send>,
    child: Box<dyn Child + Send + Sync>,
}

#[derive(Default)]
pub struct TerminalRegistry {
    sessions: Mutex<HashMap<String, PtySession>>,
}

fn default_shell() -> String {
    if cfg!(windows) {
        std::env::var("COMSPEC").unwrap_or_else(|_| "cmd.exe".to_string())
    } else {
        std::env::var("SHELL").unwrap_or_else(|_| "/bin/bash".to_string())
    }
}

struct SpawnedPty {
    reader: Box<dyn Read + Send>,
    writer: Box<dyn Write + Send>,
    master: Box<dyn MasterPty + Send>,
    child: Box<dyn Child + Send + Sync>,
}

fn spawn_pty(rows: u16, cols: u16) -> AppResult<SpawnedPty> {
    let pty_system = native_pty_system();
    let pair = pty_system
        .openpty(PtySize {
            rows,
            cols,
            pixel_width: 0,
            pixel_height: 0,
        })
        .map_err(|e| AppError::Terminal(e.to_string()))?;

    let cmd = CommandBuilder::new(default_shell());
    let child = pair
        .slave
        .spawn_command(cmd)
        .map_err(|e| AppError::Terminal(e.to_string()))?;

    let reader = pair
        .master
        .try_clone_reader()
        .map_err(|e| AppError::Terminal(e.to_string()))?;
    let writer = pair
        .master
        .take_writer()
        .map_err(|e| AppError::Terminal(e.to_string()))?;

    Ok(SpawnedPty {
        reader,
        writer,
        master: pair.master,
        child,
    })
}

#[tauri::command]
pub fn spawn_shell(
    app: AppHandle,
    registry: State<TerminalRegistry>,
    rows: u16,
    cols: u16,
) -> AppResult<String> {
    let spawned = spawn_pty(rows, cols)?;
    let mut reader = spawned.reader;
    let session_id = Uuid::new_v4().to_string();
    tracing::info!(target: "terminal", event = "session_spawned", session_id = %session_id, rows, cols);

    {
        let mut sessions = registry.sessions.lock().unwrap();
        sessions.insert(
            session_id.clone(),
            PtySession {
                writer: spawned.writer,
                master: spawned.master,
                child: spawned.child,
            },
        );
    }

    let reader_session_id = session_id.clone();
    let reader_app = app.clone();
    std::thread::spawn(move || {
        let mut buf = [0u8; 4096];
        loop {
            match reader.read(&mut buf) {
                Ok(0) => break,
                Ok(n) => {
                    let chunk = String::from_utf8_lossy(&buf[..n]).into_owned();
                    if emit_terminal_output(&reader_app, &reader_session_id, &chunk).is_err() {
                        break;
                    }
                }
                Err(_) => break,
            }
        }
        let _ = reader_app.emit("TERMINAL_CLOSED", reader_session_id);
    });

    Ok(session_id)
}

#[tauri::command]
pub fn write_to_pty(registry: State<TerminalRegistry>, session_id: String, data: String) -> AppResult<()> {
    let mut sessions = registry.sessions.lock().unwrap();
    let session = sessions
        .get_mut(&session_id)
        .ok_or_else(|| AppError::NotFound(session_id.clone()))?;
    session
        .writer
        .write_all(data.as_bytes())
        .map_err(|e| AppError::Terminal(e.to_string()))?;
    Ok(())
}

#[tauri::command]
pub fn resize_pty(registry: State<TerminalRegistry>, session_id: String, rows: u16, cols: u16) -> AppResult<()> {
    let sessions = registry.sessions.lock().unwrap();
    let session = sessions
        .get(&session_id)
        .ok_or_else(|| AppError::NotFound(session_id.clone()))?;
    session
        .master
        .resize(PtySize {
            rows,
            cols,
            pixel_width: 0,
            pixel_height: 0,
        })
        .map_err(|e| AppError::Terminal(e.to_string()))?;
    Ok(())
}

#[tauri::command]
pub fn close_pty(registry: State<TerminalRegistry>, session_id: String) -> AppResult<()> {
    let mut sessions = registry.sessions.lock().unwrap();
    if let Some(mut session) = sessions.remove(&session_id) {
        let _ = session.child.kill();
        tracing::info!(target: "terminal", event = "session_closed", session_id = %session_id);
    }
    Ok(())
}

pub fn kill_all(registry: &TerminalRegistry) {
    let mut sessions = registry.sessions.lock().unwrap();
    for (_, mut session) in sessions.drain() {
        let _ = session.child.kill();
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::sync::mpsc;
    use std::time::Duration;

    #[test]
    fn spawn_pty_executes_command_and_produces_matching_output() {
        // A real terminal client (xterm.js in the actual app) answers ConPTY's
        // initial cursor-position query (ESC[6n) automatically; a bare test
        // harness has to do the same or the shell's console host stalls
        // before ever printing its banner/prompt.
        let mut spawned = spawn_pty(24, 80).expect("failed to spawn pty");

        let (tx, rx) = mpsc::channel();
        std::thread::spawn(move || {
            let mut buf = [0u8; 4096];
            let mut collected = String::new();
            let mut sent_command = false;
            for _ in 0..200 {
                match spawned.reader.read(&mut buf) {
                    Ok(0) => break,
                    Ok(n) => {
                        let chunk = String::from_utf8_lossy(&buf[..n]).into_owned();
                        collected.push_str(&chunk);
                        if chunk.contains("\u{1b}[6n") {
                            let _ = spawned.writer.write_all(b"\x1b[1;1R");
                            let _ = spawned.writer.flush();
                        }
                        if !sent_command {
                            let _ = spawned.writer.write_all(b"echo neuralforge_test_marker\r\n");
                            let _ = spawned.writer.flush();
                            sent_command = true;
                        }
                        if collected.contains("neuralforge_test_marker") {
                            break;
                        }
                    }
                    Err(_) => break,
                }
            }
            let _ = spawned.child.kill();
            let _ = tx.send(collected);
        });

        let output = rx
            .recv_timeout(Duration::from_secs(10))
            .expect("pty produced no output within timeout");
        assert!(
            output.contains("neuralforge_test_marker"),
            "expected pty output to contain the echoed marker, got: {output:?}"
        );
    }
}
