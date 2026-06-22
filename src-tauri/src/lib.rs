//! compas Tauri shell — Phase 1.
//!
//! Threading: the real-time audio engine (`compas-audio`) lives on its own thread
//! because `cpal::Stream` is not `Send`. Tauri commands send coarse [`EngineMsg`]s over
//! an mpsc channel; the engine thread forwards them as lock-free `AudioCommand`s.
//! Decoding/analysis runs on a per-load worker thread and reports back via Tauri events.
//! A telemetry thread samples lock-free deck position/state and emits it at UI rate.

use std::fs::File;
use std::io::{BufWriter, Seek, SeekFrom, Write};
use std::sync::atomic::{AtomicBool, AtomicU32, AtomicU64, Ordering};
use std::sync::mpsc::{channel, RecvTimeoutError, Sender};
use std::sync::{Arc, Mutex};
use std::thread;
use std::time::{Duration, Instant};

mod controllers;
mod db;
mod hid;
mod spotify;

use compas_audio::{
    compute_peaks, AudioCommand, AudioEngine, DeckBuffer, DeckTelemetry, EngineConfig, FilterMode,
};
use midir::{MidiInput, MidiInputConnection};
use rtrb::{Consumer, Producer, RingBuffer};
use serde::Serialize;
use tauri::{AppHandle, Emitter, Manager, State};
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
    SetCrossfaderConfig {
        curve: f32,
        mode: u8,
        reverse: bool,
    },
    SetCueMode {
        deck: usize,
        mode: u8,
    },
    SetCuePoint {
        deck: usize,
        frame: f64,
    },
    CueButton {
        deck: usize,
        pressed: bool,
    },
    ScaleLoop {
        deck: usize,
        factor: f64,
    },
    MoveLoop {
        deck: usize,
        delta_frames: f64,
    },
    SetDeckSyncMode {
        deck: usize,
        mode: u8,
    },
    SetSyncLeader {
        deck: usize,
        explicit: bool,
    },
    SyncToLeader {
        deck: usize,
    },
    SetDeckReplayGain {
        deck: usize,
        gain: f32,
    },
    SetDeckFxMacro {
        deck: usize,
        value: f32,
    },
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
    DeckReverb {
        deck: usize,
        active: bool,
        room_size: f32,
        mix: f32,
    },
    DeckFlanger {
        deck: usize,
        active: bool,
        rate_hz: f32,
        depth: f32,
        feedback: f32,
        mix: f32,
    },
    DeckCrusher {
        deck: usize,
        active: bool,
        bits: f32,
        downsample: u32,
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
    DeckKeylock {
        deck: usize,
        active: bool,
    },
    DeckXfaderAssign {
        deck: usize,
        assign: u8,
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
    LoopRoll {
        deck: usize,
        in_frame: f64,
        out_frame: f64,
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
        beat_offset: f64,
        beat_interval: f64,
    },
    Unload {
        deck: usize,
    },
    Beatgrid {
        deck: usize,
        offset: f64,
        interval: f64,
    },
    DeckSync {
        deck: usize,
        master: Option<usize>,
    },
    StartRecording {
        sink: Producer<f32>,
    },
    StopRecording,
    DeckCue {
        deck: usize,
        active: bool,
    },
    CueMix(f32),
    CueVolume(f32),
    StartCueOutput {
        sink: Producer<f32>,
    },
    StopCueOutput,
    NoteOn {
        note: u8,
        velocity: u8,
    },
    NoteOff {
        note: u8,
    },
    AllNotesOff,
    SynthWaveform {
        index: u8,
    },
    SynthGain {
        gain: f32,
    },
    LoadSample {
        slot: usize,
        buffer: Arc<DeckBuffer>,
    },
    ClearSample {
        slot: usize,
    },
    TriggerSample {
        slot: usize,
        velocity: u8,
    },
    StopSample {
        slot: usize,
    },
    SampleLoop {
        slot: usize,
        looping: bool,
    },
    SamplerGain {
        gain: f32,
    },
}

/// Tauri-managed handle: a channel to the audio thread plus shared telemetry and output health.
struct EngineHandle {
    tx: Sender<EngineMsg>,
    telemetry: Arc<DeckTelemetry>,
    audio: Arc<AudioRuntimeStatus>,
    /// True while a master recording is in progress (guards against double-start).
    recording: AtomicBool,
}

impl EngineHandle {
    fn send(&self, msg: EngineMsg) -> Result<(), String> {
        self.tx.send(msg).map_err(|e| e.to_string())
    }
}

struct AudioRuntimeStatus {
    online: AtomicBool,
    restarting: AtomicBool,
    restarts: AtomicU64,
    sample_rate: AtomicU32,
    last_error: Mutex<Option<String>>,
}

impl AudioRuntimeStatus {
    fn new() -> Self {
        Self {
            online: AtomicBool::new(false),
            restarting: AtomicBool::new(false),
            restarts: AtomicU64::new(0),
            sample_rate: AtomicU32::new(0),
            last_error: Mutex::new(None),
        }
    }

    fn mark_online(&self, sample_rate: u32) {
        self.online.store(true, Ordering::Relaxed);
        self.restarting.store(false, Ordering::Relaxed);
        self.sample_rate.store(sample_rate, Ordering::Relaxed);
        if let Ok(mut last) = self.last_error.lock() {
            *last = None;
        }
    }

    fn mark_restarting(&self) {
        self.online.store(false, Ordering::Relaxed);
        self.restarting.store(true, Ordering::Relaxed);
    }

    fn mark_error(&self, message: String) {
        self.online.store(false, Ordering::Relaxed);
        self.restarting.store(false, Ordering::Relaxed);
        if let Ok(mut last) = self.last_error.lock() {
            *last = Some(message);
        }
    }

    fn count_restart(&self) {
        self.restarts.fetch_add(1, Ordering::Relaxed);
    }

    fn last_error(&self) -> Option<String> {
        self.last_error.lock().ok().and_then(|e| e.clone())
    }
}

/// Spawn the dedicated audio thread that owns the [`AudioEngine`].
fn spawn_engine() -> EngineHandle {
    let (tx, rx) = channel::<EngineMsg>();
    let telemetry = Arc::new(DeckTelemetry::new());
    let audio = Arc::new(AudioRuntimeStatus::new());

    // Sample rate is discovered inside the thread; hand it back over a one-shot channel.
    let (sr_tx, sr_rx) = channel::<u32>();
    let telemetry_for_thread = telemetry.clone();
    let audio_for_thread = audio.clone();

    let spawn_result = thread::Builder::new()
        .name("compas-audio".to_string())
        .spawn(move || {
            let mut initial_report = Some(sr_tx);
            let mut start_engine = |is_restart: bool| {
                audio_for_thread.mark_restarting();
                match AudioEngine::new(EngineConfig::default(), telemetry_for_thread.clone()) {
                    Ok(engine) => {
                        if is_restart {
                            audio_for_thread.count_restart();
                        }
                        let sr = engine.sample_rate();
                        tracing::info!("audio engine started @ {sr} Hz");
                        audio_for_thread.mark_online(sr);
                        if let Some(tx) = initial_report.take() {
                            let _ = tx.send(sr);
                        }
                        Some(engine)
                    }
                    Err(e) => {
                        let msg = e.to_string();
                        tracing::error!(
                            "audio engine failed to start (continuing headless): {msg}"
                        );
                        audio_for_thread.mark_error(msg);
                        if let Some(tx) = initial_report.take() {
                            let _ = tx.send(0);
                        }
                        None
                    }
                }
            };

            let mut engine = start_engine(false);
            let mut next_retry = Instant::now() + Duration::from_secs(2);

            loop {
                if let Some(active) = engine.as_ref() {
                    if active.stream_failed() {
                        let msg = active
                            .last_stream_error()
                            .unwrap_or_else(|| "audio stream error".to_string());
                        tracing::warn!("audio stream unhealthy; restarting: {msg}");
                        audio_for_thread.mark_error(msg);
                        engine = None;
                        next_retry = Instant::now();
                    }
                }

                if engine.is_none() && Instant::now() >= next_retry {
                    engine = start_engine(true);
                    next_retry = Instant::now() + Duration::from_secs(2);
                }

                let msg = match rx.recv_timeout(Duration::from_millis(250)) {
                    Ok(msg) => msg,
                    Err(RecvTimeoutError::Timeout) => continue,
                    Err(RecvTimeoutError::Disconnected) => break,
                };

                if engine.is_none() {
                    engine = start_engine(true);
                    next_retry = Instant::now() + Duration::from_secs(2);
                }

                let Some(engine) = engine.as_mut() else {
                    tracing::warn!("dropped audio command while output device is offline");
                    continue;
                };

                let cmd = match msg {
                    EngineMsg::SetCrossfader(p) => AudioCommand::SetCrossfader(p),
                    EngineMsg::SetCrossfaderConfig {
                        curve,
                        mode,
                        reverse,
                    } => AudioCommand::SetCrossfaderConfig {
                        curve,
                        mode,
                        reverse,
                    },
                    EngineMsg::SetCueMode { deck, mode } => AudioCommand::SetCueMode { deck, mode },
                    EngineMsg::SetCuePoint { deck, frame } => {
                        AudioCommand::SetCuePoint { deck, frame }
                    }
                    EngineMsg::CueButton { deck, pressed } => {
                        AudioCommand::CueButton { deck, pressed }
                    }
                    EngineMsg::ScaleLoop { deck, factor } => {
                        AudioCommand::ScaleLoop { deck, factor }
                    }
                    EngineMsg::MoveLoop { deck, delta_frames } => {
                        AudioCommand::MoveLoop { deck, delta_frames }
                    }
                    EngineMsg::SetDeckSyncMode { deck, mode } => {
                        AudioCommand::SetDeckSyncMode { deck, mode }
                    }
                    EngineMsg::SetSyncLeader { deck, explicit } => {
                        AudioCommand::SetSyncLeader { deck, explicit }
                    }
                    EngineMsg::SyncToLeader { deck } => AudioCommand::SyncToLeader { deck },
                    EngineMsg::SetDeckReplayGain { deck, gain } => {
                        AudioCommand::SetDeckReplayGain { deck, gain }
                    }
                    EngineMsg::SetDeckFxMacro { deck, value } => {
                        AudioCommand::SetDeckFxMacro { deck, value }
                    }
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
                    EngineMsg::DeckReverb {
                        deck,
                        active,
                        room_size,
                        mix,
                    } => AudioCommand::SetDeckReverb {
                        deck,
                        active,
                        room_size,
                        mix,
                    },
                    EngineMsg::DeckFlanger {
                        deck,
                        active,
                        rate_hz,
                        depth,
                        feedback,
                        mix,
                    } => AudioCommand::SetDeckFlanger {
                        deck,
                        active,
                        rate_hz,
                        depth,
                        feedback,
                        mix,
                    },
                    EngineMsg::DeckCrusher {
                        deck,
                        active,
                        bits,
                        downsample,
                        mix,
                    } => AudioCommand::SetDeckCrusher {
                        deck,
                        active,
                        bits,
                        downsample,
                        mix,
                    },
                    EngineMsg::DeckPlaying { deck, playing } => {
                        AudioCommand::SetDeckPlaying { deck, playing }
                    }
                    EngineMsg::DeckTempo { deck, ratio } => {
                        AudioCommand::SetDeckTempo { deck, ratio }
                    }
                    EngineMsg::DeckKeylock { deck, active } => {
                        AudioCommand::SetDeckKeylock { deck, active }
                    }
                    EngineMsg::DeckXfaderAssign { deck, assign } => {
                        AudioCommand::SetDeckXfaderAssign { deck, assign }
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
                    EngineMsg::LoopRoll {
                        deck,
                        in_frame,
                        out_frame,
                        active,
                    } => AudioCommand::SetLoopRoll {
                        deck,
                        in_frame,
                        out_frame,
                        active,
                    },
                    EngineMsg::Scratch {
                        deck,
                        active,
                        speed,
                    } => AudioCommand::SetScratch {
                        deck,
                        active,
                        speed,
                    },
                    EngineMsg::Load {
                        deck,
                        buffer,
                        beat_offset,
                        beat_interval,
                    } => AudioCommand::LoadDeck {
                        deck,
                        buffer,
                        beat_offset,
                        beat_interval,
                    },
                    EngineMsg::Unload { deck } => AudioCommand::UnloadDeck { deck },
                    EngineMsg::Beatgrid {
                        deck,
                        offset,
                        interval,
                    } => AudioCommand::SetBeatgrid {
                        deck,
                        offset,
                        interval,
                    },
                    EngineMsg::DeckSync { deck, master } => {
                        AudioCommand::SetDeckSync { deck, master }
                    }
                    EngineMsg::StartRecording { sink } => AudioCommand::StartRecording { sink },
                    EngineMsg::StopRecording => AudioCommand::StopRecording,
                    EngineMsg::DeckCue { deck, active } => {
                        AudioCommand::SetDeckCue { deck, active }
                    }
                    EngineMsg::CueMix(m) => AudioCommand::SetCueMix(m),
                    EngineMsg::CueVolume(v) => AudioCommand::SetCueVolume(v),
                    EngineMsg::StartCueOutput { sink } => AudioCommand::StartCueOutput { sink },
                    EngineMsg::StopCueOutput => AudioCommand::StopCueOutput,
                    EngineMsg::NoteOn { note, velocity } => AudioCommand::NoteOn { note, velocity },
                    EngineMsg::NoteOff { note } => AudioCommand::NoteOff { note },
                    EngineMsg::AllNotesOff => AudioCommand::AllNotesOff,
                    EngineMsg::SynthWaveform { index } => AudioCommand::SetSynthWaveform { index },
                    EngineMsg::SynthGain { gain } => AudioCommand::SetSynthGain { gain },
                    EngineMsg::LoadSample { slot, buffer } => {
                        AudioCommand::LoadSample { slot, buffer }
                    }
                    EngineMsg::ClearSample { slot } => AudioCommand::ClearSample { slot },
                    EngineMsg::TriggerSample { slot, velocity } => {
                        AudioCommand::TriggerSample { slot, velocity }
                    }
                    EngineMsg::StopSample { slot } => AudioCommand::StopSample { slot },
                    EngineMsg::SampleLoop { slot, looping } => {
                        AudioCommand::SetSampleLoop { slot, looping }
                    }
                    EngineMsg::SamplerGain { gain } => AudioCommand::SetSamplerGain { gain },
                };
                if let Err(e) = engine.send(cmd) {
                    tracing::warn!("dropped audio command: {e}");
                }
            }
        });

    if let Err(e) = &spawn_result {
        tracing::error!("failed to spawn audio thread: {e}");
    }

    let _ = sr_rx.recv_timeout(Duration::from_secs(5)).unwrap_or(0);

    EngineHandle {
        tx,
        telemetry,
        audio,
        recording: AtomicBool::new(false),
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
    /// Per-bin `[low, mid, high]` band energy (0..1) for frequency-colored waveforms.
    band_peaks: Vec<[f32; 3]>,
    /// Loudness-normalization (ReplayGain) factor the engine applied on load.
    replay_gain: f32,
}

#[derive(Serialize, Clone)]
struct DeckPositionEvent {
    deck: usize,
    frame: f64,
    playing: bool,
    /// Output peak (linear 0..~1) for the deck's VU meter.
    level: f32,
    /// Effective advance in source frames/sec — the UI extrapolates the play-head with this.
    rate: f64,
    /// Measured output (DAC) latency in seconds, for play-head offset.
    latency_secs: f32,
}

#[derive(Serialize, Clone)]
struct MasterMeterEvent {
    l: f32,
    r: f32,
}

#[derive(Serialize, Clone)]
struct EngineLoadEvent {
    /// Audio-callback load, 0..~1 (≥1 = overrun).
    load: f32,
    /// Cumulative real-time-budget overruns since start.
    xruns: u64,
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
    audio_online: bool,
    audio_restarting: bool,
    audio_restarts: u64,
    audio_error: Option<String>,
    decks: Vec<DeckStatus>,
}

#[derive(Serialize)]
struct DeckStatus {
    deck: usize,
    loaded: bool,
    playing: bool,
    frame: f64,
}

/// Build metadata surfaced to the UI (title-bar version chip, About dialog).
#[derive(Serialize)]
struct BuildInfo {
    /// `Cargo.toml` package version (e.g. `0.1.0`).
    version: &'static str,
    /// Short git SHA the binary was built from, or `"dev"` if vergen couldn't read git.
    sha: &'static str,
    /// ISO-8601 build timestamp (UTC), or empty string if unavailable.
    built_at: &'static str,
}

#[tauri::command]
fn build_info() -> BuildInfo {
    let sha = env!("COMPAS_GIT_SHA");
    BuildInfo {
        version: env!("CARGO_PKG_VERSION"),
        sha: if sha.is_empty() { "dev" } else { sha },
        built_at: env!("COMPAS_BUILD_TIMESTAMP"),
    }
}

#[tauri::command]
fn engine_status(state: State<'_, EngineHandle>) -> EngineStatus {
    let sample_rate = state.audio.sample_rate.load(Ordering::Relaxed);
    let decks = (0..2)
        .map(|deck| DeckStatus {
            deck,
            loaded: state.telemetry.is_loaded(deck),
            playing: state.telemetry.is_playing(deck),
            frame: state.telemetry.playhead_frames(deck),
        })
        .collect();
    EngineStatus {
        sample_rate,
        audio_online: state.audio.online.load(Ordering::Relaxed),
        audio_restarting: state.audio.restarting.load(Ordering::Relaxed),
        audio_restarts: state.audio.restarts.load(Ordering::Relaxed),
        audio_error: state.audio.last_error(),
        decks,
    }
}

/// Decode + analyze a file on a worker thread, then install it on `deck` and emit
/// `deck:loaded` (or `deck:error`). Returns immediately.
#[tauri::command]
fn load_track(app: AppHandle, state: State<'_, EngineHandle>, deck: usize, path: String) {
    let tx = state.tx.clone();
    // Tell the UI immediately so it can show a loading state during decode + analysis.
    let _ = app.emit(
        "deck:loading",
        DeckLoadingEvent {
            deck,
            path: path.clone(),
        },
    );
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
        let band_peaks = compas_dsp::analysis::band_peaks(
            &buffer.samples,
            buffer.source_rate,
            WAVEFORM_BIN_FRAMES,
        );
        let (grid, key) = analyze_track(&buffer);
        let replay_gain = compas_dsp::analysis::replaygain_linear(&buffer.samples);

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
                band_peaks,
                replay_gain,
            },
        );

        // Beatgrid → source frames for the engine's sync PLL.
        let sr = buffer.source_rate as f64;
        let beat_offset = grid.first_beat_sec as f64 * sr;
        let beat_interval = grid.beat_interval_sec as f64 * sr;
        let _ = tx.send(EngineMsg::Load {
            deck,
            buffer: Arc::new(buffer),
            beat_offset,
            beat_interval,
        });
        // Apply loudness normalization right after the load resets the deck's gain to neutral.
        let _ = tx.send(EngineMsg::SetDeckReplayGain {
            deck,
            gain: replay_gain,
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

// ----------------------------------------------------------------------------------
// Library + per-track state (SQLite)
// ----------------------------------------------------------------------------------

/// Lock the connection and run `f`, mapping any error to a `String` for the IPC boundary.
fn with_db<T>(
    db: &State<'_, db::Db>,
    f: impl FnOnce(&rusqlite::Connection) -> rusqlite::Result<T>,
) -> Result<T, String> {
    let conn = db.0.lock().map_err(|e| e.to_string())?;
    f(&conn).map_err(|e| e.to_string())
}

#[tauri::command]
fn db_list_tracks(db: State<'_, db::Db>) -> Result<Vec<db::TrackRow>, String> {
    with_db(&db, db::list_tracks)
}

/// Probe a file's header, insert it into the library (no-op if already present), return its row.
#[tauri::command]
fn db_add_track(db: State<'_, db::Db>, path: String) -> Result<db::TrackRow, String> {
    let probed = probe_track(path.clone())?;
    with_db(&db, |c| {
        db::add_track(
            c,
            &probed.path,
            &probed.title,
            &probed.artist,
            probed.duration_ms as i64,
        )
    })
}

#[tauri::command]
fn db_remove_track(db: State<'_, db::Db>, path: String) -> Result<(), String> {
    with_db(&db, |c| db::remove_track(c, &path))
}

#[tauri::command]
fn db_track_state(db: State<'_, db::Db>, path: String) -> Result<db::TrackState, String> {
    with_db(&db, |c| db::track_state(c, &path))
}

#[tauri::command]
#[allow(clippy::too_many_arguments)]
fn db_upsert_analysis(
    db: State<'_, db::Db>,
    path: String,
    bpm: f64,
    bpm_confidence: f64,
    first_beat_sec: f64,
    beat_interval_sec: f64,
    key_camelot: String,
    key_name: String,
) -> Result<(), String> {
    with_db(&db, |c| {
        db::upsert_analysis(
            c,
            &path,
            bpm,
            bpm_confidence,
            first_beat_sec,
            beat_interval_sec,
            &key_camelot,
            &key_name,
        )
    })
}

#[tauri::command]
fn db_set_cue(db: State<'_, db::Db>, path: String, slot: i64, frame: f64) -> Result<(), String> {
    with_db(&db, |c| db::set_cue(c, &path, slot, frame))
}

#[tauri::command]
fn db_clear_cue(db: State<'_, db::Db>, path: String, slot: i64) -> Result<(), String> {
    with_db(&db, |c| db::clear_cue(c, &path, slot))
}

#[tauri::command]
fn db_set_loop(
    db: State<'_, db::Db>,
    path: String,
    slot: i64,
    in_frame: f64,
    out_frame: f64,
    beats: Option<f64>,
) -> Result<(), String> {
    with_db(&db, |c| {
        db::set_loop(c, &path, slot, in_frame, out_frame, beats)
    })
}

#[tauri::command]
fn db_clear_loop(db: State<'_, db::Db>, path: String, slot: i64) -> Result<(), String> {
    with_db(&db, |c| db::clear_loop(c, &path, slot))
}

#[tauri::command]
fn db_set_grid_offset(db: State<'_, db::Db>, path: String, sec: f64) -> Result<(), String> {
    with_db(&db, |c| db::set_grid_offset(c, &path, sec))
}

#[tauri::command]
fn db_set_gain(db: State<'_, db::Db>, path: String, gain: f64) -> Result<(), String> {
    with_db(&db, |c| db::set_gain(c, &path, gain))
}

#[tauri::command]
fn db_record_play(db: State<'_, db::Db>, path: String) -> Result<(), String> {
    with_db(&db, |c| db::record_play(c, &path))
}

#[tauri::command]
fn db_history(db: State<'_, db::Db>, limit: i64) -> Result<Vec<db::HistoryRow>, String> {
    with_db(&db, |c| db::history(c, limit))
}

/// Search the library with the query grammar (`bpm:120-128 key:8A artist:foo -live`).
#[tauri::command]
fn db_search(db: State<'_, db::Db>, query: String) -> Result<Vec<db::TrackRow>, String> {
    with_db(&db, |c| db::search_tracks(c, &query))
}

#[tauri::command]
fn db_create_crate(db: State<'_, db::Db>, name: String, is_playlist: bool) -> Result<i64, String> {
    with_db(&db, |c| db::create_crate(c, &name, is_playlist))
}

#[tauri::command]
fn db_delete_crate(db: State<'_, db::Db>, id: i64) -> Result<(), String> {
    with_db(&db, |c| db::delete_crate(c, id))
}

#[tauri::command]
fn db_list_crates(db: State<'_, db::Db>) -> Result<Vec<db::CrateRow>, String> {
    with_db(&db, db::list_crates)
}

#[tauri::command]
fn db_add_to_crate(db: State<'_, db::Db>, crate_id: i64, path: String) -> Result<(), String> {
    with_db(&db, |c| db::add_to_crate(c, crate_id, &path))
}

#[tauri::command]
fn db_remove_from_crate(db: State<'_, db::Db>, crate_id: i64, path: String) -> Result<(), String> {
    with_db(&db, |c| db::remove_from_crate(c, crate_id, &path))
}

#[tauri::command]
fn db_crate_tracks(db: State<'_, db::Db>, crate_id: i64) -> Result<Vec<db::TrackRow>, String> {
    with_db(&db, |c| db::crate_tracks(c, crate_id))
}

/// Suggest the next tracks to mix after `current_path`, ranked by harmonic + tempo compatibility
/// (the auto-mix / set-construction planner). Returns up to `limit` library tracks, best first.
#[tauri::command]
fn db_plan_next(
    db: State<'_, db::Db>,
    current_path: String,
    limit: usize,
) -> Result<Vec<db::TrackRow>, String> {
    let tracks = with_db(&db, db::list_tracks)?;
    let to_info = |t: &db::TrackRow| compas_core::TrackInfo {
        bpm: t.bpm.map(|b| b as f32),
        camelot: t.key_camelot.clone(),
    };
    let current = tracks
        .iter()
        .find(|t| t.path == current_path)
        .map(&to_info)
        .unwrap_or_default();
    let pool: Vec<&db::TrackRow> = tracks.iter().filter(|t| t.path != current_path).collect();
    let infos: Vec<compas_core::TrackInfo> = pool.iter().map(|t| to_info(t)).collect();
    let ranked = compas_core::plan_next(&current, &infos);
    Ok(ranked
        .into_iter()
        .take(limit.max(1))
        .map(|(i, _)| pool[i].clone())
        .collect())
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

/// Update a deck's beatgrid (seconds → engine frames) after a manual grid-anchor nudge, so
/// the sync PLL aligns to the grid the user sees.
#[tauri::command]
fn set_beatgrid(
    state: State<'_, EngineHandle>,
    deck: usize,
    offset_frames: f64,
    interval_frames: f64,
) -> Result<(), String> {
    state.send(EngineMsg::Beatgrid {
        deck,
        offset: offset_frames,
        interval: interval_frames,
    })
}

/// Route a deck to a crossfader side for 4-deck mixing: 0 = A, 1 = thru, 2 = B.
#[tauri::command]
fn set_deck_xfader_assign(
    state: State<'_, EngineHandle>,
    deck: usize,
    assign: u8,
) -> Result<(), String> {
    state.send(EngineMsg::DeckXfaderAssign { deck, assign })
}

/// Engage/disengage continuous beat-sync: `master` is the deck to follow, or `null` for off.
#[tauri::command]
fn set_deck_sync(
    state: State<'_, EngineHandle>,
    deck: usize,
    master: Option<usize>,
) -> Result<(), String> {
    state.send(EngineMsg::DeckSync { deck, master })
}

/// Toggle key-lock (master tempo) on a deck: tempo changes preserve the original pitch.
#[tauri::command]
fn set_deck_keylock(
    state: State<'_, EngineHandle>,
    deck: usize,
    active: bool,
) -> Result<(), String> {
    state.send(EngineMsg::DeckKeylock { deck, active })
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
fn set_loop_active(
    state: State<'_, EngineHandle>,
    deck: usize,
    active: bool,
) -> Result<(), String> {
    state.send(EngineMsg::LoopActive { deck, active })
}

/// Momentary loop-roll with slip. Engage with `active = true` and a grid-snapped region;
/// release with `active = false` to drop back in where the track would be.
#[tauri::command]
fn set_loop_roll(
    state: State<'_, EngineHandle>,
    deck: usize,
    in_frame: f64,
    out_frame: f64,
    active: bool,
) -> Result<(), String> {
    state.send(EngineMsg::LoopRoll {
        deck,
        in_frame,
        out_frame,
        active,
    })
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

/// Configure the per-deck reverb insert. `room_size` (0..1) sets the tail length, `mix`
/// the wet/dry balance.
#[tauri::command]
fn set_deck_reverb(
    state: State<'_, EngineHandle>,
    deck: usize,
    active: bool,
    room_size: f32,
    mix: f32,
) -> Result<(), String> {
    state.send(EngineMsg::DeckReverb {
        deck,
        active,
        room_size,
        mix,
    })
}

/// Configure the per-deck flanger insert. `rate_hz` sweeps the LFO, `depth` (0..1) the sweep
/// width, plus feedback/resonance and wet `mix`.
#[tauri::command]
fn set_deck_flanger(
    state: State<'_, EngineHandle>,
    deck: usize,
    active: bool,
    rate_hz: f32,
    depth: f32,
    feedback: f32,
    mix: f32,
) -> Result<(), String> {
    state.send(EngineMsg::DeckFlanger {
        deck,
        active,
        rate_hz,
        depth,
        feedback,
        mix,
    })
}

/// Configure the per-deck bitcrusher insert. `bits` (1..16) sets quantisation, `downsample`
/// (1..64) the sample-and-hold factor, plus wet `mix`.
#[tauri::command]
fn set_deck_crusher(
    state: State<'_, EngineHandle>,
    deck: usize,
    active: bool,
    bits: f32,
    downsample: u32,
    mix: f32,
) -> Result<(), String> {
    state.send(EngineMsg::DeckCrusher {
        deck,
        active,
        bits,
        downsample,
        mix,
    })
}

#[tauri::command]
fn set_crossfader(state: State<'_, EngineHandle>, value: f32) -> Result<(), String> {
    state.send(EngineMsg::SetCrossfader(value))
}

/// Configure the crossfader response. `mode`: 0 = constant-power (smooth), 1 = additive (cut).
#[tauri::command]
fn set_crossfader_config(
    state: State<'_, EngineHandle>,
    curve: f32,
    mode: u8,
    reverse: bool,
) -> Result<(), String> {
    state.send(EngineMsg::SetCrossfaderConfig {
        curve,
        mode,
        reverse,
    })
}

/// Select a deck's main CUE button behavior. `mode`: 0 = CDJ, 1 = gated/stutter.
#[tauri::command]
fn set_cue_mode(state: State<'_, EngineHandle>, deck: usize, mode: u8) -> Result<(), String> {
    state.send(EngineMsg::SetCueMode { deck, mode })
}

/// Set a deck's main cue point (source frames).
#[tauri::command]
fn set_cue_point(state: State<'_, EngineHandle>, deck: usize, frame: f64) -> Result<(), String> {
    state.send(EngineMsg::SetCuePoint { deck, frame })
}

/// Press (`pressed = true`) or release the main CUE button; drives the cue state machine.
#[tauri::command]
fn cue_button(state: State<'_, EngineHandle>, deck: usize, pressed: bool) -> Result<(), String> {
    state.send(EngineMsg::CueButton { deck, pressed })
}

/// Scale the active loop length (0.5 = halve, 2.0 = double).
#[tauri::command]
fn scale_loop(state: State<'_, EngineHandle>, deck: usize, factor: f64) -> Result<(), String> {
    state.send(EngineMsg::ScaleLoop { deck, factor })
}

/// Shift the loop region (and play-head) by `delta_frames`.
#[tauri::command]
fn move_loop(state: State<'_, EngineHandle>, deck: usize, delta_frames: f64) -> Result<(), String> {
    state.send(EngineMsg::MoveLoop { deck, delta_frames })
}

/// Set a follower's sync mode: 0 = full tempo+phase, 1 = tempo-only.
#[tauri::command]
fn set_deck_sync_mode(state: State<'_, EngineHandle>, deck: usize, mode: u8) -> Result<(), String> {
    state.send(EngineMsg::SetDeckSyncMode { deck, mode })
}

/// Mark/unmark a deck as the explicit (pinned) sync leader.
#[tauri::command]
fn set_sync_leader(
    state: State<'_, EngineHandle>,
    deck: usize,
    explicit: bool,
) -> Result<(), String> {
    state.send(EngineMsg::SetSyncLeader { deck, explicit })
}

/// Auto-pick the best leader and make `deck` follow it.
#[tauri::command]
fn sync_to_leader(state: State<'_, EngineHandle>, deck: usize) -> Result<(), String> {
    state.send(EngineMsg::SyncToLeader { deck })
}

/// Set a deck's loudness-normalization factor (1.0 = off). Use to override or disable the
/// auto-computed ReplayGain.
#[tauri::command]
fn set_deck_replay_gain(
    state: State<'_, EngineHandle>,
    deck: usize,
    gain: f32,
) -> Result<(), String> {
    state.send(EngineMsg::SetDeckReplayGain { deck, gain })
}

/// Drive a deck's FX macro (super-knob), `value` 0..1.
#[tauri::command]
fn set_deck_fx_macro(
    state: State<'_, EngineHandle>,
    deck: usize,
    value: f32,
) -> Result<(), String> {
    state.send(EngineMsg::SetDeckFxMacro { deck, value })
}

#[tauri::command]
fn set_master_gain(state: State<'_, EngineHandle>, value: f32) -> Result<(), String> {
    state.send(EngineMsg::SetMasterGain(value))
}

// ----------------------------------------------------------------------------------
// Master recording (audio thread taps master → ring → this writer thread → WAV)
// ----------------------------------------------------------------------------------

/// Recording ring capacity (f32 samples) — ~5 s of stereo @ 48 kHz, ample slack for the
/// writer thread's scheduling so the audio thread never has to drop frames.
const RECORD_RING_CAPACITY: usize = 1 << 19;

#[tauri::command]
fn start_recording(state: State<'_, EngineHandle>, path: String) -> Result<(), String> {
    if state.recording.swap(true, Ordering::SeqCst) {
        return Err("already recording".into());
    }
    let file = match File::create(&path) {
        Ok(f) => f,
        Err(e) => {
            state.recording.store(false, Ordering::SeqCst);
            return Err(format!("could not create file: {e}"));
        }
    };
    let (producer, consumer) = RingBuffer::<f32>::new(RECORD_RING_CAPACITY);
    let sample_rate = state.audio.sample_rate.load(Ordering::Relaxed);
    if sample_rate == 0 {
        state.recording.store(false, Ordering::SeqCst);
        return Err("audio output is offline; cannot start recording".into());
    }
    spawn_wav_writer(consumer, file, sample_rate);
    if let Err(e) = state.send(EngineMsg::StartRecording { sink: producer }) {
        state.recording.store(false, Ordering::SeqCst);
        return Err(e);
    }
    tracing::info!("recording started → {path}");
    Ok(())
}

#[tauri::command]
fn stop_recording(state: State<'_, EngineHandle>) -> Result<(), String> {
    state.recording.store(false, Ordering::SeqCst);
    state.send(EngineMsg::StopRecording)
}

/// Stream f32 master samples from `consumer` into a 32-bit-float stereo WAV, finalizing the
/// RIFF/data chunk sizes once the producer is dropped (StopRecording). Dedicated thread; it
/// never touches the audio thread.
fn spawn_wav_writer(mut consumer: Consumer<f32>, file: File, sample_rate: u32) {
    thread::spawn(move || {
        let mut w = BufWriter::new(file);
        if let Err(e) = write_wav_header(&mut w, sample_rate) {
            tracing::error!("recording: WAV header write failed: {e}");
            return;
        }
        let mut data_bytes: u32 = 0;
        loop {
            match drain_into(&mut consumer, &mut w) {
                Ok(n) => data_bytes = data_bytes.saturating_add(n),
                Err(e) => {
                    tracing::error!("recording: write error: {e}");
                    return;
                }
            }
            if consumer.is_abandoned() {
                // Final drain of anything pushed just before the producer dropped.
                if let Ok(n) = drain_into(&mut consumer, &mut w) {
                    data_bytes = data_bytes.saturating_add(n);
                }
                break;
            }
            thread::sleep(Duration::from_millis(8));
        }
        match finalize_wav(w, data_bytes) {
            Ok(()) => tracing::info!("recording: saved ({data_bytes} bytes of audio)"),
            Err(e) => tracing::error!("recording: finalize failed: {e}"),
        }
    });
}

/// Pop all available samples and write them as little-endian f32. Returns bytes written.
fn drain_into<W: Write>(consumer: &mut Consumer<f32>, w: &mut W) -> std::io::Result<u32> {
    let mut n = 0u32;
    while let Ok(s) = consumer.pop() {
        w.write_all(&s.to_le_bytes())?;
        n = n.saturating_add(4);
    }
    Ok(n)
}

/// Write a 32-bit-float stereo WAV header with placeholder sizes (patched in `finalize_wav`).
fn write_wav_header(w: &mut impl Write, sample_rate: u32) -> std::io::Result<()> {
    let channels: u16 = 2;
    let bits: u16 = 32;
    let block_align = channels * (bits / 8);
    let byte_rate = sample_rate * block_align as u32;
    w.write_all(b"RIFF")?;
    w.write_all(&0u32.to_le_bytes())?; // RIFF chunk size — patched at finalize
    w.write_all(b"WAVE")?;
    w.write_all(b"fmt ")?;
    w.write_all(&16u32.to_le_bytes())?; // fmt chunk size
    w.write_all(&3u16.to_le_bytes())?; // format = 3 (IEEE float)
    w.write_all(&channels.to_le_bytes())?;
    w.write_all(&sample_rate.to_le_bytes())?;
    w.write_all(&byte_rate.to_le_bytes())?;
    w.write_all(&block_align.to_le_bytes())?;
    w.write_all(&bits.to_le_bytes())?;
    w.write_all(b"data")?;
    w.write_all(&0u32.to_le_bytes())?; // data chunk size — patched at finalize
    Ok(())
}

/// Patch the RIFF and data chunk sizes now that the total byte count is known.
fn finalize_wav(w: BufWriter<File>, data_bytes: u32) -> std::io::Result<()> {
    let mut file = w.into_inner().map_err(|e| e.into_error())?;
    file.seek(SeekFrom::Start(4))?;
    file.write_all(&36u32.saturating_add(data_bytes).to_le_bytes())?;
    file.seek(SeekFrom::Start(40))?;
    file.write_all(&data_bytes.to_le_bytes())?;
    file.flush()?;
    Ok(())
}

// ----------------------------------------------------------------------------------
// Headphone / cue monitoring (2nd output stream fed by the mixer's PFL bus)
// ----------------------------------------------------------------------------------

/// Cue ring capacity (f32 samples) — ~340 ms of stereo @ 48 kHz, slack to ride clock drift
/// between the master and cue device clocks.
const CUE_RING_CAPACITY: usize = 1 << 16;

/// Holds the signal that keeps the active cue-output thread alive. Dropping the sender (or
/// replacing it) tells that thread to drop its `cpal::Stream` and exit. `None` = cue off.
struct CueState {
    stop: Mutex<Option<Sender<()>>>,
}

/// List available output devices the headphone cue can target (first is usually the default).
#[tauri::command]
fn list_output_devices() -> Vec<String> {
    compas_audio::output_device_names()
}

/// Stop the active cue-output thread (if any) and tell the mixer to stop pushing.
fn stop_cue_internal(state: &EngineHandle, cue: &CueState) {
    let _ = state.send(EngineMsg::StopCueOutput);
    if let Ok(mut guard) = cue.stop.lock() {
        guard.take(); // dropping the sender ends the thread's recv() → it drops the stream
    }
}

/// Start headphone cue monitoring on `device` (None = default output). The mixer pushes the
/// PFL/cue blend into a ring; a dedicated thread drains it through a 2nd cpal output stream.
/// Returns the opened device's name.
#[tauri::command]
fn start_cue_output(
    state: State<'_, EngineHandle>,
    cue: State<'_, CueState>,
    device: Option<String>,
) -> Result<String, String> {
    stop_cue_internal(&state, &cue); // replace any existing cue output

    let (producer, consumer) = RingBuffer::<f32>::new(CUE_RING_CAPACITY);
    // Start the mixer pushing before opening the device so the prime buffer fills promptly.
    state.send(EngineMsg::StartCueOutput { sink: producer })?;

    let (stop_tx, stop_rx) = channel::<()>();
    let (ready_tx, ready_rx) = channel::<Result<String, String>>();
    let spawn = thread::Builder::new()
        .name("compas-cue".to_string())
        .spawn(
            move || match compas_audio::open_cue_output(device.as_deref(), consumer) {
                Ok(out) => {
                    let _ = ready_tx.send(Ok(out.device_name.clone()));
                    let _ = stop_rx.recv(); // park until told to stop (or the sender is dropped)
                    drop(out); // closes the cue stream
                }
                Err(e) => {
                    let _ = ready_tx.send(Err(e.to_string()));
                }
            },
        );
    if let Err(e) = spawn {
        let _ = state.send(EngineMsg::StopCueOutput);
        return Err(format!("could not spawn cue thread: {e}"));
    }

    match ready_rx.recv() {
        Ok(Ok(name)) => {
            if let Ok(mut guard) = cue.stop.lock() {
                *guard = Some(stop_tx);
            }
            tracing::info!("cue output started → {name}");
            Ok(name)
        }
        Ok(Err(e)) => {
            let _ = state.send(EngineMsg::StopCueOutput);
            Err(e)
        }
        Err(_) => {
            let _ = state.send(EngineMsg::StopCueOutput);
            Err("cue output thread exited before reporting".into())
        }
    }
}

#[tauri::command]
fn stop_cue_output(state: State<'_, EngineHandle>, cue: State<'_, CueState>) -> Result<(), String> {
    stop_cue_internal(&state, &cue);
    Ok(())
}

/// Toggle pre-fader-listen (PFL) for a deck on the headphone cue bus.
#[tauri::command]
fn set_deck_cue(state: State<'_, EngineHandle>, deck: usize, active: bool) -> Result<(), String> {
    state.send(EngineMsg::DeckCue { deck, active })
}

/// Headphone cue/master blend (0 = cue only, 1 = master only).
#[tauri::command]
fn set_cue_mix(state: State<'_, EngineHandle>, value: f32) -> Result<(), String> {
    state.send(EngineMsg::CueMix(value))
}

/// Headphone output level.
#[tauri::command]
fn set_cue_volume(state: State<'_, EngineHandle>, value: f32) -> Result<(), String> {
    state.send(EngineMsg::CueVolume(value))
}

// ----------------------------------------------------------------------------------
// Synth instrument + MIDI input
// ----------------------------------------------------------------------------------

/// Holds the live MIDI input connection (kept alive so its callback keeps firing) plus a
/// shared flag deciding whether incoming notes drive the synth. With it off, a controller's
/// pads/keys can be mapped to deck controls (via `midi:note`) without honking the synth.
struct MidiState {
    conn: Mutex<Option<MidiInputConnection<()>>>,
    synth: Arc<AtomicBool>,
}

#[derive(Serialize, Clone)]
struct MidiCcEvent {
    channel: u8,
    controller: u8,
    value: u8,
}

#[derive(Serialize, Clone)]
struct MidiNoteEvent {
    channel: u8,
    note: u8,
    velocity: u8,
    on: bool,
}

#[tauri::command]
fn note_on(state: State<'_, EngineHandle>, note: u8, velocity: u8) -> Result<(), String> {
    state.send(EngineMsg::NoteOn { note, velocity })
}

#[tauri::command]
fn note_off(state: State<'_, EngineHandle>, note: u8) -> Result<(), String> {
    state.send(EngineMsg::NoteOff { note })
}

#[tauri::command]
fn all_notes_off(state: State<'_, EngineHandle>) -> Result<(), String> {
    state.send(EngineMsg::AllNotesOff)
}

#[tauri::command]
fn set_synth_waveform(state: State<'_, EngineHandle>, index: u8) -> Result<(), String> {
    state.send(EngineMsg::SynthWaveform { index })
}

#[tauri::command]
fn set_synth_gain(state: State<'_, EngineHandle>, gain: f32) -> Result<(), String> {
    state.send(EngineMsg::SynthGain { gain })
}

// ----------------------------------------------------------------------------------
// Sampler / performance pads
// ----------------------------------------------------------------------------------

#[derive(Serialize)]
struct LoadedSample {
    slot: usize,
    name: String,
}

/// Decode an audio file into a sampler pad. Decoding is synchronous (samples are short); the
/// whole-file PCM is installed on the audio thread via [`EngineMsg::LoadSample`].
#[tauri::command]
fn load_sample(
    state: State<'_, EngineHandle>,
    slot: usize,
    path: String,
) -> Result<LoadedSample, String> {
    let (buffer, metadata) = compas_sources::decode_full(&path).map_err(|e| e.to_string())?;
    // Prefer the embedded title; fall back to the file stem.
    let name = if metadata.title.trim().is_empty() {
        std::path::Path::new(&path)
            .file_stem()
            .map(|s| s.to_string_lossy().into_owned())
            .unwrap_or_else(|| "sample".into())
    } else {
        metadata.title.clone()
    };
    state.send(EngineMsg::LoadSample {
        slot,
        buffer: Arc::new(buffer),
    })?;
    Ok(LoadedSample { slot, name })
}

#[tauri::command]
fn clear_sample(state: State<'_, EngineHandle>, slot: usize) -> Result<(), String> {
    state.send(EngineMsg::ClearSample { slot })
}

#[tauri::command]
fn trigger_sample(state: State<'_, EngineHandle>, slot: usize, velocity: u8) -> Result<(), String> {
    state.send(EngineMsg::TriggerSample { slot, velocity })
}

#[tauri::command]
fn stop_sample(state: State<'_, EngineHandle>, slot: usize) -> Result<(), String> {
    state.send(EngineMsg::StopSample { slot })
}

#[tauri::command]
fn set_sample_loop(
    state: State<'_, EngineHandle>,
    slot: usize,
    looping: bool,
) -> Result<(), String> {
    state.send(EngineMsg::SampleLoop { slot, looping })
}

#[tauri::command]
fn set_sampler_gain(state: State<'_, EngineHandle>, gain: f32) -> Result<(), String> {
    state.send(EngineMsg::SamplerGain { gain })
}

/// Number of sampler pads, so the UI lays out the right number without hard-coding it.
#[tauri::command]
fn sampler_pad_count() -> usize {
    compas_audio::NUM_SAMPLER_PADS
}

/// List connected MIDI input port names (index = position in this list).
#[tauri::command]
fn midi_list_ports() -> Result<Vec<String>, String> {
    let midi_in = MidiInput::new("compas-probe").map_err(|e| e.to_string())?;
    Ok(midi_in
        .ports()
        .iter()
        .filter_map(|p| midi_in.port_name(p).ok())
        .collect())
}

/// The full list of mappable controls (the control-bus registry) for the learn editor.
#[tauri::command]
fn controller_registry() -> Vec<compas_core::ControlSpec> {
    compas_core::Registry::defaults(compas_audio::NUM_DECKS)
        .iter()
        .cloned()
        .collect()
}

/// List controller profiles in the user controller directory.
#[tauri::command]
fn controller_list(app: AppHandle) -> Result<Vec<compas_core::ControllerProfile>, String> {
    let base = app.path().app_data_dir().map_err(|e| e.to_string())?;
    let user_dir = controllers::profiles_dir(&base).map_err(|e| e.to_string())?;
    // Bundled starter profiles ship under <resources>/resources/controllers.
    let bundled = app
        .path()
        .resource_dir()
        .ok()
        .map(|r| r.join("resources").join("controllers"));
    Ok(controllers::list_merged(bundled.as_deref(), &user_dir))
}

/// Save (or overwrite) a controller profile (used by the guided learn editor).
#[tauri::command]
fn controller_save(app: AppHandle, profile: compas_core::ControllerProfile) -> Result<(), String> {
    let base = app.path().app_data_dir().map_err(|e| e.to_string())?;
    let dir = controllers::profiles_dir(&base).map_err(|e| e.to_string())?;
    controllers::save_profile(&dir, &profile).map(|_| ())
}

/// Activate a controller profile (declarative bindings + optional script) in the controller engine.
#[tauri::command]
fn controller_activate(
    ctrl: State<'_, controllers::ControllerEngine>,
    profile: compas_core::ControllerProfile,
) -> Result<(), String> {
    ctrl.send(controllers::ControllerMsg::Activate(Box::new(profile)));
    Ok(())
}

/// Drop the active controller profile.
#[tauri::command]
fn controller_deactivate(ctrl: State<'_, controllers::ControllerEngine>) {
    ctrl.send(controllers::ControllerMsg::Deactivate);
}

/// Reflect a control's current engine value back onto the device (LED rings, motor faders). The
/// frontend calls this whenever a control changes — from the UI or the controller — so the hardware
/// tracks software state. No-op unless a profile + output port are active (handled engine-side).
#[tauri::command]
fn controller_feedback(
    ctrl: State<'_, controllers::ControllerEngine>,
    control: String,
    value: f64,
) {
    ctrl.send(controllers::ControllerMsg::Feedback { control, value });
}

/// List connected HID devices (for the controller picker / learn editor).
#[tauri::command]
fn hid_list() -> Result<Vec<hid::HidDeviceInfo>, String> {
    hid::list_devices()
}

/// Open an HID device by path; its changed report bytes drive the active profile's `hid` bindings
/// (and surface as `hid:input` events). Replaces any existing HID connection.
#[tauri::command]
fn hid_connect(
    app: AppHandle,
    ctrl: State<'_, controllers::ControllerEngine>,
    hid_state: State<'_, hid::HidState>,
    path: String,
) -> Result<(), String> {
    let conn = hid::HidConnection::open(app, ctrl.sender(), path)?;
    *hid_state.0.lock().map_err(|e| e.to_string())? = Some(conn);
    Ok(())
}

/// Close the active HID connection (stops the reader thread).
#[tauri::command]
fn hid_disconnect(hid_state: State<'_, hid::HidState>) -> Result<(), String> {
    *hid_state.0.lock().map_err(|e| e.to_string())? = None;
    Ok(())
}

/// Open a MIDI input port; its messages drive the synth (notes) and emit `midi:cc` (knobs).
#[tauri::command]
fn midi_connect(
    app: AppHandle,
    engine: State<'_, EngineHandle>,
    midi: State<'_, MidiState>,
    ctrl: State<'_, controllers::ControllerEngine>,
    index: usize,
) -> Result<String, String> {
    let ctrl_tx = ctrl.sender();
    let midi_in = MidiInput::new("compas").map_err(|e| e.to_string())?;
    let ports = midi_in.ports();
    let port = ports.get(index).ok_or("invalid MIDI port index")?.clone();
    let name = midi_in.port_name(&port).unwrap_or_else(|_| "MIDI".into());

    let tx = engine.tx.clone();
    let app_cc = app.clone();
    let synth = midi.synth.clone();
    let conn = midi_in
        .connect(
            &port,
            "compas-in",
            move |_t, message, _| {
                if message.len() < 2 {
                    return;
                }
                // Forward the raw message to the controller engine (active profile maps it to
                // controls and emits controller:update).
                let _ = ctrl_tx.send(controllers::ControllerMsg::Midi(
                    message[0],
                    message[1],
                    *message.get(2).unwrap_or(&0),
                ));
                // Every note/CC is forwarded to the frontend so the MIDI-mapping layer can
                // bind any source to a deck control; notes additionally drive the synth when
                // its routing flag is on (the instrument panel owns that toggle).
                let channel = message[0] & 0x0F;
                match message[0] & 0xF0 {
                    0x90 => {
                        let (note, vel) = (message[1], *message.get(2).unwrap_or(&0));
                        let on = vel > 0;
                        let _ = app_cc.emit(
                            "midi:note",
                            MidiNoteEvent {
                                channel,
                                note,
                                velocity: vel,
                                on,
                            },
                        );
                        if synth.load(Ordering::Relaxed) {
                            let _ = if on {
                                tx.send(EngineMsg::NoteOn {
                                    note,
                                    velocity: vel,
                                })
                            } else {
                                tx.send(EngineMsg::NoteOff { note })
                            };
                        }
                    }
                    0x80 => {
                        let note = message[1];
                        let _ = app_cc.emit(
                            "midi:note",
                            MidiNoteEvent {
                                channel,
                                note,
                                velocity: 0,
                                on: false,
                            },
                        );
                        if synth.load(Ordering::Relaxed) {
                            let _ = tx.send(EngineMsg::NoteOff { note });
                        }
                    }
                    0xB0 => {
                        let _ = app_cc.emit(
                            "midi:cc",
                            MidiCcEvent {
                                channel,
                                controller: message[1],
                                value: *message.get(2).unwrap_or(&0),
                            },
                        );
                    }
                    _ => {}
                }
            },
            (),
        )
        .map_err(|e| e.to_string())?;

    *midi.conn.lock().map_err(|e| e.to_string())? = Some(conn);
    // Open a matching MIDI output (same device name) for LED/feedback echo.
    ctrl.send(controllers::ControllerMsg::SetOutputPort(Some(
        name.clone(),
    )));
    tracing::info!("MIDI connected: {name}");
    Ok(name)
}

/// Toggle whether incoming MIDI notes drive the synth. The instrument panel enables this
/// while it is open; otherwise notes only surface as `midi:note` for control mapping.
#[tauri::command]
fn set_midi_synth(midi: State<'_, MidiState>, enabled: bool) {
    midi.synth.store(enabled, Ordering::Relaxed);
}

/// Close the MIDI input connection (releases all held notes).
#[tauri::command]
fn midi_disconnect(
    state: State<'_, EngineHandle>,
    midi: State<'_, MidiState>,
) -> Result<(), String> {
    if let Ok(mut guard) = midi.conn.lock() {
        guard.take(); // dropping the connection closes the port
    }
    let _ = state.send(EngineMsg::AllNotesOff);
    Ok(())
}

/// Spawn the telemetry emitter: samples lock-free deck state and emits `deck:position`.
fn spawn_telemetry(app: AppHandle, telemetry: Arc<DeckTelemetry>) {
    thread::spawn(move || {
        let period = Duration::from_millis(1000 / TELEMETRY_HZ);
        loop {
            for deck in 0..4 {
                if telemetry.is_loaded(deck) {
                    let _ = app.emit(
                        "deck:position",
                        DeckPositionEvent {
                            deck,
                            frame: telemetry.playhead_frames(deck),
                            playing: telemetry.is_playing(deck),
                            level: telemetry.deck_level(deck),
                            rate: telemetry.deck_rate(deck),
                            latency_secs: telemetry.output_latency_secs(),
                        },
                    );
                }
            }
            let (l, r) = telemetry.master_level();
            let _ = app.emit("master:level", MasterMeterEvent { l, r });
            let _ = app.emit(
                "engine:load",
                EngineLoadEvent {
                    load: telemetry.rt_load(),
                    xruns: telemetry.xruns(),
                },
            );
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
        .plugin(tauri_plugin_updater::Builder::new().build())
        .plugin(tauri_plugin_process::init())
        .manage(engine)
        .manage(MidiState {
            conn: Mutex::new(None),
            synth: Arc::new(AtomicBool::new(false)),
        })
        .manage(CueState {
            stop: Mutex::new(None),
        })
        .manage(hid::HidState::default())
        .setup(move |app| {
            spawn_telemetry(app.handle().clone(), telemetry.clone());
            // Controller engine: owns the script runtime + active mapping; emits controller:update.
            app.manage(controllers::ControllerEngine::spawn(app.handle().clone()));
            // Open the library DB in the app-data dir. If it fails, log and carry on —
            // DB-backed commands will then error and the frontend degrades gracefully.
            match app.path().app_data_dir() {
                Ok(dir) => {
                    let _ = std::fs::create_dir_all(&dir);
                    match db::open(dir.join("compas.db")) {
                        Ok(database) => {
                            app.manage(database);
                        }
                        Err(e) => tracing::error!("library DB open failed: {e}"),
                    }
                }
                Err(e) => tracing::error!("no app-data dir for library DB: {e}"),
            }
            Ok(())
        })
        .invoke_handler(tauri::generate_handler![
            build_info,
            engine_status,
            probe_track,
            db_list_tracks,
            db_add_track,
            db_remove_track,
            db_track_state,
            db_upsert_analysis,
            db_set_cue,
            db_clear_cue,
            db_set_loop,
            db_clear_loop,
            db_set_grid_offset,
            db_set_gain,
            db_record_play,
            db_history,
            db_search,
            db_create_crate,
            db_delete_crate,
            db_list_crates,
            db_add_to_crate,
            db_remove_from_crate,
            db_crate_tracks,
            db_plan_next,
            load_track,
            deck_play,
            deck_pause,
            deck_seek,
            deck_unload,
            set_loop,
            set_loop_active,
            set_loop_roll,
            deck_scratch,
            set_deck_tempo,
            set_deck_keylock,
            set_beatgrid,
            set_deck_sync,
            set_deck_xfader_assign,
            set_deck_gain,
            set_deck_eq,
            set_deck_filter,
            set_deck_echo,
            set_deck_reverb,
            set_deck_flanger,
            set_deck_crusher,
            set_crossfader,
            set_crossfader_config,
            set_cue_mode,
            set_cue_point,
            cue_button,
            scale_loop,
            move_loop,
            set_deck_sync_mode,
            set_sync_leader,
            sync_to_leader,
            set_deck_replay_gain,
            set_deck_fx_macro,
            set_master_gain,
            start_recording,
            stop_recording,
            list_output_devices,
            start_cue_output,
            stop_cue_output,
            set_deck_cue,
            set_cue_mix,
            set_cue_volume,
            note_on,
            note_off,
            all_notes_off,
            set_synth_waveform,
            set_synth_gain,
            load_sample,
            clear_sample,
            trigger_sample,
            stop_sample,
            set_sample_loop,
            set_sampler_gain,
            sampler_pad_count,
            midi_list_ports,
            midi_connect,
            controller_registry,
            controller_list,
            controller_save,
            controller_activate,
            controller_deactivate,
            controller_feedback,
            hid_list,
            hid_connect,
            hid_disconnect,
            midi_disconnect,
            set_midi_synth,
            spotify::spotify_listen
        ])
        .run(tauri::generate_context!());

    if let Err(e) = result {
        tracing::error!("fatal: error while running tauri application: {e}");
        std::process::exit(1);
    }
}
