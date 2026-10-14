//! Termust desktop shell.
//!
//! The frontend owns the screens; this layer only exposes the SSH session
//! commands it needs and keeps the session state alive between calls.

mod ssh;

use tauri::{AppHandle, State};

use ssh::{ConnectRequest, SshState};

#[tauri::command]
fn connect_ssh(
    app: AppHandle,
    state: State<'_, SshState>,
    request: ConnectRequest,
) -> Result<(), String> {
    ssh::start(&app, &state, request)
}

#[tauri::command]
fn write_ssh(state: State<'_, SshState>, session_id: String, data: String) -> Result<(), String> {
    ssh::write(&state, &session_id, &data)
}

#[tauri::command]
fn resize_ssh(
    state: State<'_, SshState>,
    session_id: String,
    cols: u32,
    rows: u32,
) -> Result<(), String> {
    ssh::resize(&state, &session_id, cols, rows)
}

#[tauri::command]
fn respond_host_key(
    state: State<'_, SshState>,
    session_id: String,
    accepted: bool,
) -> Result<(), String> {
    ssh::respond_host_key(&state, &session_id, accepted)
}

#[tauri::command]
async fn disconnect_ssh(state: State<'_, SshState>, session_id: String) -> Result<(), String> {
    ssh::stop(&state, &session_id).await;
    Ok(())
}

#[cfg_attr(mobile, tauri::mobile_entry_point)]
pub fn run() {
    let result = tauri::Builder::default()
        .manage(SshState::default())
        .invoke_handler(tauri::generate_handler![
            connect_ssh,
            write_ssh,
            resize_ssh,
            respond_host_key,
            disconnect_ssh
        ])
        .run(tauri::generate_context!());

    if let Err(error) = result {
        // Nothing is left to recover: report it on stderr and stop.
        eprintln!("Termust could not start: {error}");
        std::process::exit(1);
    }
}
