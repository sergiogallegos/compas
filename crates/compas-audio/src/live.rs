//! Live beat-tracking glue (adoption-plan slice 5.2). See `docs/research/live-input-beat-tracking.md`.
//!
//! The aux/mic capture ([`crate::input`]) fans its frames into a second "analysis" ring. A
//! dedicated, non-realtime thread drains that ring, downmixes to mono, feeds the causal
//! [`compas_dsp::LiveTracker`], and publishes the result into a lock-free [`LiveBeatClock`] the UI
//! polls. Nothing here runs on the audio callback.

use std::sync::atomic::{AtomicBool, AtomicU32, AtomicU64, Ordering};
use std::sync::Arc;
use std::time::{Duration, Instant};

use compas_dsp::LiveTracker;
use rtrb::Consumer;

/// Lock-free snapshot of the live beat tracker, shared between the analysis thread (writer) and the
/// UI/IPC (reader). Same atomics pattern as `MonitorLatency` / `DeckTelemetry`.
///
/// `beat_phase` is published with a timestamp (`stamp_nanos` since `epoch`), so a reader on another
/// clock domain — the audio thread's sync PLL — can extrapolate the phase to *now* instead of
/// locking to a stale snapshot. [`Self::snapshot`] does that extrapolation.
pub struct LiveBeatClock {
    /// Shared monotonic epoch; both the writer (stamping) and readers (extrapolating) measure
    /// elapsed time against it, so they agree on "how old is this phase" across threads.
    epoch: Instant,
    active: AtomicBool,
    locked: AtomicBool,
    bpm_bits: AtomicU32,
    phase_bits: AtomicU32,
    conf_bits: AtomicU32,
    /// Nanoseconds since `epoch` when `beat_phase` was last published.
    stamp_nanos: AtomicU64,
}

impl Default for LiveBeatClock {
    fn default() -> Self {
        Self {
            epoch: Instant::now(),
            active: AtomicBool::new(false),
            locked: AtomicBool::new(false),
            bpm_bits: AtomicU32::new(0),
            phase_bits: AtomicU32::new(0),
            conf_bits: AtomicU32::new(0),
            stamp_nanos: AtomicU64::new(0),
        }
    }
}

/// Plain-data view of [`LiveBeatClock`] for IPC.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct LiveBeatSnapshot {
    /// Whether the live analysis thread is running (aux capture is being tracked).
    pub active: bool,
    pub bpm: f32,
    pub beat_phase: f32,
    pub confidence: f32,
    pub locked: bool,
}

impl LiveBeatClock {
    pub(crate) fn set_active(&self, active: bool) {
        self.active.store(active, Ordering::Relaxed);
        if !active {
            // Clear the readout so a stale BPM doesn't linger after capture stops.
            self.locked.store(false, Ordering::Relaxed);
            self.bpm_bits.store(0f32.to_bits(), Ordering::Relaxed);
            self.conf_bits.store(0f32.to_bits(), Ordering::Relaxed);
        }
    }

    pub(crate) fn store(&self, bpm: f32, beat_phase: f32, confidence: f32, locked: bool) {
        // Stamp first so a reader that sees the new phase also sees a fresh-or-older timestamp
        // (never a newer one) — extrapolation then can't run the phase backwards.
        self.stamp_nanos
            .store(self.epoch.elapsed().as_nanos() as u64, Ordering::Relaxed);
        self.bpm_bits.store(bpm.to_bits(), Ordering::Relaxed);
        self.phase_bits
            .store(beat_phase.to_bits(), Ordering::Relaxed);
        self.conf_bits
            .store(confidence.to_bits(), Ordering::Relaxed);
        self.locked.store(locked, Ordering::Relaxed);
    }

    /// Read a snapshot with `beat_phase` **extrapolated to now**: the published phase advanced by
    /// the elapsed time since it was stamped, at the published tempo. This cancels the
    /// analysis/IPC lag so a consumer on another clock (the audio-thread PLL) locks to the live
    /// input's *current* phase, not a stale one. Relaxed loads — the fields are advisory.
    pub fn snapshot(&self) -> LiveBeatSnapshot {
        let bpm = f32::from_bits(self.bpm_bits.load(Ordering::Relaxed));
        let phase = f32::from_bits(self.phase_bits.load(Ordering::Relaxed));
        let stamp = self.stamp_nanos.load(Ordering::Relaxed);
        let age_secs = (self.epoch.elapsed().as_nanos() as u64).saturating_sub(stamp) as f64 / 1e9;
        let beat_phase = if bpm > 0.0 {
            ((phase as f64 + age_secs * bpm as f64 / 60.0).rem_euclid(1.0)) as f32
        } else {
            phase
        };
        LiveBeatSnapshot {
            active: self.active.load(Ordering::Relaxed),
            bpm,
            beat_phase,
            confidence: f32::from_bits(self.conf_bits.load(Ordering::Relaxed)),
            locked: self.locked.load(Ordering::Relaxed),
        }
    }
}

/// Drain the analysis ring, run the causal tracker, and publish to `clock` until the producer (the
/// aux input stream) is dropped and the ring is empty. `consumer` carries interleaved stereo f32 at
/// `sample_rate`; we downmix to mono for the tracker. Runs on its own thread; never on the callback.
pub fn run_live_analysis(mut consumer: Consumer<f32>, sample_rate: u32, clock: Arc<LiveBeatClock>) {
    let mut tracker = LiveTracker::new(sample_rate);
    let mut mono: Vec<f32> = Vec::with_capacity(4096);
    clock.set_active(true);
    loop {
        mono.clear();
        // Drain available stereo pairs → mono (bounded batch so we update the clock promptly).
        while consumer.slots() >= 2 && mono.len() < 4096 {
            let l = consumer.pop().unwrap_or(0.0);
            let r = consumer.pop().unwrap_or(0.0);
            mono.push(0.5 * (l + r));
        }
        if !mono.is_empty() {
            if let Some(est) = tracker.push(&mono) {
                clock.store(est.bpm, est.beat_phase, est.confidence, est.locked);
            }
        } else if consumer.is_abandoned() {
            break; // aux input stopped and the ring is drained
        }
        std::thread::sleep(Duration::from_millis(5));
    }
    clock.set_active(false);
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn clock_round_trips_and_clears_on_deactivate() {
        let c = LiveBeatClock::default();
        c.set_active(true);
        c.store(128.0, 0.25, 0.6, true);
        let s = c.snapshot();
        assert!(s.active);
        assert_eq!(s.bpm, 128.0);
        // beat_phase is extrapolated to "now"; with a just-stamped clock the age is ~microseconds,
        // so it stays within a hair of the stored 0.25.
        assert!(
            (s.beat_phase - 0.25).abs() < 0.02,
            "beat_phase {}",
            s.beat_phase
        );
        assert_eq!(s.confidence, 0.6);
        assert!(s.locked);

        // Deactivating clears the readout so a stale BPM doesn't linger after capture stops.
        c.set_active(false);
        let s = c.snapshot();
        assert!(!s.active);
        assert!(!s.locked);
        assert_eq!(s.bpm, 0.0);
        assert_eq!(s.confidence, 0.0);
    }

    #[test]
    fn run_live_analysis_locks_on_a_click_then_exits_when_abandoned() {
        // Feed ~14 s of 120 BPM mono-into-stereo clicks through a ring while the analysis loop
        // drains it on another thread; it should lock ~120 BPM, then end and clear `active` once
        // the producer is dropped.
        let sr = 44_100u32;
        let (mut tx, rx) = rtrb::RingBuffer::<f32>::new(8192);
        let clock = Arc::new(LiveBeatClock::default());
        let c2 = clock.clone();
        let h = std::thread::spawn(move || run_live_analysis(rx, sr, c2));

        // Build 14 s of 120 BPM clicks (mono); fed as duplicated stereo pairs.
        let total = sr as usize * 14;
        let period = (sr as f32 * 0.5) as usize; // 120 BPM
        let mut mono = vec![0.0f32; total];
        let mut t = 0;
        while t < total {
            for k in 0..64 {
                if let Some(x) = mono.get_mut(t + k) {
                    *x = (1.0 - k as f32 / 64.0) * if k % 2 == 0 { 1.0 } else { -1.0 };
                }
            }
            t += period;
        }
        for &m in &mono {
            while tx.slots() < 2 {
                std::thread::sleep(Duration::from_millis(1));
            }
            let _ = tx.push(m);
            let _ = tx.push(m);
        }
        // Let the analysis drain the tail and publish.
        std::thread::sleep(Duration::from_millis(80));
        let s = clock.snapshot();
        drop(tx); // abandon the ring → the loop drains and exits
        h.join().expect("analysis thread joins");

        assert!(
            s.locked,
            "should lock on a steady click (conf {:.3})",
            s.confidence
        );
        assert!(
            (s.bpm - 120.0).abs() <= 4.0,
            "live BPM should be ~120, got {:.2}",
            s.bpm
        );
        assert!(
            !clock.snapshot().active,
            "active cleared after the loop exits"
        );
    }
}
