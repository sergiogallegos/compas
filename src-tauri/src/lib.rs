//! compas Tauri shell.
//!
//! The webview (frontend) talks to the native core exclusively through the Tauri
//! commands declared here. The real-time audio engine lives on its OWN thread
//! (`compas-audio`), because a `cpal::Stream` is not `Send` on all platforms and must
//! not be owned by Tauri's shared state. We bridge to it with a plain mpsc channel of
//! coarse control messages; the engine then forwards them as lock-free
//! [`compas_audio::AudioCommand`]s into the audio callback.
//!
//! This is the P0 scaffold: the window opens, the engine thread starts (and degrades
//! gracefully if no audio device is available), and a couple of commands prove the
//! IPC path end-to-end. No decks are loaded yet — that is Phase 1.

use std::sync::mpsc::{channel, Sender};
use std::thread;

use compas_audio::{AudioCommand, AudioEngine, EngineConfig};
use serde::Serialize;
use tracing_subscriber::EnvFilter;

/// Coarse, `Send` control messages from Tauri commands to the audio thread.
enum EngineMsg {
    SetCrossfader(f32),
    SetMasterGain(f32),
    SetDeckGain { deck: usize, gain: f32 },
}

/// Tauri-managed handle to the audio thread.
struct EngineHandle {
    tx: Sender<EngineMsg>,
}

/// Spawn the dedicated audio thread that owns the [`AudioEngine`].
fn spawn_engine() -> EngineHandle {
    let (tx, rx) = channel::<EngineMsg>();

    let spawn_result = thread::Builder::new()
        .name("compas-audio".to_string())
        .spawn(move || {
            let mut engine = match AudioEngine::new(EngineConfig::default()) {
                Ok(engine) => {
                    tracing::info!("audio engine started @ {} Hz", engine.sample_rate());
                    Some(engine)
                }
                Err(e) => {
                    tracing::error!("audio engine failed to start (continuing headless): {e}");
                    None
                }
            };

            while let Ok(msg) = rx.recv() {
                let Some(engine) = engine.as_mut() else {
                    continue; // No device; drain and ignore until app exits.
                };
                let cmd = match msg {
                    EngineMsg::SetCrossfader(p) => AudioCommand::SetCrossfader(p),
                    EngineMsg::SetMasterGain(g) => AudioCommand::SetMasterGain(g),
                    EngineMsg::SetDeckGain { deck, gain } => {
                        AudioCommand::SetDeckGain { deck, gain }
                    }
                };
                if let Err(e) = engine.send(cmd) {
                    tracing::warn!("dropped audio command: {e}");
                }
            }
        });

    if let Err(e) = &spawn_result {
        tracing::error!("failed to spawn audio thread: {e}");
    }

    EngineHandle { tx }
}

#[derive(Serialize)]
struct AppInfo {
    name: &'static str,
    version: &'static str,
    phase: &'static str,
}

/// Basic app/build info — used by the frontend shell to prove the IPC bridge works.
#[tauri::command]
fn app_info() -> AppInfo {
    AppInfo {
        name: "compas",
        version: env!("CARGO_PKG_VERSION"),
        phase: "P0 — scaffold",
    }
}

#[tauri::command]
fn set_crossfader(state: tauri::State<'_, EngineHandle>, value: f32) -> Result<(), String> {
    state
        .tx
        .send(EngineMsg::SetCrossfader(value))
        .map_err(|e| e.to_string())
}

#[tauri::command]
fn set_master_gain(state: tauri::State<'_, EngineHandle>, value: f32) -> Result<(), String> {
    state
        .tx
        .send(EngineMsg::SetMasterGain(value))
        .map_err(|e| e.to_string())
}

#[tauri::command]
fn set_deck_gain(
    state: tauri::State<'_, EngineHandle>,
    deck: usize,
    gain: f32,
) -> Result<(), String> {
    state
        .tx
        .send(EngineMsg::SetDeckGain { deck, gain })
        .map_err(|e| e.to_string())
}

/// Application entry point (shared by desktop `main.rs` and future mobile targets).
pub fn run() {
    let _ = tracing_subscriber::fmt()
        .with_env_filter(EnvFilter::try_from_default_env().unwrap_or_else(|_| EnvFilter::new("info")))
        .try_init();

    let engine = spawn_engine();

    let result = tauri::Builder::default()
        .manage(engine)
        .invoke_handler(tauri::generate_handler![
            app_info,
            set_crossfader,
            set_master_gain,
            set_deck_gain
        ])
        .run(tauri::generate_context!());

    if let Err(e) = result {
        tracing::error!("fatal: error while running tauri application: {e}");
        std::process::exit(1);
    }
}
