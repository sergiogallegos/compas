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

CREATE TABLE IF NOT EXISTS crates (
    id          INTEGER PRIMARY KEY AUTOINCREMENT,
    name        TEXT NOT NULL,
    -- 0 = crate (unordered set), 1 = playlist (ordered by position)
    is_playlist INTEGER NOT NULL DEFAULT 0,
    created_at  INTEGER NOT NULL,
    -- Non-null for a smart crate: a saved search-grammar query that populates it dynamically.
    query       TEXT
);

CREATE TABLE IF NOT EXISTS crate_tracks (
    crate_id   INTEGER NOT NULL REFERENCES crates(id) ON DELETE CASCADE,
    track_path TEXT NOT NULL REFERENCES tracks(path) ON DELETE CASCADE,
    position   INTEGER NOT NULL DEFAULT 0,
    PRIMARY KEY (crate_id, track_path)
);

CREATE TABLE IF NOT EXISTS track_tags (
    track_path TEXT NOT NULL REFERENCES tracks(path) ON DELETE CASCADE,
    tag        TEXT NOT NULL,
    PRIMARY KEY (track_path, tag)
);

CREATE TABLE IF NOT EXISTS watch_folders (
    path     TEXT PRIMARY KEY,
    added_at INTEGER NOT NULL
);
";

/// SQL fragment selecting a comma-joined `tags` column for a track. `path_expr` is how the track's
/// path is referenced in the enclosing query (`path` or `t.path`).
fn tags_select(path_expr: &str) -> String {
    format!("(SELECT GROUP_CONCAT(tag, ',') FROM track_tags tt WHERE tt.track_path = {path_expr}) AS tags")
}

/// Open (creating if needed) the database at `path` and apply the schema.
pub fn open(path: impl AsRef<Path>) -> rusqlite::Result<Db> {
    let conn = Connection::open(path)?;
    conn.execute_batch(SCHEMA)?;
    // Idempotent migration for DBs created before smart crates: add the column if missing.
    // (Errors — i.e. "duplicate column" — are expected and ignored.)
    let _ = conn.execute("ALTER TABLE crates ADD COLUMN query TEXT", []);
    Ok(Db(Mutex::new(conn)))
}

/// A fresh in-memory connection with the schema applied — shared by unit tests across modules
/// (e.g. the `export` round-trip tests) so the schema lives in exactly one place.
#[cfg(test)]
pub(crate) fn open_in_memory() -> Connection {
    let conn = Connection::open_in_memory().expect("open in-memory db");
    conn.execute_batch(SCHEMA).expect("apply schema");
    conn
}

#[derive(Serialize, Clone)]
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
    /// User tags (lowercased), for `tag:` search and smart crates.
    pub tags: Vec<String>,
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
        tags: row
            .get::<_, Option<String>>("tags")?
            .map(|s| {
                s.split(',')
                    .filter(|x| !x.is_empty())
                    .map(str::to_string)
                    .collect()
            })
            .unwrap_or_default(),
    })
}

pub fn list_tracks(c: &Connection) -> rusqlite::Result<Vec<TrackRow>> {
    let mut stmt = c.prepare(&format!(
        "SELECT path, title, artist, duration_ms, bpm, key_camelot, key_name,
                grid_offset_sec, gain, play_count, last_played_at, {}
         FROM tracks ORDER BY added_at DESC, title ASC",
        tags_select("path"),
    ))?;
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
        &format!(
            "SELECT path, title, artist, duration_ms, bpm, key_camelot, key_name,
                    grid_offset_sec, gain, play_count, last_played_at, {}
             FROM tracks WHERE path = ?1",
            tags_select("path"),
        ),
        params![path],
        row_to_track,
    )
}

/// Normalize a tag: trimmed, lowercased, commas (the GROUP_CONCAT separator) removed.
fn clean_tag(tag: &str) -> String {
    tag.trim().to_ascii_lowercase().replace(',', "")
}

/// Tag a track (idempotent; no-op for a blank tag).
pub fn add_tag(c: &Connection, path: &str, tag: &str) -> rusqlite::Result<()> {
    let t = clean_tag(tag);
    if t.is_empty() {
        return Ok(());
    }
    c.execute(
        "INSERT OR IGNORE INTO track_tags (track_path, tag) VALUES (?1, ?2)",
        params![path, t],
    )?;
    Ok(())
}

/// Remove a tag from a track.
pub fn remove_tag(c: &Connection, path: &str, tag: &str) -> rusqlite::Result<()> {
    c.execute(
        "DELETE FROM track_tags WHERE track_path = ?1 AND tag = ?2",
        params![path, clean_tag(tag)],
    )?;
    Ok(())
}

/// Whether a track path is already in the library (so a folder scan can skip re-probing it).
pub fn track_exists(c: &Connection, path: &str) -> rusqlite::Result<bool> {
    c.query_row(
        "SELECT 1 FROM tracks WHERE path = ?1",
        params![path],
        |_| Ok(()),
    )
    .optional()
    .map(|o| o.is_some())
}

/// Register a folder to auto-import from (idempotent).
pub fn add_watch_folder(c: &Connection, path: &str) -> rusqlite::Result<()> {
    c.execute(
        "INSERT OR IGNORE INTO watch_folders (path, added_at) VALUES (?1, ?2)",
        params![path, now()],
    )?;
    Ok(())
}

pub fn remove_watch_folder(c: &Connection, path: &str) -> rusqlite::Result<()> {
    c.execute("DELETE FROM watch_folders WHERE path = ?1", params![path])?;
    Ok(())
}

pub fn list_watch_folders(c: &Connection) -> rusqlite::Result<Vec<String>> {
    let mut stmt = c.prepare("SELECT path FROM watch_folders ORDER BY path ASC")?;
    let rows = stmt.query_map([], |r| r.get::<_, String>(0))?;
    rows.collect()
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

/// Paths of library tracks that have never been analyzed (no BPM cached yet), newest first. The
/// background analyzer drains this: on success it writes a real BPM, on decode failure a 0 sentinel,
/// so a row goes NULL → non-NULL exactly once and is never reprocessed in a loop.
pub fn list_unanalyzed(c: &Connection) -> rusqlite::Result<Vec<String>> {
    let mut stmt = c.prepare("SELECT path FROM tracks WHERE bpm IS NULL ORDER BY added_at DESC")?;
    let rows = stmt.query_map([], |r| r.get::<_, String>(0))?;
    rows.collect()
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

    let mut loop_stmt = c.prepare(
        "SELECT slot, in_frame, out_frame, beats FROM loops WHERE track_path = ?1 ORDER BY slot",
    )?;
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

// ---------------------------------------------------------------------------------
// Search query language
// ---------------------------------------------------------------------------------

/// Compile a search string into a SQL `WHERE` body + bound parameters. Grammar (space-separated):
/// `bpm:120-128` (range) · `bpm:128` (±1) · `key:8A` · `artist:foo` / `title:foo` (fuzzy LIKE) ·
/// `tag:foo` (exact tag) · a bare word matches title OR artist · a `-` prefix negates any term.
/// Unknown `field:` tokens are
/// ignored. Terms are AND-ed within a group; a literal `OR` (or `|`) token starts a new group and
/// groups are OR-ed — e.g. `artist:daft OR artist:justice`, or `bpm:120-125 key:8A OR bpm:170-175`.
/// Returns `("1=1", [])` for an empty query.
pub fn build_search(query: &str) -> (String, Vec<rusqlite::types::Value>) {
    use rusqlite::types::Value;
    // Each group is a list of AND-ed conditions; groups are OR-ed together.
    let mut groups: Vec<Vec<String>> = vec![Vec::new()];
    let mut params: Vec<Value> = Vec::new();

    for raw in query.split_whitespace() {
        // `OR` / `|` (uppercase, so a lowercase "or" stays a literal search word) opens a new group.
        if raw == "OR" || raw == "|" {
            groups.push(Vec::new());
            continue;
        }
        let (neg, tok) = match raw.strip_prefix('-') {
            Some(rest) if !rest.is_empty() => (true, rest),
            _ => (false, raw),
        };
        let cond: Option<String> = if let Some((field, val)) = tok.split_once(':') {
            if val.is_empty() {
                None
            } else {
                match field.to_ascii_lowercase().as_str() {
                    "bpm" => {
                        if let Some((a, b)) = val.split_once('-') {
                            match (a.parse::<f64>(), b.parse::<f64>()) {
                                (Ok(a), Ok(b)) => {
                                    params.push(Value::Real(a));
                                    params.push(Value::Real(b));
                                    Some("bpm BETWEEN ? AND ?".to_string())
                                }
                                _ => None,
                            }
                        } else if let Ok(v) = val.parse::<f64>() {
                            params.push(Value::Real(v - 1.0));
                            params.push(Value::Real(v + 1.0));
                            Some("bpm BETWEEN ? AND ?".to_string())
                        } else {
                            None
                        }
                    }
                    "key" => {
                        params.push(Value::Text(val.to_uppercase()));
                        Some("key_camelot = ?".to_string())
                    }
                    "artist" => {
                        params.push(Value::Text(format!("%{val}%")));
                        Some("artist LIKE ?".to_string())
                    }
                    "title" => {
                        params.push(Value::Text(format!("%{val}%")));
                        Some("title LIKE ?".to_string())
                    }
                    "tag" => {
                        params.push(Value::Text(val.to_ascii_lowercase()));
                        Some(
                            "EXISTS (SELECT 1 FROM track_tags tt \
                             WHERE tt.track_path = path AND tt.tag = ?)"
                                .to_string(),
                        )
                    }
                    _ => None,
                }
            }
        } else {
            let like = format!("%{tok}%");
            params.push(Value::Text(like.clone()));
            params.push(Value::Text(like));
            Some("(title LIKE ? OR artist LIKE ?)".to_string())
        };

        if let Some(c) = cond {
            let group = groups.last_mut().expect("at least one group");
            group.push(if neg { format!("NOT ({c})") } else { c });
        }
    }

    // Render: AND within each (non-empty) group, OR across groups. A multi-condition group is
    // parenthesised so the OR precedence is correct.
    let rendered: Vec<String> = groups
        .iter()
        .filter(|g| !g.is_empty())
        .map(|g| {
            if g.len() == 1 {
                g[0].clone()
            } else {
                format!("({})", g.join(" AND "))
            }
        })
        .collect();

    match rendered.len() {
        0 => ("1=1".to_string(), params),
        1 => (rendered.into_iter().next().unwrap(), params),
        _ => (rendered.join(" OR "), params),
    }
}

/// Search the library with the [`build_search`] grammar.
pub fn search_tracks(c: &Connection, query: &str) -> rusqlite::Result<Vec<TrackRow>> {
    let (where_body, params) = build_search(query);
    let sql = format!(
        "SELECT path, title, artist, duration_ms, bpm, key_camelot, key_name,
                grid_offset_sec, gain, play_count, last_played_at, {tags}
         FROM tracks WHERE {where_body} ORDER BY added_at DESC, title ASC",
        tags = tags_select("path"),
    );
    let mut stmt = c.prepare(&sql)?;
    let rows = stmt.query_map(rusqlite::params_from_iter(params), row_to_track)?;
    rows.collect()
}

// ---------------------------------------------------------------------------------
// Crates & playlists
// ---------------------------------------------------------------------------------

#[derive(Serialize)]
pub struct CrateRow {
    pub id: i64,
    pub name: String,
    pub is_playlist: bool,
    pub track_count: i64,
    /// True for a smart crate (populated by a saved search rather than manual membership).
    pub is_smart: bool,
}

/// Create a crate (`is_playlist = false`) or ordered playlist and return its id.
pub fn create_crate(c: &Connection, name: &str, is_playlist: bool) -> rusqlite::Result<i64> {
    c.execute(
        "INSERT INTO crates (name, is_playlist, created_at) VALUES (?1, ?2, ?3)",
        params![name, is_playlist as i64, now()],
    )?;
    Ok(c.last_insert_rowid())
}

/// Create a smart crate: a saved search-grammar `query` that populates the crate dynamically
/// (see [`build_search`]). Smart crates ignore manual membership.
pub fn create_smart_crate(c: &Connection, name: &str, query: &str) -> rusqlite::Result<i64> {
    c.execute(
        "INSERT INTO crates (name, is_playlist, created_at, query) VALUES (?1, 0, ?2, ?3)",
        params![name, now(), query],
    )?;
    Ok(c.last_insert_rowid())
}

pub fn delete_crate(c: &Connection, id: i64) -> rusqlite::Result<()> {
    c.execute("DELETE FROM crates WHERE id = ?1", params![id])?;
    Ok(())
}

pub fn list_crates(c: &Connection) -> rusqlite::Result<Vec<CrateRow>> {
    let mut stmt = c.prepare(
        "SELECT cr.id, cr.name, cr.is_playlist,
                (SELECT COUNT(*) FROM crate_tracks ct WHERE ct.crate_id = cr.id),
                cr.query
         FROM crates cr ORDER BY cr.name ASC",
    )?;
    let rows = stmt.query_map([], |r| {
        let query: Option<String> = r.get(4)?;
        Ok(CrateRow {
            id: r.get(0)?,
            name: r.get(1)?,
            is_playlist: r.get::<_, i64>(2)? != 0,
            track_count: r.get(3)?,
            is_smart: query.is_some(),
        })
    })?;
    rows.collect()
}

/// Add a track to a crate at the end (idempotent). Position = current max + 1.
pub fn add_to_crate(c: &Connection, crate_id: i64, path: &str) -> rusqlite::Result<()> {
    let next: i64 = c.query_row(
        "SELECT COALESCE(MAX(position), -1) + 1 FROM crate_tracks WHERE crate_id = ?1",
        params![crate_id],
        |r| r.get(0),
    )?;
    c.execute(
        "INSERT OR IGNORE INTO crate_tracks (crate_id, track_path, position) VALUES (?1, ?2, ?3)",
        params![crate_id, path, next],
    )?;
    Ok(())
}

pub fn remove_from_crate(c: &Connection, crate_id: i64, path: &str) -> rusqlite::Result<()> {
    c.execute(
        "DELETE FROM crate_tracks WHERE crate_id = ?1 AND track_path = ?2",
        params![crate_id, path],
    )?;
    Ok(())
}

/// List a crate's tracks. A smart crate (non-null `query`) runs its saved search instead of
/// returning stored membership; manual crates/playlists return their members (playlists ordered
/// by position, crates by title).
pub fn crate_tracks(c: &Connection, crate_id: i64) -> rusqlite::Result<Vec<TrackRow>> {
    // Smart crate? Run its saved query.
    let query: Option<String> = c
        .query_row(
            "SELECT query FROM crates WHERE id = ?1",
            params![crate_id],
            |r| r.get(0),
        )
        .optional()?
        .flatten();
    if let Some(q) = query {
        return search_tracks(c, &q);
    }

    let mut stmt = c.prepare(&format!(
        "SELECT t.path, t.title, t.artist, t.duration_ms, t.bpm, t.key_camelot, t.key_name,
                t.grid_offset_sec, t.gain, t.play_count, t.last_played_at, {}
         FROM crate_tracks ct
         JOIN tracks t ON t.path = ct.track_path
         JOIN crates cr ON cr.id = ct.crate_id
         WHERE ct.crate_id = ?1
         ORDER BY CASE WHEN cr.is_playlist = 1 THEN ct.position END ASC,
                  CASE WHEN cr.is_playlist = 0 THEN t.title END ASC",
        tags_select("t.path"),
    ))?;
    let rows = stmt.query_map(params![crate_id], row_to_track)?;
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

    #[test]
    fn search_filters_fielded_range_key_and_negation() {
        let c = mem();
        add_track(&c, "/a.mp3", "Da Funk", "Daft Punk", 1).unwrap();
        upsert_analysis(&c, "/a.mp3", 123.0, 0.9, 0.0, 0.5, "8A", "A minor").unwrap();
        add_track(&c, "/b.mp3", "Live Set", "Other", 1).unwrap();
        upsert_analysis(&c, "/b.mp3", 128.0, 0.9, 0.0, 0.5, "9A", "E minor").unwrap();

        let r = search_tracks(&c, "artist:daft bpm:120-125").unwrap();
        assert_eq!(r.len(), 1);
        assert_eq!(r[0].path, "/a.mp3");

        let r = search_tracks(&c, "key:9a").unwrap();
        assert_eq!(r.len(), 1);
        assert_eq!(r[0].path, "/b.mp3");

        let r = search_tracks(&c, "-live").unwrap();
        assert_eq!(r.len(), 1);
        assert_eq!(r[0].path, "/a.mp3");

        assert_eq!(search_tracks(&c, "funk").unwrap().len(), 1);
        assert_eq!(search_tracks(&c, "  ").unwrap().len(), 2); // empty → all
    }

    #[test]
    fn search_supports_or_groups() {
        let c = mem();
        add_track(&c, "/a.mp3", "Da Funk", "Daft Punk", 1).unwrap();
        upsert_analysis(&c, "/a.mp3", 123.0, 0.9, 0.0, 0.5, "8A", "A minor").unwrap();
        add_track(&c, "/b.mp3", "Genesis", "Justice", 1).unwrap();
        upsert_analysis(&c, "/b.mp3", 128.0, 0.9, 0.0, 0.5, "9A", "E minor").unwrap();
        add_track(&c, "/c.mp3", "Other", "Someone", 1).unwrap();
        upsert_analysis(&c, "/c.mp3", 100.0, 0.9, 0.0, 0.5, "1A", "Ab minor").unwrap();

        // OR across two artist groups returns both, not the third.
        let mut r = search_tracks(&c, "artist:daft OR artist:justice").unwrap();
        r.sort_by(|x, y| x.path.cmp(&y.path));
        assert_eq!(r.len(), 2);
        assert_eq!(r[0].path, "/a.mp3");
        assert_eq!(r[1].path, "/b.mp3");

        // `|` is an alias for OR.
        assert_eq!(search_tracks(&c, "key:8A | key:9A").unwrap().len(), 2);

        // AND binds tighter than OR: (daft AND 120-125) OR 170-175 → only /a.
        let r = search_tracks(&c, "artist:daft bpm:120-125 OR bpm:170-175").unwrap();
        assert_eq!(r.len(), 1);
        assert_eq!(r[0].path, "/a.mp3");

        // A trailing/leading OR with an empty group is ignored.
        assert_eq!(search_tracks(&c, "artist:justice OR").unwrap().len(), 1);
    }

    #[test]
    fn crates_and_playlists_crud_and_ordering() {
        let c = mem();
        add_track(&c, "/1.mp3", "One", "Ar", 1).unwrap();
        add_track(&c, "/2.mp3", "Two", "Ar", 1).unwrap();
        let id = create_crate(&c, "Set A", true).unwrap();
        add_to_crate(&c, id, "/2.mp3").unwrap();
        add_to_crate(&c, id, "/1.mp3").unwrap();
        add_to_crate(&c, id, "/2.mp3").unwrap(); // idempotent

        let tracks = crate_tracks(&c, id).unwrap();
        assert_eq!(tracks.len(), 2);
        // Playlist preserves insertion order.
        assert_eq!(tracks[0].path, "/2.mp3");
        assert_eq!(tracks[1].path, "/1.mp3");

        let crates = list_crates(&c).unwrap();
        assert_eq!(crates.len(), 1);
        assert_eq!(crates[0].track_count, 2);
        assert!(crates[0].is_playlist);

        remove_from_crate(&c, id, "/2.mp3").unwrap();
        assert_eq!(crate_tracks(&c, id).unwrap().len(), 1);
        // Deleting a track cascades out of the crate.
        remove_track(&c, "/1.mp3").unwrap();
        assert!(crate_tracks(&c, id).unwrap().is_empty());

        delete_crate(&c, id).unwrap();
        assert!(list_crates(&c).unwrap().is_empty());
    }

    #[test]
    fn smart_crate_runs_its_saved_query() {
        let c = mem();
        add_track(&c, "/a.mp3", "Da Funk", "Daft Punk", 1).unwrap();
        upsert_analysis(&c, "/a.mp3", 123.0, 0.9, 0.0, 0.5, "8A", "A minor").unwrap();
        add_track(&c, "/b.mp3", "Genesis", "Justice", 1).unwrap();
        upsert_analysis(&c, "/b.mp3", 128.0, 0.9, 0.0, 0.5, "9A", "E minor").unwrap();

        let id = create_smart_crate(&c, "Daft only", "artist:daft").unwrap();
        // list_crates flags it smart.
        let cr = list_crates(&c).unwrap();
        assert_eq!(cr.len(), 1);
        assert!(cr[0].is_smart);
        assert!(!cr[0].is_playlist);

        // crate_tracks runs the saved query, not stored membership.
        let tracks = crate_tracks(&c, id).unwrap();
        assert_eq!(tracks.len(), 1);
        assert_eq!(tracks[0].path, "/a.mp3");

        // Adding a second matching track updates the smart crate automatically.
        add_track(&c, "/c.mp3", "Aerodynamic", "Daft Punk", 1).unwrap();
        assert_eq!(crate_tracks(&c, id).unwrap().len(), 2);

        // A plain crate still returns manual membership (no query).
        let plain = create_crate(&c, "Manual", false).unwrap();
        add_to_crate(&c, plain, "/b.mp3").unwrap();
        assert!(
            !list_crates(&c)
                .unwrap()
                .iter()
                .find(|x| x.id == plain)
                .unwrap()
                .is_smart
        );
        assert_eq!(crate_tracks(&c, plain).unwrap().len(), 1);
    }

    #[test]
    fn tags_attach_search_and_appear_on_rows() {
        let c = mem();
        add_track(&c, "/a.mp3", "A", "Ar", 1).unwrap();
        add_track(&c, "/b.mp3", "B", "Ar", 1).unwrap();
        add_tag(&c, "/a.mp3", "Warmup").unwrap(); // normalized to lowercase
        add_tag(&c, "/a.mp3", "warmup").unwrap(); // idempotent
        add_tag(&c, "/a.mp3", "deep house").unwrap();
        add_tag(&c, "/b.mp3", "peak").unwrap();

        // Tags ride along on track rows.
        let a = list_tracks(&c).unwrap();
        let a_row = a.iter().find(|t| t.path == "/a.mp3").unwrap();
        assert!(a_row.tags.contains(&"warmup".to_string()));
        assert!(a_row.tags.contains(&"deep house".to_string()));
        assert_eq!(a_row.tags.len(), 2);

        // tag: search (exact), and negation.
        assert_eq!(search_tracks(&c, "tag:warmup").unwrap().len(), 1);
        assert_eq!(search_tracks(&c, "tag:warmup").unwrap()[0].path, "/a.mp3");
        assert_eq!(search_tracks(&c, "-tag:warmup").unwrap().len(), 1); // → /b
        assert_eq!(search_tracks(&c, "-tag:warmup").unwrap()[0].path, "/b.mp3");
        // Combines with OR and other fields → smart-crate-able.
        assert_eq!(
            search_tracks(&c, "tag:warmup OR tag:peak").unwrap().len(),
            2
        );

        // Remove a tag; it drops from rows and search.
        remove_tag(&c, "/a.mp3", "warmup").unwrap();
        assert!(search_tracks(&c, "tag:warmup").unwrap().is_empty());

        // Deleting a track cascades its tags.
        remove_track(&c, "/b.mp3").unwrap();
        assert!(search_tracks(&c, "tag:peak").unwrap().is_empty());
    }

    #[test]
    fn watch_folders_crud_and_track_exists() {
        let c = mem();
        assert!(list_watch_folders(&c).unwrap().is_empty());
        add_watch_folder(&c, "/music/house").unwrap();
        add_watch_folder(&c, "/music/house").unwrap(); // idempotent
        add_watch_folder(&c, "/music/techno").unwrap();
        let f = list_watch_folders(&c).unwrap();
        assert_eq!(
            f,
            vec!["/music/house".to_string(), "/music/techno".to_string()]
        );

        add_track(&c, "/music/house/a.mp3", "A", "Ar", 1).unwrap();
        assert!(track_exists(&c, "/music/house/a.mp3").unwrap());
        assert!(!track_exists(&c, "/music/house/missing.mp3").unwrap());

        remove_watch_folder(&c, "/music/house").unwrap();
        assert_eq!(
            list_watch_folders(&c).unwrap(),
            vec!["/music/techno".to_string()]
        );
    }

    #[test]
    fn list_unanalyzed_returns_only_tracks_without_bpm() {
        let c = mem();
        add_track(&c, "/a.mp3", "A", "Ar", 1).unwrap();
        add_track(&c, "/b.mp3", "B", "Br", 1).unwrap();
        // Both start un-analyzed (BPM NULL).
        assert_eq!(list_unanalyzed(&c).unwrap().len(), 2);

        // Caching analysis onto /a.mp3 drops it from the work list.
        upsert_analysis(&c, "/a.mp3", 128.0, 0.9, 0.01, 0.468, "8A", "A minor").unwrap();
        assert_eq!(list_unanalyzed(&c).unwrap(), vec!["/b.mp3".to_string()]);

        // A 0-BPM sentinel (decode failure / no beat) still counts as attempted → not retried.
        upsert_analysis(&c, "/b.mp3", 0.0, 0.0, 0.0, 0.0, "", "").unwrap();
        assert!(list_unanalyzed(&c).unwrap().is_empty());
    }
}
