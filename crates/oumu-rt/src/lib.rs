//! A small stand-in for the parts of Tauri that Oumu's logic uses, so that logic runs
//! unchanged under a Rune front end. Oumu depends on this crate under the name `tauri`.
//!
//! - `AppHandle::emit` / `emit_to` hand events to the UI (a sink the front end installs).
//! - Windows are labels with a desired state (position, size, visible, click-through,
//!   focus). The logic sets it; the front end applies it to real windows every frame and
//!   reports back the actual frame.
//! - `async_runtime::spawn` runs futures on one shared tokio runtime.

use serde::Serialize;
use std::any::{Any, TypeId};
use std::collections::HashMap;
use std::sync::{Arc, Mutex, OnceLock};

#[derive(Debug)]
pub struct Error(pub String);

impl std::fmt::Display for Error {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str(&self.0)
    }
}

impl std::error::Error for Error {}

pub type Result<T> = std::result::Result<T, Error>;

/// An event for the UI: `target` None = every window.
#[derive(Clone, Debug)]
pub struct UiEvent {
    pub target: Option<String>,
    pub name: String,
    pub payload: serde_json::Value,
}

/// What the logic wants a window to be (logical px).
#[derive(Clone, Debug, Default, PartialEq)]
pub struct WinState {
    pub visible: bool,
    pub x: f64,
    pub y: f64,
    pub w: f64,
    pub h: f64,
    pub passthrough: bool,
    /// bumped on each focus request
    pub focus_seq: u64,
    /// bumped whenever the logic changes something
    pub seq: u64,
}

#[derive(Clone, Copy, Debug)]
pub struct MonitorInfo {
    pub x: i32,
    pub y: i32,
    pub w: u32,
    pub h: u32,
    pub scale: f64,
}

type Sink = Box<dyn Fn(UiEvent) + Send + Sync>;

struct Inner {
    sink: Mutex<Option<Sink>>,
    windows: Mutex<HashMap<String, WinState>>,
    states: Mutex<HashMap<TypeId, Arc<dyn Any + Send + Sync>>>,
    monitor: Mutex<Option<MonitorInfo>>,
    exit: Mutex<Option<i32>>,
}

#[derive(Clone)]
pub struct AppHandle(Arc<Inner>);

impl Default for AppHandle {
    fn default() -> Self {
        Self::new()
    }
}

impl AppHandle {
    pub fn new() -> AppHandle {
        AppHandle(Arc::new(Inner { sink: Mutex::new(None), windows: Mutex::new(HashMap::new()), states: Mutex::new(HashMap::new()), monitor: Mutex::new(None), exit: Mutex::new(None) }))
    }

    /// The front end receives every event through this.
    pub fn set_sink(&self, f: impl Fn(UiEvent) + Send + Sync + 'static) {
        *self.0.sink.lock().unwrap() = Some(Box::new(f));
    }

    fn send(&self, target: Option<&str>, name: &str, payload: serde_json::Value) {
        if let Some(s) = self.0.sink.lock().unwrap().as_ref() {
            s(UiEvent { target: target.map(String::from), name: name.into(), payload });
        }
    }

    /// Store shared state (Tauri's `manage`).
    pub fn manage<T: Send + Sync + 'static>(&self, v: T) {
        self.0.states.lock().unwrap().insert(TypeId::of::<T>(), Arc::new(v));
    }

    pub fn state<T: Send + Sync + 'static>(&self) -> State<'static, T> {
        let a = self.0.states.lock().unwrap().get(&TypeId::of::<T>()).cloned().expect("state not managed");
        State(Owned(a.downcast::<T>().expect("state type")), std::marker::PhantomData)
    }

    /// Declare a window the front end will show for `label`.
    pub fn register_window(&self, label: &str, st: WinState) {
        self.0.windows.lock().unwrap().insert(label.into(), st);
    }

    pub fn window_state(&self, label: &str) -> Option<WinState> {
        self.0.windows.lock().unwrap().get(label).cloned()
    }

    pub fn window_labels(&self) -> Vec<String> {
        self.0.windows.lock().unwrap().keys().cloned().collect()
    }

    /// The front end reports where a window really is (after the user moved it, say).
    pub fn report_frame(&self, label: &str, x: f64, y: f64, w: f64, h: f64) {
        if let Some(s) = self.0.windows.lock().unwrap().get_mut(label) {
            s.x = x;
            s.y = y;
            s.w = w;
            s.h = h;
        }
    }

    fn update(&self, label: &str, f: impl FnOnce(&mut WinState)) {
        if let Some(s) = self.0.windows.lock().unwrap().get_mut(label) {
            let before = s.clone();
            f(s);
            if *s != before {
                s.seq += 1;
            }
        }
        self.send(Some(label), "__window", serde_json::Value::Null);
    }

    pub fn set_primary_monitor(&self, m: MonitorInfo) {
        *self.0.monitor.lock().unwrap() = Some(m);
    }

    pub fn primary_monitor(&self) -> Result<Option<Monitor>> {
        Ok(self.0.monitor.lock().unwrap().map(Monitor))
    }

    pub fn get_webview_window(&self, label: &str) -> Option<WebviewWindow> {
        self.0.windows.lock().unwrap().contains_key(label).then(|| WebviewWindow { app: self.clone(), label: label.into() })
    }

    pub fn exit(&self, code: i32) {
        *self.0.exit.lock().unwrap() = Some(code);
        self.send(None, "__exit", serde_json::Value::Null);
    }

    pub fn exit_requested(&self) -> Option<i32> {
        *self.0.exit.lock().unwrap()
    }
}

/// `app.emit(...)` / `app.emit_to(...)`.
pub trait Emitter {
    fn emit<S: Serialize + Clone>(&self, event: &str, payload: S) -> Result<()>;
    fn emit_to<S: Serialize + Clone>(&self, target: &str, event: &str, payload: S) -> Result<()>;
}

impl Emitter for AppHandle {
    fn emit<S: Serialize + Clone>(&self, event: &str, payload: S) -> Result<()> {
        self.send(None, event, serde_json::to_value(payload).map_err(|e| Error(e.to_string()))?);
        Ok(())
    }
    fn emit_to<S: Serialize + Clone>(&self, target: &str, event: &str, payload: S) -> Result<()> {
        self.send(Some(target), event, serde_json::to_value(payload).map_err(|e| Error(e.to_string()))?);
        Ok(())
    }
}

/// Kept so `use tauri::Manager` still compiles.
pub trait Manager {}
impl Manager for AppHandle {}

struct Owned<T>(Arc<T>);

/// Tauri's managed-state handle.
pub struct State<'a, T: 'static>(Owned<T>, std::marker::PhantomData<&'a T>);

impl<'a, T> State<'a, T> {
    pub fn inner(&self) -> &T {
        &self.0.0
    }
    pub fn from_arc(a: Arc<T>) -> Self {
        State(Owned(a), std::marker::PhantomData)
    }
}

impl<T> std::ops::Deref for State<'_, T> {
    type Target = T;
    fn deref(&self) -> &T {
        &self.0.0
    }
}

pub struct Monitor(MonitorInfo);

impl Monitor {
    pub fn scale_factor(&self) -> f64 {
        self.0.scale
    }
    pub fn size(&self) -> PhysicalSize<u32> {
        PhysicalSize { width: self.0.w, height: self.0.h }
    }
    pub fn position(&self) -> PhysicalPosition<i32> {
        PhysicalPosition { x: self.0.x, y: self.0.y }
    }
}

#[derive(Clone, Copy, Debug)]
pub struct PhysicalSize<T> {
    pub width: T,
    pub height: T,
}

#[derive(Clone, Copy, Debug)]
pub struct PhysicalPosition<T> {
    pub x: T,
    pub y: T,
}

#[derive(Clone, Copy, Debug)]
pub struct LogicalPosition<T> {
    pub x: T,
    pub y: T,
}

impl<T> LogicalPosition<T> {
    pub fn new(x: T, y: T) -> Self {
        LogicalPosition { x, y }
    }
}

#[derive(Clone, Copy, Debug)]
pub struct LogicalSize<T> {
    pub width: T,
    pub height: T,
}

impl<T> LogicalSize<T> {
    pub fn new(width: T, height: T) -> Self {
        LogicalSize { width, height }
    }
}

pub enum Position {
    Logical(LogicalPosition<f64>),
}

pub enum Size {
    Logical(LogicalSize<f64>),
}

/// A window by label; every call goes to its desired state.
pub struct WebviewWindow {
    app: AppHandle,
    label: String,
}

impl WebviewWindow {
    pub fn label(&self) -> &str {
        &self.label
    }
    pub fn app_handle(&self) -> &AppHandle {
        &self.app
    }
    fn scale(&self) -> f64 {
        self.app.0.monitor.lock().unwrap().map_or(1.0, |m| m.scale)
    }
    pub fn show(&self) -> Result<()> {
        self.app.update(&self.label, |s| s.visible = true);
        Ok(())
    }
    pub fn hide(&self) -> Result<()> {
        self.app.update(&self.label, |s| s.visible = false);
        Ok(())
    }
    pub fn is_visible(&self) -> Result<bool> {
        Ok(self.app.window_state(&self.label).is_some_and(|s| s.visible))
    }
    pub fn set_focus(&self) -> Result<()> {
        self.app.update(&self.label, |s| s.focus_seq += 1);
        Ok(())
    }
    pub fn set_position(&self, p: Position) -> Result<()> {
        let Position::Logical(p) = p;
        self.app.update(&self.label, |s| {
            s.x = p.x;
            s.y = p.y;
        });
        Ok(())
    }
    pub fn set_size(&self, z: Size) -> Result<()> {
        let Size::Logical(z) = z;
        self.app.update(&self.label, |s| {
            s.w = z.width;
            s.h = z.height;
        });
        Ok(())
    }
    pub fn scale_factor(&self) -> Result<f64> {
        Ok(self.scale())
    }
    pub fn outer_position(&self) -> Result<PhysicalPosition<i32>> {
        let s = self.app.window_state(&self.label).ok_or_else(|| Error("no window".into()))?;
        let k = self.scale();
        Ok(PhysicalPosition { x: (s.x * k).round() as i32, y: (s.y * k).round() as i32 })
    }
    pub fn outer_size(&self) -> Result<PhysicalSize<u32>> {
        let s = self.app.window_state(&self.label).ok_or_else(|| Error("no window".into()))?;
        let k = self.scale();
        Ok(PhysicalSize { width: (s.w * k).round() as u32, height: (s.h * k).round() as u32 })
    }
    pub fn set_ignore_cursor_events(&self, on: bool) -> Result<()> {
        self.app.update(&self.label, |s| s.passthrough = on);
        Ok(())
    }
}

pub mod async_runtime {
    use super::*;

    pub fn runtime() -> &'static tokio::runtime::Runtime {
        static RT: OnceLock<tokio::runtime::Runtime> = OnceLock::new();
        RT.get_or_init(|| tokio::runtime::Builder::new_multi_thread().worker_threads(2).enable_all().thread_name("oumu-rt").build().expect("tokio runtime"))
    }

    pub fn spawn<F>(f: F) -> tokio::task::JoinHandle<F::Output>
    where
        F: std::future::Future + Send + 'static,
        F::Output: Send + 'static,
    {
        runtime().spawn(f)
    }

    pub fn block_on<F: std::future::Future>(f: F) -> F::Output {
        runtime().block_on(f)
    }
}
