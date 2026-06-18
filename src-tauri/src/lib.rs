//! compas Tauri shell — Phase 1.
//!
//! Threading: the real-time audio engine (`compas-audio`) lives on its own thread
//! because `cpal::Stream` is not `Send`. Tauri commands send coarse [`EngineMsg`]s over
//! an mpsc channel; the engine thread forwards them as lock-free `AudioCommand`s.
//! Decoding/analysis runs on a per-load worker thread and reports back via Tauri events.
//! A telemetry thread samples lock-free deck position/state and emits it at UI rate.

use std::sync::mpsc::{channel, Sender};
use std::sync::Arc;
use std::thread;
use std::time::Duration;

mod spotify;

use compas_audio::{
    compute_peaks, AudioCommand, AudioEngine, DeckBuffer, DeckTelemetry, EngineConfig, FilterMode,
};
use serde::Serialize;
use tauri::{AppHandle, Emitter, State};
use tracing_subscriber::EnvFilter;

/// Frames per waveform peak bin (≈10.7 ms at 48 kHz). Tunes overview resolution.
const WAVEFORM_BIN_FRAMES: usize = 512;
/// Cap BPM analysis to the first N seconds for responsiveness on long tracks.
const ANALYSIS_MAX_SECS: usize = 90;
/// UI telemetry rate.
const TELEMETRY_HZ: u64 = 30;

/// Coarse, `Send` control messages from Tauri commands to the audio thread.
enum EngineMsg {
    SetCrossfader(f32),
    SetMasterGain(f32),
    DeckGain {
        deck: usize,
        gain: f32,
    },
    DeckEq {
        deck: usize,
        low: f32,
        mid: f32,
        high: f32,
    },
    DeckFilter {
        deck: usize,
        mode: FilterMode,
        cutoff: f32,
        resonance: f32,
    },
    DeckEcho {
        deck: usize,
        active: bool,
        time_sec: f32,
        feedback: f32,
        mix: f32,
    },
    DeckPlaying {
        deck: usize,
        playing: bool,
    },
    DeckTempo {
        deck: usize,
        ratio: f64,
    },
    DeckSeek {
        deck: usize,
        frame: f64,
    },
    Loop {
        deck: usize,
        in_frame: f64,
        out_frame: f64,
        active: bool,
    },
    LoopActive {
        deck: usize,
        active: bool,
    },
    Scratch {
        deck: usize,
        active: bool,
        speed: f64,
    },
    Load {
        deck: usize,
        buffer: Arc<DeckBuffer>,
    },
    Unload {
        deck: usize,
    },
}

/// Tauri-managed handle: a channel to the audio thread plus shared telemetry and the
/// negotiated device sample rate.
struct EngineHandle {
    tx: Sender<EngineMsg>,
    telemetry: Arc<DeckTelemetry>,
    sample_rate: u32,
}

impl EngineHandle {
    fn send(&self, msg: EngineMsg) -> Result<(), String> {
        self.tx.send(msg).map_err(|e| e.to_string())
    }
}

/// Spawn the dedicated audio thread that owns the [`AudioEngine`].
fn spawn_engine() -> EngineHandle {
    let (tx, rx) = channel::<EngineMsg>();
    let telemetry = Arc::new(DeckTelemetry::new());

    // Sample rate is discovered inside the thread; hand it back over a one-shot channel.
    let (sr_tx, sr_rx) = channel::<u32>();
    let telemetry_for_thread = telemetry.clone();

    let spawn_result = thread::Builder::new()
        .name("compas-audio".to_string())
        .spawn(move || {
            let mut engine = match AudioEngine::new(EngineConfig::default(), telemetry_for_thread) {
                Ok(engine) => {
                    let sr = engine.sample_rate();
                    tracing::info!("audio engine started @ {sr} Hz");
                    let _ = sr_tx.send(sr);
                    Some(engine)
                }
                Err(e) => {
                    tracing::error!("audio engine failed to start (continuing headless): {e}");
                    let _ = sr_tx.send(0);
                    None
                }
            };

            while let Ok(msg) = rx.recv() {
                let Some(engine) = engine.as_mut() else {
                    continue;
                };
                let cmd = match msg {
                    EngineMsg::SetCrossfader(p) => AudioCommand::SetCrossfader(p),
                    EngineMsg::SetMasterGain(g) => AudioCommand::SetMasterGain(g),
                    EngineMsg::DeckGain { deck, gain } => AudioCommand::SetDeckGain { deck, gain },
                    EngineMsg::DeckEq {
                        deck,
                        low,
                        mid,
                        high,
                    } => AudioCommand::SetDeckEq {
                        deck,
                        low_db: low,
                        mid_db: mid,
                        high_db: high,
                    },
                    EngineMsg::DeckFilter {
                        deck,
                        mode,
                        cutoff,
                        resonance,
                    } => AudioCommand::SetDeckFilter {
                        deck,
                        mode,
                        cutoff_hz: cutoff,
                        resonance,
                    },
                    EngineMsg::DeckEcho {
                        deck,
                        active,
                        time_sec,
                        feedback,
                        mix,
                    } => AudioCommand::SetDeckEcho {
                        deck,
                        active,
                        time_sec,
                        feedback,
                        mix,
                    },
                    EngineMsg::DeckPlaying { deck, playing } => {
                        AudioCommand::SetDeckPlaying { deck, playing }
                    }
                    EngineMsg::DeckTempo { deck, ratio } => {
                        AudioCommand::SetDeckTempo { deck, ratio }
                    }
                    EngineMsg::DeckSeek { deck, frame } => AudioCommand::SeekDeck { deck, frame },
                    EngineMsg::Loop {
                        deck,
                        in_frame,
                        out_frame,
                        active,
                    } => AudioCommand::SetLoop {
                        deck,
                        in_frame,
                        out_frame,
                        active,
                    },
                    EngineMsg::LoopActive { deck, active } => {
                        AudioCommand::SetLoopActive { deck, active }
                    }
                    EngineMsg::Scratch {
                        deck,
                        active,
                        speed,
                    } => AudioCommand::SetScratch {
                        deck,
                        active,
                        speed,
                    },
                    EngineMsg::Load { deck, buffer } => AudioCommand::LoadDeck { deck, buffer },
                    EngineMsg::Unload { deck } => AudioCommand::UnloadDeck { deck },
                };
                if let Err(e) = engine.send(cmd) {
                    tracing::warn!("dropped audio command: {e}");
                }
            }
        });

    if let Err(e) = &spawn_result {
        tracing::error!("failed to spawn audio thread: {e}");
    }

    let sample_rate = sr_rx.recv_timeout(Duration::from_secs(5)).unwrap_or(0);

    EngineHandle {
        tx,
        telemetry,
        sample_rate,
    }
}

// ----------------------------------------------------------------------------------
// Event payloads (Rust → frontend)
// ----------------------------------------------------------------------------------

#[derive(Serialize, Clone)]
struct DeckLoadedEvent {
    deck: usize,
    /// Absolute file path (provider id for local tracks) — lets the library mark A/B.
    path: String,
    title: String,
    artist: String,
    duration_ms: u64,
    source_rate: u32,
    frames: usize,
    bpm: f32,
    bpm_confidence: f32,
    /// Beatgrid phase: time of the first beat, and seconds per beat.
    first_beat_sec: f32,
    beat_interval_sec: f32,
    /// Detected musical key (Camelot code + traditional name).
    key_camelot: String,
    key_name: String,
    /// Max-abs amplitude per `WAVEFORM_BIN_FRAMES` frames; used to draw the overview.
    peaks: Vec<f32>,
}

#[derive(Serialize, Clone)]
struct DeckPositionEvent {
    deck: usize,
    frame: f64,
    playing: bool,
    /// Output peak (linear 0..~1) for the deck's VU meter.
    level: f32,
}

#[derive(Serialize, Clone)]
struct MasterMeterEvent {
    l: f32,
    r: f32,
}

#[derive(Serialize, Clone)]
struct DeckLoadingEvent {
    deck: usize,
    path: String,
}

#[derive(Serialize, Clone)]
struct DeckErrorEvent {
    deck: usize,
    message: String,
}

// ----------------------------------------------------------------------------------
// Commands
// ----------------------------------------------------------------------------------

#[derive(Serialize)]
struct EngineStatus {
    sample_rate: u32,
    decks: Vec<DeckStatus>,
}

#[derive(Serialize)]
struct DeckStatus {
    deck: usize,
    loaded: bool,
    playing: bool,
    frame: f64,
}

#[tauri::command]
fn engine_status(state: State<'_, EngineHandle>) -> EngineStatus {
    let decks = (0..2)
        .map(|deck| DeckStatus {
            deck,
            loaded: state.telemetry.is_loaded(deck),
            playing: state.telemetry.is_playing(deck),
            frame: state.telemetry.playhead_frames(deck),
        })
        .collect();
    EngineStatus {
        sample_rate: state.sample_rate,
        decks,
    }
}

/// Decode + analyze a file on a worker thread, then install it on `deck` and emit
/// `deck:loaded` (or `deck:error`). Returns immediately.
#[tauri::command]
fn load_track(app: AppHandle, state: State<'_, EngineHandle>, deck: usize, path: String) {
    let tx = state.tx.clone();
    // Tell the UI immediately so it can show a loading state during decode + analysis.
    let _ = app.emit("deck:loading", DeckLoadingEvent { deck, path: path.clone() });
    thread::spawn(move || {
        let (buffer, metadata) = match compas_sources::decode_full(&path) {
            Ok(v) => v,
            Err(e) => {
                let _ = app.emit(
                    "deck:error",
                    DeckErrorEvent {
                        deck,
                        message: format!("decode failed: {e}"),
                    },
                );
                return;
            }
        };

        let peaks = compute_peaks(&buffer.samples, WAVEFORM_BIN_FRAMES);
        let (grid, key) = analyze_track(&buffer);

        let _ = app.emit(
            "deck:loaded",
            DeckLoadedEvent {
                deck,
                path: metadata.provider_id.clone(),
                title: metadata.title.clone(),
                artist: metadata.artist.clone(),
                duration_ms: buffer.duration_ms(),
                source_rate: buffer.source_rate,
                frames: buffer.frames(),
                bpm: grid.bpm,
                bpm_confidence: grid.confidence,
                first_beat_sec: grid.first_beat_sec,
                beat_interval_sec: grid.beat_interval_sec,
                key_camelot: key.camelot,
                key_name: key.name,
                peaks,
            },
        );

        let _ = tx.send(EngineMsg::Load {
            deck,
            buffer: Arc::new(buffer),
        });
    });
}

/// Downmix to mono (capped to [`ANALYSIS_MAX_SECS`]) and estimate beatgrid + key.
fn analyze_track(
    buffer: &DeckBuffer,
) -> (
    compas_dsp::analysis::BeatGrid,
    compas_dsp::analysis::KeyEstimate,
) {
    let max_frames = ANALYSIS_MAX_SECS * buffer.source_rate as usize;
    let frames = buffer.frames().min(max_frames);
    let mut mono = Vec::with_capacity(frames);
    for f in 0..frames {
        mono.push(0.5 * (buffer.samples[f * 2] + buffer.samples[f * 2 + 1]));
    }
    let grid = compas_dsp::analysis::estimate_beatgrid(&mono, buffer.source_rate);
    let key = compas_dsp::analysis::estimate_key(&mono, buffer.source_rate);
    (grid, key)
}

#[derive(Serialize)]
struct ProbedTrack {
    path: String,
    title: String,
    artist: String,
    duration_ms: u64,
}

/// Cheap header probe (no full decode) for adding a file to the library.
#[tauri::command]
fn probe_track(path: String) -> Result<ProbedTrack, String> {
    use compas_sources::{AudioSource, LocalFileSource};
    let src = LocalFileSource::open(&path).map_err(|e| e.to_string())?;
    let m = src.metadata();
    Ok(ProbedTrack {
        title: m.title.clone(),
        artist: m.artist.clone(),
        duration_ms: m.duration_ms.unwrap_or(0),
        path,
    })
}

#[tauri::command]
fn deck_play(state: State<'_, EngineHandle>, deck: usize) -> Result<(), String> {
    state.send(EngineMsg::DeckPlaying {
        deck,
        playing: true,
    })
}

#[tauri::command]
fn deck_pause(state: State<'_, EngineHandle>, deck: usize) -> Result<(), String> {
    state.send(EngineMsg::DeckPlaying {
        deck,
        playing: false,
    })
}

#[tauri::command]
fn deck_seek(state: State<'_, EngineHandle>, deck: usize, frame: f64) -> Result<(), String> {
    state.send(EngineMsg::DeckSeek { deck, frame })
}

#[tauri::command]
fn deck_unload(state: State<'_, EngineHandle>, deck: usize) -> Result<(), String> {
    state.send(EngineMsg::Unload { deck })
}

#[tauri::command]
fn set_deck_tempo(state: State<'_, EngineHandle>, deck: usize, ratio: f64) -> Result<(), String> {
    state.send(EngineMsg::DeckTempo { deck, ratio })
}

#[tauri::command]
fn set_loop(
    state: State<'_, EngineHandle>,
    deck: usize,
    in_frame: f64,
    out_frame: f64,
    active: bool,
) -> Result<(), String> {
    state.send(EngineMsg::Loop {
        deck,
        in_frame,
        out_frame,
        active,
    })
}

#[tauri::command]
fn set_loop_active(state: State<'_, EngineHandle>, deck: usize, active: bool) -> Result<(), String> {
    state.send(EngineMsg::LoopActive { deck, active })
}

/// Jog-wheel scratch: `active` engages the gesture; `speed` is the read rate (1.0 =
/// natural play speed, negative = reverse). The UI streams `speed` from drag velocity.
#[tauri::command]
fn deck_scratch(
    state: State<'_, EngineHandle>,
    deck: usize,
    active: bool,
    speed: f64,
) -> Result<(), String> {
    state.send(EngineMsg::Scratch {
        deck,
        active,
        speed,
    })
}

#[tauri::command]
fn set_deck_gain(state: State<'_, EngineHandle>, deck: usize, gain: f32) -> Result<(), String> {
    state.send(EngineMsg::DeckGain { deck, gain })
}

#[tauri::command]
fn set_deck_eq(
    state: State<'_, EngineHandle>,
    deck: usize,
    low: f32,
    mid: f32,
    high: f32,
) -> Result<(), String> {
    state.send(EngineMsg::DeckEq {
        deck,
        low,
        mid,
        high,
    })
}

#[tauri::command]
fn set_deck_filter(
    state: State<'_, EngineHandle>,
    deck: usize,
    mode: String,
    cutoff: f32,
    resonance: f32,
) -> Result<(), String> {
    let mode = match mode.as_str() {
        "lowpass" => FilterMode::LowPass,
        "highpass" => FilterMode::HighPass,
        _ => FilterMode::Off,
    };
    state.send(EngineMsg::DeckFilter {
        deck,
        mode,
        cutoff,
        resonance,
    })
}

/// Configure the per-deck echo/delay insert. The UI computes `time_sec` (often
/// beat-synced from the analyzed BPM) and the wet/feedback amounts.
#[tauri::command]
fn set_deck_echo(
    state: State<'_, EngineHandle>,
    deck: usize,
    active: bool,
    time_sec: f32,
    feedback: f32,
    mix: f32,
) -> Result<(), String> {
    state.send(EngineMsg::DeckEcho {
        deck,
        active,
        time_sec,
        feedback,
        mix,
    })
}

#[tauri::command]
fn set_crossfader(state: State<'_, EngineHandle>, value: f32) -> Result<(), String> {
    state.send(EngineMsg::SetCrossfader(value))
}

#[tauri::command]
fn set_master_gain(state: State<'_, EngineHandle>, value: f32) -> Result<(), String> {
    state.send(EngineMsg::SetMasterGain(value))
}

/// Spawn the telemetry emitter: samples lock-free deck state and emits `deck:position`.
fn spawn_telemetry(app: AppHandle, telemetry: Arc<DeckTelemetry>) {
    thread::spawn(move || {
        let period = Duration::from_millis(1000 / TELEMETRY_HZ);
        loop {
            for deck in 0..2 {
                if telemetry.is_loaded(deck) {
                    let _ = app.emit(
                        "deck:position",
                        DeckPositionEvent {
                            deck,
                            frame: telemetry.playhead_frames(deck),
                            playing: telemetry.is_playing(deck),
                            level: telemetry.deck_level(deck),
                        },
                    );
                }
            }
            let (l, r) = telemetry.master_level();
            let _ = app.emit("master:level", MasterMeterEvent { l, r });
            thread::sleep(period);
        }
    });
}

/// Application entry point.
pub fn run() {
    let _ = tracing_subscriber::fmt()
        .with_env_filter(
            EnvFilter::try_from_default_env().unwrap_or_else(|_| EnvFilter::new("info")),
        )
        .try_init();

    let engine = spawn_engine();
    let telemetry = engine.telemetry.clone();

    let result = tauri::Builder::default()
        .plugin(tauri_plugin_dialog::init())
        .plugin(tauri_plugin_opener::init())
        .manage(engine)
        .setup(move |app| {
            spawn_telemetry(app.handle().clone(), telemetry.clone());
            Ok(())
        })
        .invoke_handler(tauri::generate_handler![
            engine_status,
            probe_track,
            load_track,
            deck_play,
            deck_pause,
            deck_seek,
            deck_unload,
            set_loop,
            set_loop_active,
            deck_scratch,
            set_deck_tempo,
            set_deck_gain,
            set_deck_eq,
            set_deck_filter,
            set_deck_echo,
            set_crossfader,
            set_master_gain,
            spotify::spotify_listen
        ])
        .run(tauri::generate_context!());

    if let Err(e) = result {
        tracing::error!("fatal: error while running tauri application: {e}");
        std::process::exit(1);
    }
}
