//! HID input for non-class-compliant controllers (e.g. Native Instruments Traktor units). `midir`
//! covers MIDI; this reads raw **HID input reports**, diffs consecutive reports, and forwards each
//! *changed* byte both to the controller engine (resolved through `InputKind::Hid` bindings) and to
//! the frontend as a `hid:input` event for the guided learn editor.
//!
//! Scope: absolute single-byte axes (knobs/faders/jogs) — the common continuous case. Bit-packed
//! buttons and device-specific **output/LED** reports are hardware-gated follow-ups (each device has
//! its own report layout); deriving those layouts is a clean-room, per-device task.

use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::mpsc::{self, Sender};
use std::sync::{Arc, Mutex};

use hidapi::HidApi;
use serde::Serialize;
use tauri::{AppHandle, Emitter};

use crate::controllers::ControllerMsg;

/// Reader read-timeout (ms) — small enough to poll the stop flag promptly.
const READ_TIMEOUT_MS: i32 = 50;

/// One enumerated HID device, for the frontend picker.
#[derive(Serialize, Clone)]
pub struct HidDeviceInfo {
    /// Opaque OS path — the stable handle used to open the device.
    pub path: String,
    pub vendor_id: u16,
    pub product_id: u16,
    pub manufacturer: String,
    pub product: String,
}

/// A changed byte in an HID input report (for the learn editor / diagnostics).
#[derive(Serialize, Clone)]
pub struct HidInputEvent {
    pub byte: u8,
    pub value: u8,
}

/// List connected HID devices.
pub fn list_devices() -> Result<Vec<HidDeviceInfo>, String> {
    let api = HidApi::new().map_err(|e| e.to_string())?;
    let mut out = Vec::new();
    for d in api.device_list() {
        let Ok(path) = d.path().to_str() else {
            continue;
        };
        out.push(HidDeviceInfo {
            path: path.to_string(),
            vendor_id: d.vendor_id(),
            product_id: d.product_id(),
            manufacturer: d.manufacturer_string().unwrap_or_default().to_string(),
            product: d.product_string().unwrap_or_default().to_string(),
        });
    }
    Ok(out)
}

/// An open HID device with a background reader thread. Dropping it stops the reader.
pub struct HidConnection {
    stop: Arc<AtomicBool>,
}

impl HidConnection {
    /// Open `path` and spawn the reader. The device is opened *inside* the thread (hidapi handles are
    /// not `Send`); the open result is reported back so this returns synchronously.
    pub fn open(
        app: AppHandle,
        ctrl_tx: Sender<ControllerMsg>,
        path: String,
    ) -> Result<Self, String> {
        let stop = Arc::new(AtomicBool::new(false));
        let stop_thread = stop.clone();
        let (res_tx, res_rx) = mpsc::channel::<Result<(), String>>();

        std::thread::Builder::new()
            .name("compas-hid".into())
            .spawn(move || {
                // Open within the thread; HidApi/HidDevice are !Send.
                let opened = (|| {
                    let api = HidApi::new().map_err(|e| e.to_string())?;
                    let cpath = std::ffi::CString::new(path).map_err(|e| e.to_string())?;
                    api.open_path(&cpath).map_err(|e| e.to_string())
                })();
                let device = match opened {
                    Ok(d) => {
                        let _ = res_tx.send(Ok(()));
                        d
                    }
                    Err(e) => {
                        let _ = res_tx.send(Err(e));
                        return;
                    }
                };

                let mut prev: Vec<u8> = Vec::new();
                let mut buf = [0u8; 256];
                while !stop_thread.load(Ordering::Relaxed) {
                    match device.read_timeout(&mut buf, READ_TIMEOUT_MS) {
                        Ok(0) => {} // timeout, no report this tick
                        Ok(n) => {
                            let report = &buf[..n];
                            // Seed a baseline on the first report (or if the report size changes) so
                            // we don't emit a flood of deltas against an empty/old buffer.
                            if prev.len() != n {
                                prev = report.to_vec();
                                continue;
                            }
                            for (i, (&cur, &old)) in report.iter().zip(prev.iter()).enumerate() {
                                if cur != old {
                                    let byte = i as u8; // reports ≤256 bytes; index fits u8
                                    let _ = ctrl_tx.send(ControllerMsg::Hid { byte, value: cur });
                                    let _ =
                                        app.emit("hid:input", HidInputEvent { byte, value: cur });
                                }
                            }
                            prev.copy_from_slice(report);
                        }
                        Err(_) => break, // device error / disconnect
                    }
                }
            })
            .map_err(|e| e.to_string())?;

        // Surface the open result (channel error only if the thread died before reporting).
        res_rx.recv().map_err(|e| e.to_string())??;
        Ok(HidConnection { stop })
    }
}

impl Drop for HidConnection {
    fn drop(&mut self) {
        self.stop.store(true, Ordering::Relaxed);
    }
}

/// Tauri-managed state holding the active HID connection (if any).
#[derive(Default)]
pub struct HidState(pub Mutex<Option<HidConnection>>);
