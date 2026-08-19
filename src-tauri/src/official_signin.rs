//! Launches the official, user-installed sign-in clients from Settings.
//!
//! This deliberately has no OAuth callback, token parser, browser-session
//! importer, or provider-routing integration. Each identifier maps to one
//! fixed command, so renderer input can never become an executable or command
//! argument. Direct NeuralForge provider calls continue to use their explicit
//! API credential configuration.

use crate::core::errors::{AppError, AppResult};
use serde::Serialize;
use std::process::{Command, Stdio};

#[derive(Clone, Copy)]
struct SignInSpec {
    id: &'static str,
    title: &'static str,
    detail: &'static str,
    executable: &'static str,
    arguments: &'static [&'static str],
}

const CHATGPT_CODEX: SignInSpec = SignInSpec {
    id: "chatgpt_codex",
    title: "ChatGPT / Codex",
    detail: "Opens the installed official ChatGPT/Codex Windows app. Its account remains client-bound and is not imported into NeuralForge.",
    executable: "explorer.exe",
    arguments: &["shell:AppsFolder\\OpenAI.Codex_2p2nqsd0c76g0!App"],
};

const CLAUDE_CODE: SignInSpec = SignInSpec {
    id: "claude_code",
    title: "Claude / Claude Code",
    detail: "Opens Claude Code's official browser-based account sign-in in a separate terminal. NeuralForge does not read its credentials.",
    executable: "claude.exe",
    arguments: &["auth", "login"],
};

const GITHUB_CLI: SignInSpec = SignInSpec {
    id: "github_cli",
    title: "GitHub",
    detail: "Starts GitHub CLI's official web sign-in in a separate terminal. This signs in the GitHub CLI only; it does not configure a Copilot provider in NeuralForge.",
    executable: "gh.exe",
    arguments: &["auth", "login", "--web", "--git-protocol", "https"],
};

const SIGN_IN_CLIENTS: &[SignInSpec] = &[CHATGPT_CODEX, CLAUDE_CODE, GITHUB_CLI];

#[derive(Serialize)]
pub struct OfficialSignInClient {
    pub id: &'static str,
    pub title: &'static str,
    pub detail: &'static str,
    pub available: bool,
}

fn sign_in_spec(id: &str) -> AppResult<SignInSpec> {
    SIGN_IN_CLIENTS
        .iter()
        .copied()
        .find(|client| client.id == id)
        .ok_or_else(|| AppError::CommandRejected("Unknown official sign-in client".to_string()))
}

fn executable_exists(executable: &str) -> bool {
    Command::new("where.exe")
        .arg(executable)
        .stdin(Stdio::null())
        .stdout(Stdio::null())
        .stderr(Stdio::null())
        .status()
        .map(|status| status.success())
        .unwrap_or(false)
}

/// Lists fixed, official client launchers. This only checks whether their
/// executables can be found; it never probes or returns account credentials.
#[tauri::command]
pub fn list_official_signin_clients() -> Vec<OfficialSignInClient> {
    SIGN_IN_CLIENTS
        .iter()
        .map(|client| OfficialSignInClient {
            id: client.id,
            title: client.title,
            detail: client.detail,
            available: executable_exists(client.executable),
        })
        .collect()
}

/// Starts a fixed official sign-in flow in a separate visible process.
/// No renderer-provided text is passed through to the OS command line.
#[tauri::command]
pub fn start_official_signin(client_id: String) -> AppResult<()> {
    let client = sign_in_spec(&client_id)?;
    if !executable_exists(client.executable) {
        return Err(AppError::NotFound(format!(
            "{} is not installed or is not available on PATH",
            client.title
        )));
    }

    let mut command = Command::new(client.executable);
    command.args(client.arguments).stdin(Stdio::null());

    // Claude Code and GitHub CLI are interactive. They need their own visible
    // console instead of inheriting the hidden GUI process's standard streams.
    #[cfg(target_os = "windows")]
    {
        use std::os::windows::process::CommandExt;
        const CREATE_NEW_CONSOLE: u32 = 0x0000_0010;
        command.creation_flags(CREATE_NEW_CONSOLE);
    }

    command.spawn().map_err(AppError::Io)?;
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn sign_in_launches_are_allowlisted() {
        assert_eq!(
            sign_in_spec("chatgpt_codex").unwrap().executable,
            "explorer.exe"
        );
        assert_eq!(
            sign_in_spec("claude_code").unwrap().arguments,
            &["auth", "login"]
        );
        assert_eq!(
            sign_in_spec("github_cli").unwrap().arguments,
            &["auth", "login", "--web", "--git-protocol", "https"]
        );
        assert!(sign_in_spec("powershell").is_err());
        assert!(sign_in_spec("github_cli; calc.exe").is_err());
    }
}
