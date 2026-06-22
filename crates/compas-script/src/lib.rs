//! Sandboxed controller-scripting runtime.
//!
//! Embeds QuickJS (via `rquickjs`) and exposes a small `engine.*` API so a controller mapping can be
//! expressed as a script when declarative bindings ([`compas_core::mapping`]) aren't enough — jog
//! modes, shift layers, banks, custom LED feedback. Scripts target the same named control bus
//! ([`compas_core::control`]): `engine.set("deck.0.gain", 0.8)` enqueues a control update the host
//! then applies. The JS engine itself runs in a fresh sandbox with no I/O globals — it can only see
//! the `engine` object we install.
//!
//! Threading: the QuickJS runtime is single-threaded; drive it from the control thread that handles
//! MIDI, never the audio callback. `on_midi` invokes the script's `onMidi(status, d1, d2)` handler
//! (if defined) and returns the control updates it produced.

use std::cell::RefCell;
use std::rc::Rc;

use rquickjs::{Context, Function, Runtime};

/// A control change a script asked for: a control-bus id and a value (normalized, by convention).
#[derive(Debug, Clone, PartialEq)]
pub struct ControlUpdate {
    pub control: String,
    pub value: f64,
}

/// Errors from loading or running a controller script.
#[derive(Debug, thiserror::Error)]
pub enum ScriptError {
    #[error("script engine: {0}")]
    Engine(String),
}

fn err<E: std::fmt::Display>(e: E) -> ScriptError {
    ScriptError::Engine(e.to_string())
}

/// A loaded controller script and its QuickJS context. Updates produced by `engine.set(...)` are
/// buffered and drained by the host.
pub struct ScriptRuntime {
    context: Context,
    // Runtime must outlive the context; kept alive here.
    _runtime: Runtime,
    updates: Rc<RefCell<Vec<ControlUpdate>>>,
    logs: Rc<RefCell<Vec<String>>>,
}

impl ScriptRuntime {
    /// Create a fresh sandbox with the `engine` API installed.
    pub fn new() -> Result<Self, ScriptError> {
        let runtime = Runtime::new().map_err(err)?;
        let context = Context::full(&runtime).map_err(err)?;
        let updates: Rc<RefCell<Vec<ControlUpdate>>> = Rc::new(RefCell::new(Vec::new()));
        let logs: Rc<RefCell<Vec<String>>> = Rc::new(RefCell::new(Vec::new()));

        context.with(|ctx| -> Result<(), ScriptError> {
            let globals = ctx.globals();
            let up = updates.clone();
            globals
                .set(
                    "__engine_set",
                    Function::new(ctx.clone(), move |id: String, value: f64| {
                        up.borrow_mut().push(ControlUpdate { control: id, value });
                    })
                    .map_err(err)?,
                )
                .map_err(err)?;
            let lg = logs.clone();
            globals
                .set(
                    "__engine_log",
                    Function::new(ctx.clone(), move |msg: String| {
                        lg.borrow_mut().push(msg);
                    })
                    .map_err(err)?,
                )
                .map_err(err)?;
            // Wrap the natives into a tidy `engine` object; coerce arguments defensively.
            ctx.eval::<(), _>(
                r#"globalThis.engine = {
                    set: (id, v) => __engine_set(String(id), Number(v)),
                    log: (m) => __engine_log(String(m)),
                };"#,
            )
            .map_err(err)?;
            Ok(())
        })?;

        Ok(ScriptRuntime {
            context,
            _runtime: runtime,
            updates,
            logs,
        })
    }

    /// Evaluate script source in the sandbox (e.g. to define `onMidi`).
    pub fn eval(&self, source: &str) -> Result<(), ScriptError> {
        self.context
            .with(|ctx| ctx.eval::<(), _>(source).map_err(err))
    }

    /// Invoke the script's `onMidi(status, data1, data2)` handler (if defined) and return the
    /// control updates it produced. A missing handler is not an error (returns empty).
    pub fn on_midi(
        &self,
        status: u8,
        data1: u8,
        data2: u8,
    ) -> Result<Vec<ControlUpdate>, ScriptError> {
        self.context.with(|ctx| -> Result<(), ScriptError> {
            let handler: Result<Function, _> = ctx.globals().get("onMidi");
            if let Ok(f) = handler {
                f.call::<_, ()>((status, data1, data2)).map_err(err)?;
            }
            Ok(())
        })?;
        Ok(self.updates.borrow_mut().drain(..).collect())
    }

    /// Drain any `engine.log(...)` messages the script emitted (for an in-app console).
    pub fn take_logs(&self) -> Vec<String> {
        self.logs.borrow_mut().drain(..).collect()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn engine_set_enqueues_a_control_update() {
        let rt = ScriptRuntime::new().unwrap();
        rt.eval(r#"engine.set("deck.0.gain", 0.75);"#).unwrap();
        // No onMidi handler, but the eval already produced an update; drain via on_midi.
        let updates = rt.on_midi(0, 0, 0).unwrap();
        assert_eq!(updates.len(), 1);
        assert_eq!(updates[0].control, "deck.0.gain");
        assert!((updates[0].value - 0.75).abs() < 1e-9);
    }

    #[test]
    fn on_midi_handler_maps_cc_to_a_control() {
        let rt = ScriptRuntime::new().unwrap();
        rt.eval(
            r#"globalThis.onMidi = function(status, cc, value) {
                 if (status === 0xB0 && cc === 7) engine.set("deck.0.gain", value / 127);
               };"#,
        )
        .unwrap();
        let updates = rt.on_midi(0xB0, 7, 127).unwrap();
        assert_eq!(updates.len(), 1);
        assert_eq!(updates[0].control, "deck.0.gain");
        assert!((updates[0].value - 1.0).abs() < 1e-9);
        // A non-matching message produces nothing.
        assert!(rt.on_midi(0xB0, 8, 64).unwrap().is_empty());
    }

    #[test]
    fn missing_handler_is_not_an_error() {
        let rt = ScriptRuntime::new().unwrap();
        assert!(rt.on_midi(0x90, 60, 100).unwrap().is_empty());
    }

    #[test]
    fn log_is_captured() {
        let rt = ScriptRuntime::new().unwrap();
        rt.eval(r#"engine.log("hello from a mapping");"#).unwrap();
        let logs = rt.take_logs();
        assert_eq!(logs, vec!["hello from a mapping".to_string()]);
    }

    #[test]
    fn syntax_errors_surface() {
        let rt = ScriptRuntime::new().unwrap();
        assert!(rt.eval("this is not valid javascript {{{").is_err());
    }

    #[test]
    fn sandbox_has_no_filesystem_globals() {
        let rt = ScriptRuntime::new().unwrap();
        // QuickJS without std features exposes no `require`/`process`/file APIs.
        rt.eval(r#"if (typeof require !== "undefined") throw new Error("require leaked");"#)
            .unwrap();
    }
}
