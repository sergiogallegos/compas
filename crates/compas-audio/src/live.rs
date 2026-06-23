//! Live beat-tracking glue (adoption-plan slice 5.2). See `docs/research/live-input-beat-tracking.md`.
//!
//! The aux/mic capture ([`crate::input`]) fans its frames into a second "analysis" ring. A
//! dedicated, non-realtime thread drains that ring, downmixes to mono, feeds the causal
//! [`compas_dsp::LiveTracker`], and publishes the result into a lock-free [`LiveBeatClock`] the UI
//! polls. Nothing here runs on the audio callback.

use std::sync::atomic::{AtomicBool, AtomicU32, Ordering};
use std::sync::Arc;
use std::time::Duration;

use compas_dsp::LiveTracker;
use rtrb::Consumer;

/// Lock-free snapshot of the live beat tracker, shared between the analysis thread (writer) and the
/// UI/IPC (reader). Same atomics pattern as `MonitorLatency` / `DeckTelemetry`.
#[derive(Default)]
pub struct LiveBeatClock {
    active: AtomicBool,
    locked: AtomicBool,
    bpm_bits: AtomicU32,
    phase_bits: AtomicU32,
    conf_bits: AtomicU32,
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
        self.bpm_bits.store(bpm.to_bits(), Ordering::Relaxed);
        self.phase_bits
            .store(beat_phase.to_bits(), Ordering::Relaxed);
        self.conf_bits
            .store(confidence.to_bits(), Ordering::Relaxed);
        self.locked.store(locked, Ordering::Relaxed);
    }

    /// Read a consistent-enough snapshot (relaxed loads; the fields are advisory UI state).
    pub fn snapshot(&self) -> LiveBeatSnapshot {
        LiveBeatSnapshot {
            active: self.active.load(Ordering::Relaxed),
            bpm: f32::from_bits(self.bpm_bits.load(Ordering::Relaxed)),
            beat_phase: f32::from_bits(self.phase_bits.load(Ordering::Relaxed)),
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
        assert_eq!(s.beat_phase, 0.25);
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
