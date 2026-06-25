//! Portable crate/playlist packages — the data layer.
//!
//! A *manifest* is a self-describing serde snapshot of a crate (or playlist): the resolved track
//! list plus every piece of performance data worth moving between machines — cached analysis
//! (BPM/key/beatgrid), the manual grid nudge, saved gain, hot cues, saved loops, and tags. This
//! module is the pure read/write core: [`gather_crate`] builds a manifest from the library DB and
//! [`apply_manifest`] re-imports one into a (possibly different) library DB. It does no file I/O,
//! zipping, or audio bundling — that packaging layer wraps these and sets each track's `file`.
//!
//! Like [`crate::db`], everything here is plain blocking `rusqlite` behind the library `Mutex`,
//! called only from Tauri commands — never on the audio path.

use std::collections::HashMap;
use std::io::{Read, Seek, Write};

use rusqlite::{params, Connection};
use serde::{Deserialize, Serialize};

/// Manifest schema version. Bump on any incompatible shape change; [`apply_manifest`] checks it.
pub const MANIFEST_VERSION: u32 = 1;

/// The app identity stamped into a manifest (so a future importer can recognize foreign packages).
pub const MANIFEST_APP: &str = "compas-dj";

/// A portable snapshot of one crate/playlist and all its tracks' performance data.
#[derive(Serialize, Deserialize, Debug, Clone, PartialEq)]
pub struct CrateManifest {
    pub version: u32,
    pub app: String,
    /// The source crate's display name (re-used when recreating the crate on import).
    pub name: String,
    pub is_playlist: bool,
    /// The source crate's saved smart-search query, if it was a smart crate. Informational: the
    /// exported `tracks` are the *resolved* snapshot at export time, not a live re-running query.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub smart_query: Option<String>,
    pub tracks: Vec<ManifestTrack>,
}

/// One track plus the performance data carried alongside it.
#[derive(Serialize, Deserialize, Debug, Clone, PartialEq)]
pub struct ManifestTrack {
    /// The track's original absolute path (its identity in the source library).
    pub path: String,
    /// Filename inside the package archive when audio is bundled. Set by the packaging layer; the
    /// importer rewrites `path` to the extracted location. `None` for a manifest-only export.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub file: Option<String>,
    pub title: String,
    pub artist: String,
    pub duration_ms: i64,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub bpm: Option<f64>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub bpm_confidence: Option<f64>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub first_beat_sec: Option<f64>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub beat_interval_sec: Option<f64>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub key_camelot: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub key_name: Option<String>,
    pub grid_offset_sec: f64,
    pub gain: f64,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub tags: Vec<String>,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub cues: Vec<ManifestCue>,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub loops: Vec<ManifestLoop>,
}

#[derive(Serialize, Deserialize, Debug, Clone, PartialEq)]
pub struct ManifestCue {
    pub slot: i64,
    pub frame: f64,
}

#[derive(Serialize, Deserialize, Debug, Clone, PartialEq)]
pub struct ManifestLoop {
    pub slot: i64,
    pub in_frame: f64,
    pub out_frame: f64,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub beats: Option<f64>,
}

/// What [`apply_manifest`] wrote, for a user-facing import summary.
#[derive(Serialize, Default, Debug, Clone, PartialEq)]
pub struct ImportSummary {
    pub tracks: usize,
    pub cues: usize,
    pub loops: usize,
    pub tags: usize,
    /// The id of the crate recreated on import, if requested.
    pub crate_id: Option<i64>,
}

/// Build a manifest from a crate by id. The track list is the crate's *resolved* snapshot — for a
/// smart crate this runs its saved query once and captures the matching tracks (the query itself is
/// recorded in `smart_query` for reference). Each track carries its full cached analysis + state.
pub fn gather_crate(c: &Connection, crate_id: i64) -> rusqlite::Result<CrateManifest> {
    let (name, is_playlist, smart_query) = c.query_row(
        "SELECT name, is_playlist, query FROM crates WHERE id = ?1",
        params![crate_id],
        |r| {
            Ok((
                r.get::<_, String>(0)?,
                r.get::<_, i64>(1)? != 0,
                r.get::<_, Option<String>>(2)?,
            ))
        },
    )?;
    let tracks = crate::db::crate_tracks(c, crate_id)?
        .iter()
        .map(|t| gather_track(c, &t.path))
        .collect::<rusqlite::Result<Vec<_>>>()?;
    Ok(CrateManifest {
        version: MANIFEST_VERSION,
        app: MANIFEST_APP.to_string(),
        name,
        is_playlist,
        smart_query,
        tracks,
    })
}

/// Read one track's full row + tags + cues + loops into a manifest entry.
fn gather_track(c: &Connection, path: &str) -> rusqlite::Result<ManifestTrack> {
    let mut track = c.query_row(
        "SELECT path, title, artist, duration_ms, bpm, bpm_confidence, first_beat_sec,
                beat_interval_sec, key_camelot, key_name, grid_offset_sec, gain
         FROM tracks WHERE path = ?1",
        params![path],
        |r| {
            Ok(ManifestTrack {
                path: r.get(0)?,
                file: None,
                title: r.get(1)?,
                artist: r.get(2)?,
                duration_ms: r.get(3)?,
                bpm: r.get(4)?,
                bpm_confidence: r.get(5)?,
                first_beat_sec: r.get(6)?,
                beat_interval_sec: r.get(7)?,
                key_camelot: r.get(8)?,
                key_name: r.get(9)?,
                grid_offset_sec: r.get(10)?,
                gain: r.get(11)?,
                tags: Vec::new(),
                cues: Vec::new(),
                loops: Vec::new(),
            })
        },
    )?;

    let mut tag_stmt =
        c.prepare("SELECT tag FROM track_tags WHERE track_path = ?1 ORDER BY tag")?;
    track.tags = tag_stmt
        .query_map(params![path], |r| r.get::<_, String>(0))?
        .collect::<rusqlite::Result<Vec<_>>>()?;

    // Reuse the deck-restore reader for cues + loops so both stay in lockstep with the schema.
    let state = crate::db::track_state(c, path)?;
    track.cues = state
        .cues
        .iter()
        .map(|x| ManifestCue {
            slot: x.slot,
            frame: x.frame,
        })
        .collect();
    track.loops = state
        .loops
        .iter()
        .map(|x| ManifestLoop {
            slot: x.slot,
            in_frame: x.in_frame,
            out_frame: x.out_frame,
            beats: x.beats,
        })
        .collect();
    Ok(track)
}

/// Re-import a manifest into a library DB: insert each track (idempotent — existing rows keep their
/// identity), then apply its analysis, grid nudge, gain, cues, loops, and tags. When `recreate_crate`
/// is set, a fresh crate (or playlist) is created from the manifest name with the tracks added in
/// order. Returns counts of what was written. Errors are surfaced unchanged from `rusqlite`.
///
/// Existing performance data is *overwritten* for the imported tracks (the manifest is authoritative
/// for a track it carries), but tracks already in the library that aren't in the manifest are left
/// untouched. Tags are additive (a track keeps any tags it already had).
pub fn apply_manifest(
    c: &Connection,
    manifest: &CrateManifest,
    recreate_crate: bool,
) -> rusqlite::Result<ImportSummary> {
    if manifest.version != MANIFEST_VERSION {
        return Err(rusqlite::Error::InvalidParameterName(format!(
            "unsupported manifest version {} (expected {MANIFEST_VERSION})",
            manifest.version
        )));
    }

    let mut summary = ImportSummary::default();
    for t in &manifest.tracks {
        crate::db::add_track(c, &t.path, &t.title, &t.artist, t.duration_ms)?;

        // Analysis is written as a group (matching how the analyzer caches it); absent fields fall
        // back to neutral defaults so a partially-analyzed export still imports cleanly.
        if t.bpm.is_some() || t.key_camelot.is_some() {
            crate::db::upsert_analysis(
                c,
                &t.path,
                t.bpm.unwrap_or(0.0),
                t.bpm_confidence.unwrap_or(0.0),
                t.first_beat_sec.unwrap_or(0.0),
                t.beat_interval_sec.unwrap_or(0.0),
                t.key_camelot.as_deref().unwrap_or(""),
                t.key_name.as_deref().unwrap_or(""),
            )?;
        }
        crate::db::set_grid_offset(c, &t.path, t.grid_offset_sec)?;
        crate::db::set_gain(c, &t.path, t.gain)?;

        for cue in &t.cues {
            crate::db::set_cue(c, &t.path, cue.slot, cue.frame)?;
            summary.cues += 1;
        }
        for lp in &t.loops {
            crate::db::set_loop(c, &t.path, lp.slot, lp.in_frame, lp.out_frame, lp.beats)?;
            summary.loops += 1;
        }
        for tag in &t.tags {
            crate::db::add_tag(c, &t.path, tag)?;
            summary.tags += 1;
        }
        summary.tracks += 1;
    }

    if recreate_crate {
        // Recreate as a normal crate/playlist holding the resolved snapshot — not as a live smart
        // crate, since the importing library's contents differ from where the query was authored.
        let id = crate::db::create_crate(c, &manifest.name, manifest.is_playlist)?;
        for t in &manifest.tracks {
            crate::db::add_to_crate(c, id, &t.path)?;
        }
        summary.crate_id = Some(id);
    }

    Ok(summary)
}

/// Serialize a manifest to pretty JSON (the on-disk / in-archive form).
pub fn to_json(manifest: &CrateManifest) -> serde_json::Result<String> {
    serde_json::to_string_pretty(manifest)
}

/// Parse a manifest from JSON.
pub fn from_json(json: &str) -> serde_json::Result<CrateManifest> {
    serde_json::from_str(json)
}

// ---------------------------------------------------------------------------------
// Zip packaging — manifest + bundled audio. A package is a `.zip` containing
// `manifest.json` plus one `audio/<file>` entry per track, where `<file>` is the
// track's assigned bundle filename. Read/write are generic over the byte stream so
// the round-trip is testable fully in memory (no filesystem).
// ---------------------------------------------------------------------------------

const MANIFEST_ENTRY: &str = "manifest.json";
const AUDIO_DIR: &str = "audio";

/// Reduce a string to a safe single path component (alphanumerics, `.`, `-`, `_`; everything else
/// becomes `_`). Used for archive entry names and the on-import extraction folder.
pub fn sanitize_component(s: &str) -> String {
    let out: String = s
        .chars()
        .map(|ch| {
            if ch.is_alphanumeric() || matches!(ch, '.' | '-' | '_') {
                ch
            } else {
                '_'
            }
        })
        .collect();
    if out.is_empty() {
        "track".to_string()
    } else {
        out
    }
}

/// Insert `-n` before the extension (or at the end if none) to disambiguate a duplicate filename.
fn dedup_name(name: &str, n: u32) -> String {
    match name.rsplit_once('.') {
        Some((stem, ext)) => format!("{stem}-{n}.{ext}"),
        None => format!("{name}-{n}"),
    }
}

/// Assign each track a unique archive filename (under `audio/`) derived from its source path's
/// basename, deduping collisions. Mutates the manifest's `file` fields in place — call this before
/// [`write_package`] when bundling audio.
pub fn assign_bundle_files(manifest: &mut CrateManifest) {
    let mut counts: HashMap<String, u32> = HashMap::new();
    for t in &mut manifest.tracks {
        let base = std::path::Path::new(&t.path)
            .file_name()
            .map(|s| sanitize_component(&s.to_string_lossy()))
            .unwrap_or_else(|| "track".to_string());
        let file = match counts.get_mut(&base) {
            Some(n) => {
                *n += 1;
                dedup_name(&base, *n)
            }
            None => {
                counts.insert(base.clone(), 0);
                base
            }
        };
        t.file = Some(file);
    }
}

/// Write a `.zip` package: `manifest.json` (with bundle filenames already assigned) plus an
/// `audio/<file>` entry for every track that has a `file`. `open_audio(path)` yields a reader for a
/// track's source audio, streamed into the archive (so files aren't all held in memory at once).
pub fn write_package<W, F>(
    writer: W,
    manifest: &CrateManifest,
    mut open_audio: F,
) -> std::io::Result<()>
where
    W: Write + Seek,
    F: FnMut(&str) -> std::io::Result<Box<dyn Read>>,
{
    let mut zip = zip::ZipWriter::new(writer);
    // Stored (no compression): audio files are already compressed, and Stored keeps the dependency
    // free of a deflate backend. The tiny manifest.json isn't worth a backend either.
    let opts =
        zip::write::SimpleFileOptions::default().compression_method(zip::CompressionMethod::Stored);

    let json = to_json(manifest).map_err(std::io::Error::other)?;
    zip.start_file(MANIFEST_ENTRY, opts)
        .map_err(std::io::Error::other)?;
    zip.write_all(json.as_bytes())?;

    for t in &manifest.tracks {
        let Some(file) = &t.file else { continue };
        let mut reader = open_audio(&t.path)?;
        zip.start_file(format!("{AUDIO_DIR}/{file}"), opts)
            .map_err(std::io::Error::other)?;
        std::io::copy(&mut reader, &mut zip)?;
    }

    zip.finish().map_err(std::io::Error::other)?;
    Ok(())
}

/// Write a single named entry into a fresh `.zip` (Stored). Used for one-file bundles like the
/// diagnostics report — a zip (rather than a bare file) so logs/other files can join it later.
pub fn write_single_entry_zip<W: Write + Seek>(
    writer: W,
    name: &str,
    bytes: &[u8],
) -> std::io::Result<()> {
    let mut zip = zip::ZipWriter::new(writer);
    let opts =
        zip::write::SimpleFileOptions::default().compression_method(zip::CompressionMethod::Stored);
    zip.start_file(name, opts).map_err(std::io::Error::other)?;
    zip.write_all(bytes)?;
    zip.finish().map_err(std::io::Error::other)?;
    Ok(())
}

/// Read a package's manifest plus every `audio/<file>` payload into memory, keyed by the archive
/// filename (matching each track's `file`). Audio is loaded eagerly — fine for a one-shot import.
pub fn read_package<R: Read + Seek>(
    reader: R,
) -> std::io::Result<(CrateManifest, HashMap<String, Vec<u8>>)> {
    let mut archive = zip::ZipArchive::new(reader).map_err(std::io::Error::other)?;
    let mut manifest_json = String::new();
    let mut audio: HashMap<String, Vec<u8>> = HashMap::new();
    let prefix = format!("{AUDIO_DIR}/");

    for i in 0..archive.len() {
        let mut entry = archive.by_index(i).map_err(std::io::Error::other)?;
        let name = entry.name().to_string();
        if name == MANIFEST_ENTRY {
            entry.read_to_string(&mut manifest_json)?;
        } else if let Some(file) = name.strip_prefix(&prefix) {
            if file.is_empty() {
                continue; // the directory entry itself
            }
            let mut buf = Vec::with_capacity(entry.size() as usize);
            entry.read_to_end(&mut buf)?;
            audio.insert(file.to_string(), buf);
        }
    }

    if manifest_json.is_empty() {
        return Err(std::io::Error::new(
            std::io::ErrorKind::InvalidData,
            "package has no manifest.json",
        ));
    }
    let manifest = from_json(&manifest_json).map_err(std::io::Error::other)?;
    Ok((manifest, audio))
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::db::open_in_memory as mem;

    /// Add a fully-populated track (analysis + grid/gain + cues + loops + tags) to a library.
    fn seed_track(c: &Connection, path: &str, title: &str) {
        crate::db::add_track(c, path, title, "Artist", 200_000).unwrap();
        crate::db::upsert_analysis(c, path, 128.0, 0.9, 0.25, 0.46875, "8A", "Am").unwrap();
        crate::db::set_grid_offset(c, path, 0.012).unwrap();
        crate::db::set_gain(c, path, 0.8).unwrap();
        crate::db::set_cue(c, path, 0, 44_100.0).unwrap();
        crate::db::set_cue(c, path, 1, 88_200.0).unwrap();
        crate::db::set_loop(c, path, 0, 100.0, 200.0, Some(4.0)).unwrap();
        crate::db::add_tag(c, path, "banger").unwrap();
        crate::db::add_tag(c, path, "peak-time").unwrap();
    }

    #[test]
    fn crate_round_trips_through_a_manifest_json() {
        let src = mem();
        seed_track(&src, "/music/a.mp3", "Track A");
        seed_track(&src, "/music/b.mp3", "Track B");
        let id = crate::db::create_crate(&src, "My Set", true).unwrap();
        crate::db::add_to_crate(&src, id, "/music/a.mp3").unwrap();
        crate::db::add_to_crate(&src, id, "/music/b.mp3").unwrap();

        let manifest = gather_crate(&src, id).unwrap();
        assert_eq!(manifest.version, MANIFEST_VERSION);
        assert_eq!(manifest.app, MANIFEST_APP);
        assert_eq!(manifest.name, "My Set");
        assert!(manifest.is_playlist);
        assert_eq!(manifest.tracks.len(), 2);

        // Survives a full JSON serialize/parse cycle unchanged.
        let json = to_json(&manifest).unwrap();
        let parsed = from_json(&json).unwrap();
        assert_eq!(parsed, manifest);

        // Re-import into a fresh, empty library reproduces the tracks + crate.
        let dst = mem();
        let summary = apply_manifest(&dst, &parsed, true).unwrap();
        assert_eq!(summary.tracks, 2);
        assert_eq!(summary.cues, 4); // 2 cues x 2 tracks
        assert_eq!(summary.loops, 2);
        assert_eq!(summary.tags, 4);
        assert!(summary.crate_id.is_some());

        // The recreated crate's manifest matches the original (full fidelity).
        let reimported = gather_crate(&dst, summary.crate_id.unwrap()).unwrap();
        assert_eq!(reimported.tracks, manifest.tracks);
    }

    #[test]
    fn import_preserves_per_track_analysis_and_state() {
        let src = mem();
        seed_track(&src, "/music/a.mp3", "Track A");
        let id = crate::db::create_crate(&src, "One", false).unwrap();
        crate::db::add_to_crate(&src, id, "/music/a.mp3").unwrap();
        let manifest = gather_crate(&src, id).unwrap();

        let t = &manifest.tracks[0];
        assert_eq!(t.bpm, Some(128.0));
        assert_eq!(t.key_camelot.as_deref(), Some("8A"));
        assert_eq!(t.key_name.as_deref(), Some("Am"));
        assert_eq!(t.grid_offset_sec, 0.012);
        assert_eq!(t.gain, 0.8);
        assert_eq!(t.cues.len(), 2);
        assert_eq!(t.loops.len(), 1);
        assert_eq!(t.loops[0].beats, Some(4.0));
        assert_eq!(t.tags, vec!["banger".to_string(), "peak-time".to_string()]);

        // Apply without recreating a crate: tracks/state land but no crate is made.
        let dst = mem();
        let summary = apply_manifest(&dst, &manifest, false).unwrap();
        assert_eq!(summary.crate_id, None);
        let state = crate::db::track_state(&dst, "/music/a.mp3").unwrap();
        assert_eq!(state.grid_offset_sec, 0.012);
        assert_eq!(state.gain, 0.8);
        assert_eq!(state.cues.len(), 2);
        assert_eq!(state.loops.len(), 1);
    }

    #[test]
    fn smart_crate_exports_resolved_snapshot_and_query() {
        let src = mem();
        seed_track(&src, "/music/a.mp3", "Track A");
        seed_track(&src, "/music/b.mp3", "Track B");
        // Smart crate matching the shared tag.
        let id = crate::db::create_smart_crate(&src, "Bangers", "tag:banger").unwrap();

        let manifest = gather_crate(&src, id).unwrap();
        assert_eq!(manifest.smart_query.as_deref(), Some("tag:banger"));
        assert_eq!(manifest.tracks.len(), 2); // resolved snapshot, both tagged

        // Re-import recreates it as a *normal* crate (not smart) holding the snapshot.
        let dst = mem();
        let summary = apply_manifest(&dst, &manifest, true).unwrap();
        let recreated = gather_crate(&dst, summary.crate_id.unwrap()).unwrap();
        assert_eq!(recreated.smart_query, None);
        assert_eq!(recreated.tracks.len(), 2);
    }

    #[test]
    fn package_round_trips_manifest_and_audio_in_memory() {
        let src = mem();
        seed_track(&src, "/music/a.mp3", "Track A");
        seed_track(&src, "/music/b.mp3", "Track B");
        let id = crate::db::create_crate(&src, "Set", true).unwrap();
        crate::db::add_to_crate(&src, id, "/music/a.mp3").unwrap();
        crate::db::add_to_crate(&src, id, "/music/b.mp3").unwrap();

        let mut manifest = gather_crate(&src, id).unwrap();
        assign_bundle_files(&mut manifest);
        assert!(manifest.tracks.iter().all(|t| t.file.is_some()));

        // Synthetic audio bytes keyed by source path.
        let bytes: HashMap<&str, Vec<u8>> = HashMap::from([
            ("/music/a.mp3", b"AAAA".to_vec()),
            ("/music/b.mp3", b"BBBBBB".to_vec()),
        ]);

        let mut buf = std::io::Cursor::new(Vec::new());
        write_package(&mut buf, &manifest, |path| {
            Ok(Box::new(std::io::Cursor::new(bytes[path].clone())))
        })
        .unwrap();

        buf.set_position(0);
        let (parsed, audio) = read_package(buf).unwrap();
        assert_eq!(parsed, manifest); // manifest survives the zip unchanged
        for t in &manifest.tracks {
            let file = t.file.as_ref().unwrap();
            assert_eq!(&audio[file], &bytes[t.path.as_str()]);
        }
    }

    #[test]
    fn bundle_filenames_are_unique_for_colliding_basenames() {
        let src = mem();
        // Same basename in two different folders.
        seed_track(&src, "/music/live/set.mp3", "A");
        seed_track(&src, "/music/studio/set.mp3", "B");
        let id = crate::db::create_crate(&src, "Dupes", false).unwrap();
        crate::db::add_to_crate(&src, id, "/music/live/set.mp3").unwrap();
        crate::db::add_to_crate(&src, id, "/music/studio/set.mp3").unwrap();

        let mut manifest = gather_crate(&src, id).unwrap();
        assign_bundle_files(&mut manifest);
        let files: Vec<&String> = manifest
            .tracks
            .iter()
            .filter_map(|t| t.file.as_ref())
            .collect();
        assert_eq!(files.len(), 2);
        assert_ne!(files[0], files[1]); // deduped
        assert!(files.iter().all(|f| f.ends_with(".mp3")));
    }

    #[test]
    fn single_entry_zip_round_trips() {
        let mut buf = std::io::Cursor::new(Vec::new());
        write_single_entry_zip(&mut buf, "diagnostics.json", b"{\"ok\":true}").unwrap();
        buf.set_position(0);
        let mut archive = zip::ZipArchive::new(buf).unwrap();
        assert_eq!(archive.len(), 1);
        let mut entry = archive.by_name("diagnostics.json").unwrap();
        let mut s = String::new();
        entry.read_to_string(&mut s).unwrap();
        assert_eq!(s, "{\"ok\":true}");
    }

    #[test]
    fn read_package_rejects_a_zip_without_a_manifest() {
        let mut buf = std::io::Cursor::new(Vec::new());
        {
            let mut zip = zip::ZipWriter::new(&mut buf);
            zip.start_file("audio/orphan.mp3", zip::write::SimpleFileOptions::default())
                .unwrap();
            zip.write_all(b"x").unwrap();
            zip.finish().unwrap();
        }
        buf.set_position(0);
        assert!(read_package(buf).is_err());
    }

    #[test]
    fn rejects_an_unknown_manifest_version() {
        let dst = mem();
        let manifest = CrateManifest {
            version: MANIFEST_VERSION + 1,
            app: MANIFEST_APP.to_string(),
            name: "Future".to_string(),
            is_playlist: false,
            smart_query: None,
            tracks: Vec::new(),
        };
        assert!(apply_manifest(&dst, &manifest, false).is_err());
    }
}
