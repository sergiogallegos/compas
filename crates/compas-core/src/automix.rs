//! Auto-mix / set-construction planning: score how well one track mixes into another and rank a
//! pool of candidates for the next track. Pure domain logic (no I/O), so it's fully unit-tested and
//! reusable by the AutoDJ queue and the "suggest next" UI.
//!
//! Two signals drive the score: **harmonic** compatibility on the Camelot wheel (same key, relative
//! major/minor, ±1 neighbor, energy ±2…) and **tempo** compatibility (within pitch range, or a
//! half/double-time relationship). Missing analysis contributes a neutral score rather than
//! disqualifying a track.

use serde::{Deserialize, Serialize};

/// The analysis a track contributes to planning. Both fields are optional — unanalyzed tracks still
/// rank, just on whatever signal is present.
#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct TrackInfo {
    pub bpm: Option<f32>,
    /// Camelot key code, e.g. `"8A"` / `"8B"`.
    pub camelot: Option<String>,
}

/// Weight of the harmonic vs tempo component in the blended transition score.
const KEY_WEIGHT: f32 = 0.6;
const BPM_WEIGHT: f32 = 0.4;

/// Parse a Camelot code like `"8A"` into `(wheel_number 1..=12, 'A' | 'B')`.
fn parse_camelot(s: &str) -> Option<(i32, char)> {
    let s = s.trim().to_uppercase();
    let letter = s.chars().next_back()?;
    if letter != 'A' && letter != 'B' {
        return None;
    }
    let num: i32 = s[..s.len() - letter.len_utf8()].parse().ok()?;
    (1..=12).contains(&num).then_some((num, letter))
}

/// Shortest distance between two positions on the 12-spoke Camelot wheel.
fn wheel_distance(a: i32, b: i32) -> i32 {
    let d = (a - b).rem_euclid(12);
    d.min(12 - d)
}

/// Harmonic compatibility `0..=1` between two Camelot keys (1.0 = perfect). Unknown keys → 0.5.
pub fn camelot_compat(a: &str, b: &str) -> f32 {
    let (Some((na, la)), Some((nb, lb))) = (parse_camelot(a), parse_camelot(b)) else {
        return 0.5;
    };
    if na == nb && la == lb {
        1.0 // same key
    } else if na == nb {
        0.9 // relative major/minor
    } else if la == lb {
        match wheel_distance(na, nb) {
            1 => 0.85, // adjacent on the wheel (±1)
            2 => 0.5,  // two-step energy mix
            _ => 0.15,
        }
    } else {
        // Different letter and number: only the diagonal ±1 is musically useful.
        match wheel_distance(na, nb) {
            1 => 0.4,
            _ => 0.1,
        }
    }
}

/// Tempo compatibility `0..=1` between two BPMs, rewarding pitch-range matches and half/double-time
/// relationships. Unknown/zero tempo → 0.5.
pub fn bpm_compat(a: f32, b: f32) -> f32 {
    if a <= 0.0 || b <= 0.0 {
        return 0.5;
    }
    let diff = (b / a - 1.0).abs();
    if diff <= 0.02 {
        1.0
    } else if diff <= 0.06 {
        0.8 // inside a typical ±6% pitch fader
    } else if diff <= 0.12 {
        0.4
    } else {
        // Half/double-time can still beatmatch.
        let d2 = (b / (a * 2.0) - 1.0).abs().min((b * 2.0 / a - 1.0).abs());
        if d2 <= 0.06 {
            0.5
        } else {
            0.1
        }
    }
}

/// Blended transition score `0..=1` for mixing `from` → `to`.
pub fn score_transition(from: &TrackInfo, to: &TrackInfo) -> f32 {
    let key = match (from.camelot.as_deref(), to.camelot.as_deref()) {
        (Some(a), Some(b)) => camelot_compat(a, b),
        _ => 0.5,
    };
    let bpm = match (from.bpm, to.bpm) {
        (Some(a), Some(b)) => bpm_compat(a, b),
        _ => 0.5,
    };
    KEY_WEIGHT * key + BPM_WEIGHT * bpm
}

/// Rank `pool` as candidate next tracks after `current`, best first. Returns `(pool_index, score)`;
/// the caller maps indices back to its own track list. Stable for equal scores (preserves input
/// order), so ties fall back to the pool's existing ordering.
pub fn plan_next(current: &TrackInfo, pool: &[TrackInfo]) -> Vec<(usize, f32)> {
    let mut ranked: Vec<(usize, f32)> = pool
        .iter()
        .enumerate()
        .map(|(i, t)| (i, score_transition(current, t)))
        .collect();
    // Sort by score desc; stable sort keeps input order for ties.
    ranked.sort_by(|a, b| b.1.partial_cmp(&a.1).unwrap_or(std::cmp::Ordering::Equal));
    ranked
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn camelot_rules() {
        assert!((camelot_compat("8A", "8A") - 1.0).abs() < 1e-6); // same
        assert!((camelot_compat("8A", "8B") - 0.9).abs() < 1e-6); // relative
        assert!((camelot_compat("8A", "9A") - 0.85).abs() < 1e-6); // +1 neighbor
        assert!((camelot_compat("8A", "7A") - 0.85).abs() < 1e-6); // -1 neighbor
        assert!((camelot_compat("12A", "1A") - 0.85).abs() < 1e-6); // wraps around the wheel
        assert!(camelot_compat("8A", "2A") < 0.2); // far apart
        assert!((camelot_compat("8A", "garbage") - 0.5).abs() < 1e-6); // unknown → neutral
    }

    #[test]
    fn bpm_rules() {
        assert!((bpm_compat(128.0, 128.0) - 1.0).abs() < 1e-6);
        assert!(bpm_compat(128.0, 131.0) >= 0.8); // ~2.3% in pitch range
        assert!((bpm_compat(128.0, 140.0) - 0.4).abs() < 1e-6); // ~9% off
        assert!((bpm_compat(128.0, 64.0) - 0.5).abs() < 1e-6); // half-time
        assert!((bpm_compat(0.0, 128.0) - 0.5).abs() < 1e-6); // unknown
        assert!(bpm_compat(128.0, 175.0) < 0.2); // incompatible
    }

    #[test]
    fn plan_ranks_best_transition_first() {
        let current = TrackInfo {
            bpm: Some(128.0),
            camelot: Some("8A".into()),
        };
        let pool = vec![
            TrackInfo {
                bpm: Some(175.0),
                camelot: Some("2B".into()),
            }, // poor: far key + tempo
            TrackInfo {
                bpm: Some(128.0),
                camelot: Some("8A".into()),
            }, // perfect
            TrackInfo {
                bpm: Some(129.0),
                camelot: Some("9A".into()),
            }, // great: neighbor key, close tempo
        ];
        let ranked = plan_next(&current, &pool);
        assert_eq!(ranked.len(), 3);
        assert_eq!(ranked[0].0, 1, "perfect match ranks first");
        assert_eq!(ranked[1].0, 2, "neighbor key ranks second");
        assert_eq!(ranked[2].0, 0, "incompatible ranks last");
        assert!(ranked[0].1 > ranked[2].1);
    }

    #[test]
    fn missing_analysis_is_neutral_not_disqualifying() {
        let current = TrackInfo::default();
        let pool = vec![TrackInfo::default()];
        let ranked = plan_next(&current, &pool);
        assert!((ranked[0].1 - 0.5).abs() < 1e-6);
    }
}
