//! SQLite-backed library + per-track performance state.
//!
//! Stores the track list plus everything worth surviving a restart: hot cues, the last loop,
//! the manual beatgrid nudge, a saved gain trim, and play history. This is *not* on the audio
//! path — it's plain blocking `rusqlite` behind a `Mutex`, called only from Tauri commands.
//! A track's absolute file path is its identity (matching `DeckLoadedEvent.path`).

use std::path::Path;
use std::sync::Mutex;
use std::time::{SystemTime, UNIX_EPOCH};

use rusqlite::{params, Connection, OptionalExtension};
use serde::Serialize;

/// Managed Tauri state: the single long-lived connection behind a mutex.
pub struct Db(pub Mutex<Connection>);

fn now() -> i64 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map(|d| d.as_secs() as i64)
        .unwrap_or(0)
}

const SCHEMA: &str = "
PRAGMA journal_mode = WAL;
PRAGMA foreign_keys = ON;

CREATE TABLE IF NOT EXISTS tracks (
    path              TEXT PRIMARY KEY,
    title             TEXT NOT NULL,
    artist            TEXT NOT NULL,
    duration_ms       INTEGER NOT NULL,
    bpm               REAL,
    bpm_confidence    REAL,
    first_beat_sec    REAL,
    beat_interval_sec REAL,
    key_camelot       TEXT,
    key_name          TEXT,
    grid_offset_sec   REAL NOT NULL DEFAULT 0,
    gain              REAL NOT NULL DEFAULT 1.0,
    added_at          INTEGER NOT NULL,
    last_played_at    INTEGER,
    play_count        INTEGER NOT NULL DEFAULT 0
);

CREATE TABLE IF NOT EXISTS cues (
    track_path TEXT NOT NULL REFERENCES tracks(path) ON DELETE CASCADE,
    slot       INTEGER NOT NULL,
    frame      REAL NOT NULL,
    PRIMARY KEY (track_path, slot)
);

CREATE TABLE IF NOT EXISTS loops (
    track_path TEXT NOT NULL REFERENCES tracks(path) ON DELETE CASCADE,
    slot       INTEGER NOT NULL DEFAULT 0,
    in_frame   REAL NOT NULL,
    out_frame  REAL NOT NULL,
    beats      REAL,
    PRIMARY KEY (track_path, slot)
);

CREATE TABLE IF NOT EXISTS history (
    id         INTEGER PRIMARY KEY AUTOINCREMENT,
    track_path TEXT NOT NULL,
    played_at  INTEGER NOT NULL
);
";

/// Open (creating if needed) the database at `path` and apply the schema.
pub fn open(path: impl AsRef<Path>) -> rusqlite::Result<Db> {
    let conn = Connection::open(path)?;
    conn.execute_batch(SCHEMA)?;
    Ok(Db(Mutex::new(conn)))
}

#[derive(Serialize)]
pub struct TrackRow {
    pub path: String,
    pub title: String,
    pub artist: String,
    pub duration_ms: i64,
    pub bpm: Option<f64>,
    pub key_camelot: Option<String>,
    pub key_name: Option<String>,
    pub grid_offset_sec: f64,
    pub gain: f64,
    pub play_count: i64,
    pub last_played_at: Option<i64>,
}

#[derive(Serialize)]
pub struct CueRow {
    pub slot: i64,
    pub frame: f64,
}

#[derive(Serialize)]
pub struct LoopRow {
    pub slot: i64,
    pub in_frame: f64,
    pub out_frame: f64,
    pub beats: Option<f64>,
}

/// Everything needed to restore a deck when a track is reloaded.
#[derive(Serialize)]
pub struct TrackState {
    pub grid_offset_sec: f64,
    pub gain: f64,
    pub cues: Vec<CueRow>,
    pub loops: Vec<LoopRow>,
}

#[derive(Serialize)]
pub struct HistoryRow {
    pub track_path: String,
    pub title: String,
    pub artist: String,
    pub played_at: i64,
}

fn row_to_track(row: &rusqlite::Row) -> rusqlite::Result<TrackRow> {
    Ok(TrackRow {
        path: row.get("path")?,
        title: row.get("title")?,
        artist: row.get("artist")?,
        duration_ms: row.get("duration_ms")?,
        bpm: row.get("bpm")?,
        key_camelot: row.get("key_camelot")?,
        key_name: row.get("key_name")?,
        grid_offset_sec: row.get("grid_offset_sec")?,
        gain: row.get("gain")?,
        play_count: row.get("play_count")?,
        last_played_at: row.get("last_played_at")?,
    })
}

pub fn list_tracks(c: &Connection) -> rusqlite::Result<Vec<TrackRow>> {
    let mut stmt = c.prepare(
        "SELECT path, title, artist, duration_ms, bpm, key_camelot, key_name,
                grid_offset_sec, gain, play_count, last_played_at
         FROM tracks ORDER BY added_at DESC, title ASC",
    )?;
    let rows = stmt.query_map([], row_to_track)?;
    rows.collect()
}

/// Insert a track if absent (keeping any existing analysis/cues), then return its row.
pub fn add_track(
    c: &Connection,
    path: &str,
    title: &str,
    artist: &str,
    duration_ms: i64,
) -> rusqlite::Result<TrackRow> {
    c.execute(
        "INSERT OR IGNORE INTO tracks (path, title, artist, duration_ms, added_at)
         VALUES (?1, ?2, ?3, ?4, ?5)",
        params![path, title, artist, duration_ms, now()],
    )?;
    c.query_row(
        "SELECT path, title, artist, duration_ms, bpm, key_camelot, key_name,
                grid_offset_sec, gain, play_count, last_played_at
         FROM tracks WHERE path = ?1",
        params![path],
        row_to_track,
    )
}

pub fn remove_track(c: &Connection, path: &str) -> rusqlite::Result<()> {
    // Children cascade via the FK; history is intentionally a standalone log and kept.
    c.execute("DELETE FROM tracks WHERE path = ?1", params![path])?;
    Ok(())
}

/// Cache analysis results onto an existing track row (no-op if the track isn't in the library).
#[allow(clippy::too_many_arguments)]
pub fn upsert_analysis(
    c: &Connection,
    path: &str,
    bpm: f64,
    bpm_confidence: f64,
    first_beat_sec: f64,
    beat_interval_sec: f64,
    key_camelot: &str,
    key_name: &str,
) -> rusqlite::Result<()> {
    c.execute(
        "UPDATE tracks SET bpm = ?2, bpm_confidence = ?3, first_beat_sec = ?4,
                beat_interval_sec = ?5, key_camelot = ?6, key_name = ?7
         WHERE path = ?1",
        params![
            path,
            bpm,
            bpm_confidence,
            first_beat_sec,
            beat_interval_sec,
            key_camelot,
            key_name
        ],
    )?;
    Ok(())
}

pub fn track_state(c: &Connection, path: &str) -> rusqlite::Result<TrackState> {
    let (grid_offset_sec, gain) = c
        .query_row(
            "SELECT grid_offset_sec, gain FROM tracks WHERE path = ?1",
            params![path],
            |r| Ok((r.get::<_, f64>(0)?, r.get::<_, f64>(1)?)),
        )
        .optional()?
        .unwrap_or((0.0, 1.0));

    let mut cue_stmt =
        c.prepare("SELECT slot, frame FROM cues WHERE track_path = ?1 ORDER BY slot")?;
    let cues = cue_stmt
        .query_map(params![path], |r| {
            Ok(CueRow {
                slot: r.get(0)?,
                frame: r.get(1)?,
            })
        })?
        .collect::<rusqlite::Result<Vec<_>>>()?;

    let mut loop_stmt = c
        .prepare("SELECT slot, in_frame, out_frame, beats FROM loops WHERE track_path = ?1 ORDER BY slot")?;
    let loops = loop_stmt
        .query_map(params![path], |r| {
            Ok(LoopRow {
                slot: r.get(0)?,
                in_frame: r.get(1)?,
                out_frame: r.get(2)?,
                beats: r.get(3)?,
            })
        })?
        .collect::<rusqlite::Result<Vec<_>>>()?;

    Ok(TrackState {
        grid_offset_sec,
        gain,
        cues,
        loops,
    })
}

pub fn set_cue(c: &Connection, path: &str, slot: i64, frame: f64) -> rusqlite::Result<()> {
    c.execute(
        "INSERT INTO cues (track_path, slot, frame) VALUES (?1, ?2, ?3)
         ON CONFLICT(track_path, slot) DO UPDATE SET frame = excluded.frame",
        params![path, slot, frame],
    )?;
    Ok(())
}

pub fn clear_cue(c: &Connection, path: &str, slot: i64) -> rusqlite::Result<()> {
    c.execute(
        "DELETE FROM cues WHERE track_path = ?1 AND slot = ?2",
        params![path, slot],
    )?;
    Ok(())
}

pub fn set_loop(
    c: &Connection,
    path: &str,
    slot: i64,
    in_frame: f64,
    out_frame: f64,
    beats: Option<f64>,
) -> rusqlite::Result<()> {
    c.execute(
        "INSERT INTO loops (track_path, slot, in_frame, out_frame, beats) VALUES (?1, ?2, ?3, ?4, ?5)
         ON CONFLICT(track_path, slot) DO UPDATE SET
            in_frame = excluded.in_frame, out_frame = excluded.out_frame, beats = excluded.beats",
        params![path, slot, in_frame, out_frame, beats],
    )?;
    Ok(())
}

pub fn clear_loop(c: &Connection, path: &str, slot: i64) -> rusqlite::Result<()> {
    c.execute(
        "DELETE FROM loops WHERE track_path = ?1 AND slot = ?2",
        params![path, slot],
    )?;
    Ok(())
}

pub fn set_grid_offset(c: &Connection, path: &str, sec: f64) -> rusqlite::Result<()> {
    c.execute(
        "UPDATE tracks SET grid_offset_sec = ?2 WHERE path = ?1",
        params![path, sec],
    )?;
    Ok(())
}

pub fn set_gain(c: &Connection, path: &str, gain: f64) -> rusqlite::Result<()> {
    c.execute(
        "UPDATE tracks SET gain = ?2 WHERE path = ?1",
        params![path, gain],
    )?;
    Ok(())
}

/// Bump play count + last-played and append to the history log.
pub fn record_play(c: &Connection, path: &str) -> rusqlite::Result<()> {
    let ts = now();
    c.execute(
        "UPDATE tracks SET play_count = play_count + 1, last_played_at = ?2 WHERE path = ?1",
        params![path, ts],
    )?;
    c.execute(
        "INSERT INTO history (track_path, played_at) VALUES (?1, ?2)",
        params![path, ts],
    )?;
    Ok(())
}

pub fn history(c: &Connection, limit: i64) -> rusqlite::Result<Vec<HistoryRow>> {
    let mut stmt = c.prepare(
        "SELECT h.track_path, COALESCE(t.title, ''), COALESCE(t.artist, ''), h.played_at
         FROM history h LEFT JOIN tracks t ON t.path = h.track_path
         ORDER BY h.played_at DESC LIMIT ?1",
    )?;
    let rows = stmt.query_map(params![limit], |r| {
        Ok(HistoryRow {
            track_path: r.get(0)?,
            title: r.get(1)?,
            artist: r.get(2)?,
            played_at: r.get(3)?,
        })
    })?;
    rows.collect()
}

#[cfg(test)]
mod tests {
    use super::*;

    fn mem() -> Connection {
        let c = Connection::open_in_memory().unwrap();
        c.execute_batch(SCHEMA).unwrap();
        c
    }

    #[test]
    fn add_is_idempotent_and_listed() {
        let c = mem();
        add_track(&c, "/a.mp3", "A", "Artist", 1000).unwrap();
        // Re-adding must not duplicate or clobber.
        add_track(&c, "/a.mp3", "A2", "Artist2", 2000).unwrap();
        let rows = list_tracks(&c).unwrap();
        assert_eq!(rows.len(), 1);
        assert_eq!(rows[0].title, "A"); // INSERT OR IGNORE keeps the first
        assert_eq!(rows[0].gain, 1.0);
    }

    #[test]
    fn cue_loop_grid_gain_round_trip_and_restore() {
        let c = mem();
        add_track(&c, "/t.flac", "T", "Ar", 5000).unwrap();
        set_cue(&c, "/t.flac", 0, 44_100.5).unwrap();
        set_cue(&c, "/t.flac", 3, 88_200.0).unwrap();
        set_cue(&c, "/t.flac", 0, 100.0).unwrap(); // upsert replaces slot 0
        clear_cue(&c, "/t.flac", 3).unwrap();
        set_loop(&c, "/t.flac", 0, 10.0, 20.0, Some(4.0)).unwrap();
        set_grid_offset(&c, "/t.flac", 0.012).unwrap();
        set_gain(&c, "/t.flac", 1.25).unwrap();

        let st = track_state(&c, "/t.flac").unwrap();
        assert_eq!(st.cues.len(), 1);
        assert_eq!(st.cues[0].slot, 0);
        assert_eq!(st.cues[0].frame, 100.0);
        assert_eq!(st.loops.len(), 1);
        assert_eq!(st.loops[0].beats, Some(4.0));
        assert!((st.grid_offset_sec - 0.012).abs() < 1e-9);
        assert_eq!(st.gain, 1.25);
    }

    #[test]
    fn remove_cascades_children() {
        let c = mem();
        add_track(&c, "/x.wav", "X", "Ar", 1).unwrap();
        set_cue(&c, "/x.wav", 1, 5.0).unwrap();
        set_loop(&c, "/x.wav", 0, 1.0, 2.0, None).unwrap();
        remove_track(&c, "/x.wav").unwrap();
        assert!(list_tracks(&c).unwrap().is_empty());
        let st = track_state(&c, "/x.wav").unwrap(); // defaults for an absent track
        assert!(st.cues.is_empty());
        assert!(st.loops.is_empty());
        assert_eq!(st.gain, 1.0);
    }

    #[test]
    fn record_play_bumps_count_and_history() {
        let c = mem();
        add_track(&c, "/p.mp3", "P", "Ar", 1).unwrap();
        record_play(&c, "/p.mp3").unwrap();
        record_play(&c, "/p.mp3").unwrap();
        let rows = list_tracks(&c).unwrap();
        assert_eq!(rows[0].play_count, 2);
        assert!(rows[0].last_played_at.is_some());
        assert_eq!(history(&c, 10).unwrap().len(), 2);
    }

    #[test]
    fn analysis_cache_updates_existing_row() {
        let c = mem();
        add_track(&c, "/m.flac", "M", "Ar", 1).unwrap();
        upsert_analysis(&c, "/m.flac", 128.0, 0.9, 0.1, 0.46875, "8A", "A minor").unwrap();
        let rows = list_tracks(&c).unwrap();
        assert_eq!(rows[0].bpm, Some(128.0));
        assert_eq!(rows[0].key_camelot.as_deref(), Some("8A"));
    }
}
