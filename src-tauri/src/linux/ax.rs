//! The Linux side of `crate::ax`: the focused text field through AT-SPI (the
//! accessibility bus every GTK, Qt, Firefox, Chromium and Electron app speaks),
//! with screen geometry from Hyprland.
//!
//! The surface mirrors the macOS module (same names, same AX attribute strings,
//! UTF-16 offsets), so `host.rs` runs unchanged. AT-SPI counts offsets in
//! characters (code points); the UTF-16 <-> character mapping happens here and
//! only here.
//!
//! Wayland hands a client no global coordinates, so every AT-SPI extent is read
//! window-relative and moved onto the screen by the window position Hyprland
//! reports for the app's pid.

use super::desktop;
use serde::{Deserialize, Serialize};
use std::collections::hash_map::DefaultHasher;
use std::hash::{Hash, Hasher};
use std::sync::{Mutex, OnceLock};
use std::time::{Duration, Instant};
use zbus::blocking::Connection;
use zbus::zvariant::{OwnedObjectPath, OwnedValue, Structure};

/// A hung app must not stall the host loop (macOS uses the same 0.5 s AX timeout).
const CALL_TIMEOUT: Duration = Duration::from_millis(500);
const TRUST_CACHE: Duration = Duration::from_secs(2);

const IFACE_ACCESSIBLE: &str = "org.a11y.atspi.Accessible";
const IFACE_TEXT: &str = "org.a11y.atspi.Text";
const IFACE_EDITABLE: &str = "org.a11y.atspi.EditableText";
const IFACE_COMPONENT: &str = "org.a11y.atspi.Component";

const ROLE_FRAME: u32 = 23;
const ROLE_DIALOG: u32 = 16;
const ROLE_PASSWORD_TEXT: u32 = 40;
const ROLE_SCROLL_PANE: u32 = 49;
const ROLE_TERMINAL: u32 = 60;
const ROLE_TEXT: u32 = 61;
const ROLE_VIEWPORT: u32 = 68;
const ROLE_WINDOW: u32 = 69;
const ROLE_APPLICATION: u32 = 75;
const ROLE_ENTRY: u32 = 79;

const STATE_ACTIVE: u32 = 1;
const STATE_DEFUNCT: u32 = 6;
const STATE_EDITABLE: u32 = 7;
const STATE_FOCUSED: u32 = 12;
const STATE_MULTI_LINE: u32 = 17;
const STATE_READ_ONLY: u32 = 43;
const STATE_SHOWING: u32 = 25;

const GRANULARITY_LINE: u32 = 3;
/// Characters around the visible text whose underlines are placed too (a
/// partly shown line, a scroll in progress).
const VISIBLE_SLACK: usize = 300;
const COORD_SCREEN: u32 = 0;
const COORD_WINDOW: u32 = 1;

/// Rich editors (web contenteditable, LibreOffice) splice child objects into
/// their text as U+FFFC; such a field is not a plain string we can patch.
const EMBEDDED_OBJECT: char = '\u{FFFC}';

/// A screen rectangle in logical points, origin at the top-left of the
/// compositor layout (the coordinate system Hyprland reports in).
#[derive(Clone, Copy, Debug, Default, PartialEq, Serialize, Deserialize)]
pub struct Rect {
    pub x: f64,
    pub y: f64,
    pub w: f64,
    pub h: f64,
}

impl Rect {
    pub fn intersect(&self, o: &Rect) -> Option<Rect> {
        let x1 = self.x.max(o.x);
        let y1 = self.y.max(o.y);
        let x2 = (self.x + self.w).min(o.x + o.w);
        let y2 = (self.y + self.h).min(o.y + o.h);
        if x2 > x1 && y2 > y1 {
            Some(Rect { x: x1, y: y1, w: x2 - x1, h: y2 - y1 })
        } else {
            None
        }
    }
    /// The smallest rect holding both.
    pub fn union(&self, o: &Rect) -> Rect {
        let (x, y) = (self.x.min(o.x), self.y.min(o.y));
        Rect { x, y, w: (self.x + self.w).max(o.x + o.w) - x, h: (self.y + self.h).max(o.y + o.h) - y }
    }

    pub fn contains(&self, px: f64, py: f64) -> bool {
        px >= self.x && px <= self.x + self.w && py >= self.y && py <= self.y + self.h
    }
    pub fn is_empty(&self) -> bool {
        self.w <= 0.0 || self.h <= 0.0
    }
}

// ---------------------------------------------------------------------------
// UTF-16 <-> character offsets

/// Character (code point) index for a UTF-16 offset. An offset inside a
/// surrogate pair rounds down to the character that holds it.
pub fn u16_to_char(text: &str, u16_off: usize) -> usize {
    let mut units = 0;
    for (i, c) in text.chars().enumerate() {
        let next = units + c.len_utf16();
        if next > u16_off {
            return i;
        }
        units = next;
    }
    text.chars().count()
}

/// UTF-16 offset for a character index (clamped to the end of the text).
pub fn char_to_u16(text: &str, char_off: usize) -> usize {
    text.chars().take(char_off).map(char::len_utf16).sum()
}

// ---------------------------------------------------------------------------
// The accessibility bus

struct Bus {
    conn: Mutex<Option<Connection>>,
}

fn bus() -> &'static Bus {
    static BUS: OnceLock<Bus> = OnceLock::new();
    BUS.get_or_init(|| Bus { conn: Mutex::new(None) })
}

/// After a failed connect, wait this long before trying again (the host loop ticks every 220 ms).
const RECONNECT_BACKOFF: Duration = Duration::from_secs(5);
/// An app that let a call time out is left alone for this long, so one hung
/// app cannot stall every host tick.
const SLOW_APP_BACKOFF: Duration = Duration::from_secs(2);

/// The session bus, opened once, with the same call timeout as the a11y bus.
fn session() -> Option<Connection> {
    static SESSION: Mutex<Option<Connection>> = Mutex::new(None);
    let mut g = SESSION.lock().ok()?;
    if g.is_none() {
        *g = zbus::blocking::connection::Builder::session().ok()?.method_timeout(CALL_TIMEOUT).build().ok();
    }
    g.clone()
}

fn session_call<B>(iface: &str, method: &str, body: &B) -> Option<zbus::Message>
where
    B: serde::Serialize + zbus::zvariant::DynamicType,
{
    session()?.call_method(Some("org.a11y.Bus"), "/org/a11y/bus", Some(iface), method, body).ok()
}

/// The a11y bus address, from the session bus (this also starts the bus if needed).
fn a11y_address() -> Option<String> {
    session_call("org.a11y.Bus", "GetAddress", &())?.body().deserialize::<String>().ok()
}

pub(super) fn open_a11y() -> Option<Connection> {
    let addr = a11y_address()?;
    zbus::blocking::connection::Builder::address(addr.as_str())
        .ok()?
        .method_timeout(CALL_TIMEOUT)
        .build()
        .map_err(|e| log::warn!("a11y bus: {e}"))
        .ok()
}

/// The shared a11y connection, reconnecting after a bus restart (with a back-off,
/// and without holding the lock while connecting).
fn conn() -> Option<Connection> {
    static LAST_FAIL: Mutex<Option<Instant>> = Mutex::new(None);
    if let Some(c) = bus().conn.lock().ok()?.clone() {
        return Some(c);
    }
    if LAST_FAIL.lock().ok()?.map(|t| t.elapsed() < RECONNECT_BACKOFF).unwrap_or(false) {
        return None;
    }
    let fresh = open_a11y();
    match &fresh {
        Some(c) => *bus().conn.lock().ok()? = Some(c.clone()),
        None => *LAST_FAIL.lock().ok()? = Some(Instant::now()),
    }
    fresh
}

fn drop_conn() {
    if let Ok(mut g) = bus().conn.lock() {
        *g = None;
    }
}

/// Bus names of apps that recently let a call time out, and until when to skip them.
fn slow_apps() -> &'static Mutex<std::collections::HashMap<String, Instant>> {
    static SLOW: OnceLock<Mutex<std::collections::HashMap<String, Instant>>> = OnceLock::new();
    SLOW.get_or_init(Default::default)
}

/// Accessibility calls made so far (the debug probe reports it, to keep idle cost honest).
pub static CALLS: std::sync::atomic::AtomicU64 = std::sync::atomic::AtomicU64::new(0);

fn call<B, R>(dest: &str, path: &str, iface: &str, method: &str, body: &B) -> Option<R>
where
    B: serde::Serialize + zbus::zvariant::DynamicType,
    R: for<'d> zbus::zvariant::DynamicDeserialize<'d>,
{
    CALLS.fetch_add(1, std::sync::atomic::Ordering::Relaxed);
    if let Ok(mut slow) = slow_apps().lock() {
        match slow.get(dest) {
            Some(until) if Instant::now() < *until => return None,
            Some(_) => {
                slow.remove(dest);
            }
            None => {}
        }
    }
    let c = conn()?;
    let t0 = Instant::now();
    match c.call_method(Some(dest), path, Some(iface), method, body) {
        Ok(m) => m.body().deserialize::<R>().ok(),
        Err(e) => {
            if t0.elapsed() >= CALL_TIMEOUT.mul_f32(0.9) {
                log::debug!("a11y: {dest} timed out on {method}; skipping it for {SLOW_APP_BACKOFF:?}");
                if let Ok(mut slow) = slow_apps().lock() {
                    slow.insert(dest.to_string(), Instant::now() + SLOW_APP_BACKOFF);
                }
            } else if matches!(e, zbus::Error::InputOutput(_)) {
                // The bus itself went away (logout, at-spi restart): reconnect next time.
                drop_conn();
            }
            None
        }
    }
}

/// The process behind a bus name (unique names are never reused, so this is cached).
fn bus_pid(bus: &str) -> Option<i32> {
    static PIDS: OnceLock<Mutex<std::collections::HashMap<String, i32>>> = OnceLock::new();
    let pids = PIDS.get_or_init(Default::default);
    if let Some(p) = pids.lock().ok()?.get(bus) {
        return Some(*p);
    }
    let pid = conn()?
        .call_method(Some("org.freedesktop.DBus"), "/org/freedesktop/DBus", Some("org.freedesktop.DBus"), "GetConnectionUnixProcessID", &(bus,))
        .ok()?
        .body()
        .deserialize::<u32>()
        .ok()? as i32;
    let mut g = pids.lock().ok()?;
    if g.len() > 512 {
        g.clear();
    }
    g.insert(bus.to_string(), pid);
    Some(pid)
}

/// Keyboard focus is inside Oumu itself (the panel took it). The host keeps
/// the last document then, as it does when Oumu is the frontmost app on macOS.
pub fn focus_is_ours() -> bool {
    let Some((bus, _)) = focus::CURRENT.lock().ok().and_then(|g| g.clone()) else { return false };
    bus_pid(&bus) == Some(std::process::id() as i32)
}

fn prop<R: TryFrom<OwnedValue>>(dest: &str, path: &str, iface: &str, name: &str) -> Option<R> {
    let v: OwnedValue = call(dest, path, "org.freedesktop.DBus.Properties", "Get", &(iface, name))?;
    R::try_from(v).ok()
}

// ---------------------------------------------------------------------------
// Trust: on Linux "trusted" means the accessibility bus is switched on, so
// toolkits export their widget trees.

fn status_enabled() -> Option<bool> {
    let reply = session_call("org.freedesktop.DBus.Properties", "Get", &("org.a11y.Status", "IsEnabled"))?;
    let v: OwnedValue = reply.body().deserialize().ok()?;
    bool::try_from(v).ok()
}

/// Whether the accessibility bus is on (cached for two seconds; the host loop asks every tick).
static TRUST: Mutex<Option<(Instant, bool)>> = Mutex::new(None);

pub fn is_trusted() -> bool {
    if let Ok(mut g) = TRUST.lock() {
        if let Some((t, v)) = *g {
            if t.elapsed() < TRUST_CACHE {
                return v;
            }
        }
        let v = status_enabled().unwrap_or(false);
        *g = Some((Instant::now(), v));
        return v;
    }
    false
}

/// The user turned accessibility on before: `prompt_trust` saves it in the
/// `toolkit-accessibility` setting, which outlives the bus's own switch.
pub fn consent_saved() -> bool {
    std::process::Command::new("gsettings")
        .args(["get", "org.gnome.desktop.interface", "toolkit-accessibility"])
        .output()
        .map(|o| o.status.success() && String::from_utf8_lossy(&o.stdout).trim() == "true")
        .unwrap_or(false)
}

/// Turn the accessibility bus on: `org.a11y.Status.IsEnabled = true`, the
/// switch a screen reader flips. at-spi2-core turns it off again when the
/// process that set it exits, so the choice is also saved in the
/// `toolkit-accessibility` setting (see `consent_saved`). Apps that were
/// already running when it flipped need a restart to export their text.
pub fn prompt_trust() -> bool {
    let set = session_call("org.freedesktop.DBus.Properties", "Set", &("org.a11y.Status", "IsEnabled", zbus::zvariant::Value::from(true)));
    if set.is_none() {
        log::warn!("could not switch the accessibility bus on (is at-spi2-core installed?)");
    }
    // The saved consent: Oumu turns the bus back on at its next launch.
    let _ = std::process::Command::new("gsettings").args(["set", "org.gnome.desktop.interface", "toolkit-accessibility", "true"]).status();
    focus::start();
    std::thread::sleep(Duration::from_millis(150));
    let on = status_enabled().unwrap_or(false);
    if let Ok(mut g) = TRUST.lock() {
        *g = Some((Instant::now(), on));
    }
    on
}

// ---------------------------------------------------------------------------
// Focus tracking: apps announce focus with `object:state-changed:focused`.

pub(super) mod focus {
    use super::*;
    use std::sync::atomic::{AtomicBool, Ordering};

    /// (bus name, object path) of the most recently focused accessible.
    pub static CURRENT: Mutex<Option<(String, String)>> = Mutex::new(None);
    static RUNNING: AtomicBool = AtomicBool::new(false);

    /// Ask every app to emit focus events (toolkits only send what a listener registered for).
    fn register(c: &Connection) {
        let event = "object:state-changed:focused";
        let reg = |body: &dyn Fn() -> zbus::Result<zbus::Message>| body().is_ok();
        let path = "/org/a11y/atspi/registry";
        let dest = "org.a11y.atspi.Registry";
        let iface = Some("org.a11y.atspi.Registry");
        // at-spi2-core 2.52+: RegisterEvent(s event, as properties, s app_bus_name); older: RegisterEvent(s).
        if !reg(&|| c.call_method(Some(dest), path, iface, "RegisterEvent", &(event, Vec::<String>::new(), ""))) {
            let _ = c.call_method(Some(dest), path, iface, "RegisterEvent", &(event,));
        }
    }

    fn listen() -> zbus::Result<()> {
        let c = open_a11y().ok_or_else(|| zbus::Error::Failure("no a11y bus".into()))?;
        register(&c);
        let rule = zbus::MatchRule::builder()
            .msg_type(zbus::message::Type::Signal)
            .interface("org.a11y.atspi.Event.Object")?
            .member("StateChanged")?
            .arg(0, "focused")?
            .build();
        let it = zbus::blocking::MessageIterator::for_match_rule(rule, &c, Some(64))?;
        for msg in it {
            let Ok(msg) = msg else { continue };
            let header = msg.header();
            let (Some(sender), Some(path)) = (header.sender(), header.path()) else { continue };
            let Some(gained) = gained(&msg) else { continue };
            let key = (sender.to_string(), path.to_string());
            if let Ok(mut g) = CURRENT.lock() {
                if gained {
                    *g = Some(key);
                } else if g.as_ref() == Some(&key) {
                    *g = None;
                }
            }
            crate::pace::wake();
        }
        Ok(())
    }

    /// `detail1` of an `object:state-changed:focused` signal: 1 gained, 0 lost.
    /// The body is `(siiva{sv})` on current at-spi2-core and `(siiv(so))` on old
    /// ones; only the leading `(s, i)` matters, read through the dynamic signature.
    pub(super) fn gained(msg: &zbus::Message) -> Option<bool> {
        let body = msg.body();
        let s: Structure = body.deserialize().ok()?;
        let detail1 = i32::try_from(s.fields().get(1)?).ok()?;
        Some(detail1 == 1)
    }

    /// Objects visited at most per tree search, and how often one window is
    /// searched again while its focus event stays the same.
    const SEARCH_LIMIT: usize = 400;
    const SEARCH_EVERY: Duration = Duration::from_secs(5);

    fn children(bus: &str, path: &str) -> Vec<(String, OwnedObjectPath)> {
        call(bus, path, IFACE_ACCESSIBLE, "GetChildren", &()).unwrap_or_default()
    }

    fn states(bus: &str, path: &str) -> u64 {
        AXElement::node(bus.to_string(), path.to_string(), 0).states()
    }

    /// The focused object of the app `pid`, found by walking its tree from the
    /// active window down through showing objects. Throttled per window and
    /// focus event (`current`), so a focused button is not searched every tick.
    pub(super) fn search(pid: i32, window: &str, current: Option<&(String, String)>) -> Option<(String, String)> {
        type Last = Option<(String, Option<(String, String)>, Instant)>;
        static LAST: Mutex<Last> = Mutex::new(None);
        {
            let mut g = LAST.lock().ok()?;
            if let Some((w, c, t)) = g.as_ref() {
                if w == window && c.as_ref() == current && t.elapsed() < SEARCH_EVERY {
                    return None;
                }
            }
            *g = Some((window.to_string(), current.cloned(), Instant::now()));
        }
        let apps = children("org.a11y.atspi.Registry", "/org/a11y/atspi/accessible/root");
        let (bus, root) = apps.into_iter().find(|(b, _)| bus_pid(b) == Some(pid))?;
        let tops = children(&bus, root.as_str());
        let active: Vec<_> = tops.iter().filter(|(b, p)| AXElement::has_state(states(b, p.as_str()), STATE_ACTIVE)).cloned().collect();
        let mut queue: std::collections::VecDeque<_> = if active.is_empty() { tops } else { active }.into();
        // Browsers mark the page focused as well as the field inside it: an
        // editable focused object wins, any other focused one is the fallback.
        let mut fallback = None;
        let mut seen = 0;
        log::debug!("a11y: searching the tree of pid {pid} for the focused object");
        while let Some((b, p)) = queue.pop_front() {
            seen += 1;
            if seen > SEARCH_LIMIT {
                break;
            }
            let st = states(&b, p.as_str());
            if AXElement::has_state(st, STATE_FOCUSED) && !AXElement::has_state(st, STATE_DEFUNCT) {
                if AXElement::has_state(st, STATE_EDITABLE) {
                    return Some((b, p.to_string()));
                }
                fallback.get_or_insert_with(|| (b.clone(), p.to_string()));
            }
            if seen == 1 || AXElement::has_state(st, STATE_SHOWING) {
                queue.extend(children(&b, p.as_str()));
            }
        }
        fallback
    }

    /// Start the listener thread once; it reconnects if the bus goes away.
    pub fn start() {
        if RUNNING.swap(true, Ordering::SeqCst) {
            return;
        }
        std::thread::Builder::new()
            .name("a11y-focus".into())
            .spawn(|| loop {
                if let Err(e) = listen() {
                    log::debug!("a11y focus listener: {e}");
                }
                std::thread::sleep(Duration::from_secs(2));
            })
            .map_err(|e| log::error!("a11y focus thread: {e}"))
            .ok();
    }
}

// ---------------------------------------------------------------------------
// Elements

#[derive(Clone, Debug, PartialEq)]
enum Kind {
    /// The desktop-wide entry point (`AXUIElementCreateSystemWide`).
    System,
    /// One accessible object.
    Node { bus: String, path: String },
    /// The app's window as Hyprland sees it (AT-SPI cannot place it on screen).
    Window(Rect),
}

/// An accessible object, or the stand-ins `system_wide()` and `window()` return.
#[derive(Clone, Debug)]
pub struct AXElement {
    kind: Kind,
    pid: i32,
    /// Last full text read, for UTF-16 <-> character conversion between reads.
    text: std::sync::Arc<Mutex<String>>,
}

impl AXElement {
    fn node(bus: String, path: String, pid: i32) -> AXElement {
        AXElement { kind: Kind::Node { bus, path }, pid, text: Default::default() }
    }

    pub fn system_wide() -> AXElement {
        focus::start();
        AXElement { kind: Kind::System, pid: 0, text: Default::default() }
    }

    pub fn set_timeout(&self, _secs: f32) {}

    pub fn pid(&self) -> i32 {
        self.pid
    }

    pub fn same_as(&self, other: &AXElement) -> bool {
        self.kind == other.kind
    }

    pub fn identity_hash(&self) -> u64 {
        let mut h = DefaultHasher::new();
        match &self.kind {
            Kind::Node { bus, path } => (bus, path).hash(&mut h),
            Kind::System => 0u8.hash(&mut h),
            Kind::Window(r) => (r.x as i64, r.y as i64).hash(&mut h),
        }
        h.finish()
    }

    fn ids(&self) -> Option<(&str, &str)> {
        match &self.kind {
            Kind::Node { bus, path } => Some((bus, path)),
            _ => None,
        }
    }

    fn call<B, R>(&self, iface: &str, method: &str, body: &B) -> Option<R>
    where
        B: serde::Serialize + zbus::zvariant::DynamicType,
        R: for<'d> zbus::zvariant::DynamicDeserialize<'d>,
    {
        let (bus, path) = self.ids()?;
        call(bus, path, iface, method, body)
    }

    fn role_id(&self) -> Option<u32> {
        self.call(IFACE_ACCESSIBLE, "GetRole", &())
    }

    fn states(&self) -> u64 {
        self.call::<_, Vec<u32>>(IFACE_ACCESSIBLE, "GetState", &())
            .map(|v| v.first().copied().unwrap_or(0) as u64 | (v.get(1).copied().unwrap_or(0) as u64) << 32)
            .unwrap_or(0)
    }

    fn has_state(states: u64, s: u32) -> bool {
        states & (1u64 << s) != 0
    }

    fn interfaces(&self) -> Vec<String> {
        self.call(IFACE_ACCESSIBLE, "GetInterfaces", &()).unwrap_or_default()
    }

    fn parent_node(&self) -> Option<AXElement> {
        let (bus, path) = self.ids()?;
        let (pbus, ppath): (String, OwnedObjectPath) = prop(bus, path, IFACE_ACCESSIBLE, "Parent").or_else(|| {
            let v: OwnedValue = call(bus, path, "org.freedesktop.DBus.Properties", "Get", &(IFACE_ACCESSIBLE, "Parent"))?;
            let s = Structure::try_from(v).ok()?;
            let f = s.fields();
            Some((String::try_from(f.first()?.try_clone().ok()?).ok()?, OwnedObjectPath::try_from(f.get(1)?.try_clone().ok()?).ok()?))
        })?;
        if ppath.as_str() == "/org/a11y/atspi/null" || ppath.as_str() == "/org/a11y/atspi/accessible/root" && pbus == "org.a11y.atspi.Registry" {
            return None;
        }
        Some(AXElement::node(pbus, ppath.to_string(), self.pid))
    }

    /// The focused, still-alive accessible of the focused window, if it is ours to read.
    pub fn focused_element(&self) -> Option<AXElement> {
        // The focus event can outlive the window: only the app in front counts.
        let front = desktop::front()?;
        let current = focus::CURRENT.lock().ok()?.clone();
        let live = current
            .clone()
            .filter(|(bus, _)| bus_pid(bus) == Some(front.pid))
            .map(|(bus, path)| AXElement::node(bus, path, front.pid))
            .and_then(|el| el.usable_focus().map(|editable| (el, editable)));
        let live = match live {
            Some((el, true)) => return Some(el),
            other => other.map(|(el, _)| el),
        };
        // No event for an editable field of the front app: it took focus before
        // the listener was up, or the event named the page around the field
        // (browsers). Look the focused field up in the app's tree.
        match focus::search(front.pid, &front.key, current.as_ref()) {
            Some((bus, path)) => {
                if let Ok(mut g) = focus::CURRENT.lock() {
                    *g = Some((bus.clone(), path.clone()));
                }
                Some(AXElement::node(bus, path, front.pid)).filter(|el| el.usable_focus().is_some())
            }
            None => live,
        }
    }

    /// `Some(editable)` while the object holds focus and is alive, else `None`.
    fn usable_focus(&self) -> Option<bool> {
        let st = self.states();
        (Self::has_state(st, STATE_FOCUSED) && !Self::has_state(st, STATE_DEFUNCT)).then(|| Self::has_state(st, STATE_EDITABLE))
    }

    pub fn parent(&self) -> Option<AXElement> {
        self.parent_node()
    }

    /// The window that holds this element, placed by Hyprland.
    pub fn window(&self) -> Option<AXElement> {
        let w = window_for_pid(self.pid)?;
        Some(AXElement { kind: Kind::Window(w), pid: self.pid, text: Default::default() })
    }

    /// The macOS role names `host.rs` reasons about.
    pub fn role(&self) -> Option<String> {
        let role = self.role_id()?;
        let st = self.states();
        Some(
            match role {
                ROLE_PASSWORD_TEXT => "AXSecureTextField",
                ROLE_TERMINAL => "AXTerminal",
                ROLE_FRAME | ROLE_WINDOW | ROLE_DIALOG => "AXWindow",
                ROLE_APPLICATION => "AXApplication",
                ROLE_SCROLL_PANE | ROLE_VIEWPORT => "AXScrollArea",
                ROLE_ENTRY | ROLE_TEXT if Self::has_state(st, STATE_MULTI_LINE) => "AXTextArea",
                ROLE_ENTRY => "AXTextField",
                ROLE_TEXT => "AXTextArea",
                _ => "AXGroup",
            }
            .to_string(),
        )
    }

    pub fn subrole(&self) -> Option<String> {
        None
    }

    fn full_text(&self) -> Option<String> {
        let t: String = self.call(IFACE_TEXT, "GetText", &(0i32, -1i32))?;
        if let Ok(mut g) = self.text.lock() {
            g.clone_from(&t);
        }
        Some(t)
    }

    fn cached_text(&self) -> String {
        let cached = self.text.lock().map(|g| g.clone()).unwrap_or_default();
        if cached.is_empty() {
            self.full_text().unwrap_or_default()
        } else {
            cached
        }
    }

    /// Selection (or caret) in characters.
    fn selection_chars(&self) -> Option<(usize, usize)> {
        let n: i32 = self.call(IFACE_TEXT, "GetNSelections", &()).unwrap_or(0);
        if n > 0 {
            let (s, e): (i32, i32) = self.call(IFACE_TEXT, "GetSelection", &(0i32,))?;
            if s >= 0 && e >= s {
                return Some((s as usize, e as usize));
            }
        }
        let (bus, path) = self.ids()?;
        let caret: i32 = prop(bus, path, IFACE_TEXT, "CaretOffset")?;
        (caret >= 0).then_some((caret as usize, caret as usize))
    }

    pub fn string(&self, name: &str) -> Option<String> {
        match name {
            "AXValue" => {
                let t = self.full_text()?;
                // A rich editor's text is only part of its document: not a field we can patch.
                (!t.contains(EMBEDDED_OBJECT)).then_some(t)
            }
            "AXSelectedText" => {
                let (s, e) = self.selection_chars()?;
                let t = self.cached_text();
                Some(t.chars().skip(s).take(e - s).collect())
            }
            "AXRole" => self.role(),
            "AXTitle" => {
                let (bus, path) = self.ids()?;
                prop(bus, path, IFACE_ACCESSIBLE, "Name")
            }
            _ => None,
        }
    }

    pub fn number(&self, _name: &str) -> Option<i64> {
        None
    }

    pub fn has_attr(&self, name: &str) -> bool {
        match name {
            "AXSelectedTextRange" | "AXValue" => self.interfaces().iter().any(|i| i == IFACE_TEXT),
            _ => false,
        }
    }

    /// Editable plain text: EditableText, EDITABLE, not read-only, no embedded objects.
    pub fn is_settable(&self, name: &str) -> bool {
        if !matches!(name, "AXValue" | "AXSelectedText" | "AXSelectedTextRange") {
            return false;
        }
        // A terminal or password field is never a writing surface, whatever it claims.
        if matches!(self.role_id(), Some(ROLE_TERMINAL) | Some(ROLE_PASSWORD_TEXT) | None) {
            return false;
        }
        let ifaces = self.interfaces();
        if !ifaces.iter().any(|i| i == IFACE_EDITABLE) || !ifaces.iter().any(|i| i == IFACE_TEXT) {
            return false;
        }
        let st = self.states();
        Self::has_state(st, STATE_EDITABLE) && !Self::has_state(st, STATE_READ_ONLY)
    }

    /// Whether `replace_chars` lands where it is told. Gecko (Firefox, Zen,
    /// Thunderbird) runs DeleteText and InsertText in its content process and
    /// inserts at the caret instead of the given offset, so it is pasted into.
    pub fn edits_in_place(&self) -> bool {
        self.interfaces().iter().any(|i| i == IFACE_EDITABLE) && !self.toolkit().is_some_and(|t| t.eq_ignore_ascii_case("gecko"))
    }

    /// The app's toolkit name (`GTK`, `Qt`, `Gecko`, `Chromium`), cached per bus.
    fn toolkit(&self) -> Option<String> {
        static NAMES: OnceLock<Mutex<std::collections::HashMap<String, String>>> = OnceLock::new();
        let names = NAMES.get_or_init(Default::default);
        let (bus, _) = self.ids()?;
        if let Some(t) = names.lock().ok()?.get(bus) {
            return Some(t.clone());
        }
        // Every app serves its application object at the same root path.
        let name: String = prop(bus, "/org/a11y/atspi/accessible/root", "org.a11y.atspi.Application", "ToolkitName")?;
        let mut g = names.lock().ok()?;
        if g.len() > 256 {
            g.clear();
        }
        g.insert(bus.to_string(), name.clone());
        Some(name)
    }

    /// `(location, length)` in UTF-16 units.
    pub fn range(&self, name: &str) -> Option<(usize, usize)> {
        if name != "AXSelectedTextRange" {
            return None;
        }
        let (s, e) = self.selection_chars()?;
        let t = self.cached_text();
        let us = char_to_u16(&t, s);
        Some((us, char_to_u16(&t, e) - us))
    }

    pub fn set_range(&self, name: &str, loc: usize, len: usize) -> Result<(), i32> {
        if name != "AXSelectedTextRange" {
            return Err(-1);
        }
        let t = self.full_text().ok_or(-2)?;
        let s = u16_to_char(&t, loc) as i32;
        let e = u16_to_char(&t, loc + len) as i32;
        let ok: bool = if len == 0 {
            let _: Option<bool> = self.call(IFACE_TEXT, "RemoveSelection", &(0i32,));
            self.call(IFACE_TEXT, "SetCaretOffset", &(s,)).unwrap_or(false)
        } else {
            // SetSelection(0) also works with no selection yet (GTK, Gecko, Chromium).
            // Gecko accepts AddSelection too but then pastes at the old caret.
            self.call(IFACE_TEXT, "SetSelection", &(0i32, s, e)).unwrap_or(false) || self.call(IFACE_TEXT, "AddSelection", &(s, e)).unwrap_or(false)
        };
        if ok {
            Ok(())
        } else {
            Err(-3)
        }
    }

    /// `AXSelectedText` replaces the selection; `AXValue` replaces everything.
    pub fn set_string(&self, name: &str, value: &str) -> Result<(), i32> {
        match name {
            "AXSelectedText" => {
                let (s, e) = self.selection_chars().ok_or(-2)?;
                self.replace_chars(s, e, value)
            }
            "AXValue" => {
                let ok: bool = self.call(IFACE_EDITABLE, "SetTextContents", &(value,)).unwrap_or(false);
                if ok {
                    Ok(())
                } else {
                    Err(-3)
                }
            }
            _ => Err(-1),
        }
    }

    /// Replace characters `[s, e)` with `value` through EditableText.
    pub fn replace_chars(&self, s: usize, e: usize, value: &str) -> Result<(), i32> {
        if e > s {
            let ok: bool = self.call(IFACE_EDITABLE, "DeleteText", &(s as i32, e as i32)).unwrap_or(false);
            if !ok {
                return Err(-3);
            }
        }
        if !value.is_empty() {
            let n = value.chars().count() as i32;
            let ok: bool = self.call(IFACE_EDITABLE, "InsertText", &(s as i32, value, n)).unwrap_or(false);
            if !ok {
                return Err(-4);
            }
        }
        Ok(())
    }

    fn extents_window(&self) -> Option<Rect> {
        let (x, y, w, h): (i32, i32, i32, i32) = self.call(IFACE_COMPONENT, "GetExtents", &(COORD_WINDOW,))?;
        Some(Rect { x: x as f64, y: y as f64, w: w as f64, h: h as f64 })
    }

    /// Where window-relative coordinates start on screen.
    fn origin(&self) -> Option<(f64, f64)> {
        let w = window_for_pid(self.pid)?;
        let (dx, dy) = calibration(self, &w);
        Some((w.x + dx, w.y + dy))
    }

    pub fn frame(&self) -> Option<Rect> {
        match &self.kind {
            Kind::Window(r) => Some(*r),
            Kind::System => None,
            Kind::Node { .. } => {
                let r = self.extents_window()?;
                let (ox, oy) = self.origin()?;
                Some(Rect { x: r.x + ox, y: r.y + oy, ..r })
            }
        }
    }

    /// Screen box for a UTF-16 range.
    pub fn bounds_for_range(&self, loc: usize, len: usize) -> Option<Rect> {
        let t = self.cached_text();
        let s = u16_to_char(&t, loc) as i32;
        let e = u16_to_char(&t, loc + len) as i32;
        self.bounds_chars(s, e)
    }

    fn bounds_chars(&self, s: i32, e: i32) -> Option<Rect> {
        let range = self.extents(self.call(IFACE_TEXT, "GetRangeExtents", &(s, e, self.text_coord())));
        // GTK4 answers a range with its first character's box only; the last
        // character's box restores the width (a no-op where the range box is right).
        let last = (e - 1 > s).then(|| self.extents(self.call(IFACE_TEXT, "GetCharacterExtents", &(e - 1, self.text_coord())))).flatten();
        let r = match (range, last) {
            (Some(a), Some(b)) if (a.y - b.y).abs() < a.h.max(b.h) => a.union(&b),
            (Some(a), _) => a,
            (None, b) => b?,
        };
        let (ox, oy) = self.origin()?;
        Some(Rect { x: r.x + ox, y: r.y + oy, ..r })
    }

    /// The coordinate type whose text boxes share the field's window-relative
    /// frame. GTK answers the same in both. Chromium counts WINDOW text boxes
    /// from the web page (below its toolbar) but SCREEN ones, on Wayland, from
    /// the window. Decided once per field: the caret's box must lie in the field.
    fn text_coord(&self) -> u32 {
        static CHOSEN: OnceLock<Mutex<std::collections::HashMap<u64, u32>>> = OnceLock::new();
        let chosen = CHOSEN.get_or_init(Default::default);
        let key = self.identity_hash();
        if let Some(c) = chosen.lock().ok().and_then(|g| g.get(&key).copied()) {
            return c;
        }
        let Some(frame) = self.extents_window() else { return COORD_WINDOW };
        let caret = self.ids().and_then(|(b, p)| prop::<i32>(b, p, IFACE_TEXT, "CaretOffset")).unwrap_or(0).max(0);
        let fits = |coord: u32| {
            self.extents(self.call(IFACE_TEXT, "GetCharacterExtents", &(caret, coord)))
                .map(|r| frame_holds(&frame, &r))
        };
        let pick = match (fits(COORD_WINDOW), fits(COORD_SCREEN)) {
            (Some(true), _) => COORD_WINDOW,
            (_, Some(true)) => COORD_SCREEN,
            // Caret off screen or at an empty line end: decide on a later read.
            _ => return COORD_WINDOW,
        };
        if let Ok(mut g) = chosen.lock() {
            if g.len() > 256 {
                g.clear();
            }
            g.insert(key, pick);
        }
        pick
    }

    fn extents(&self, v: Option<(i32, i32, i32, i32)>) -> Option<Rect> {
        let (x, y, w, h) = v?;
        (w > 0 && h > 0).then_some(Rect { x: x as f64, y: y as f64, w: w as f64, h: h as f64 })
    }

    /// `(start, end)` in characters of the visual line holding character `off`.
    fn line_at(&self, off: i32) -> Option<(i32, i32)> {
        let (_, s, e): (String, i32, i32) = self.call(IFACE_TEXT, "GetStringAtOffset", &(off, GRANULARITY_LINE))?;
        (e > s).then_some((s, e))
    }

    /// Screen rects (one per visual line) for a UTF-16 range. `GetRangeExtents`
    /// over a wrapped range answers one box around every line, so split first.
    pub fn rects_for_range(&self, loc: usize, len: usize) -> Vec<Rect> {
        let t = self.cached_text();
        let s = u16_to_char(&t, loc) as i32;
        let e = u16_to_char(&t, loc + len) as i32;
        let mut out = Vec::new();
        let mut cur = s;
        for _ in 0..40 {
            if cur >= e {
                break;
            }
            let (ls, le) = self.line_at(cur).unwrap_or((s, e));
            let (a, b) = (cur.max(ls), e.min(le));
            if b > a {
                if let Some(r) = self.bounds_chars(a, trim_line_end(&t, a, b)) {
                    out.push(r);
                }
            }
            if le <= cur {
                break;
            }
            cur = le;
        }
        out
    }

    /// The UTF-16 range of the text showing inside `vis` (screen coordinates),
    /// widened by `VISIBLE_SLACK` characters each way; `None` when the toolkit
    /// does not answer (then every alert is placed). Corners can sit in a
    /// margin or below the last line, so a few points step inwards.
    pub fn visible_text_range(&self, vis: &Rect) -> Option<(usize, usize)> {
        let (ox, oy) = self.origin()?;
        let coord = self.text_coord();
        let at = |x: f64, y: f64| -> Option<i32> {
            let o: i32 = self.call(IFACE_TEXT, "GetOffsetAtPoint", &((x - ox).round() as i32, (y - oy).round() as i32, coord))?;
            (o >= 0).then_some(o)
        };
        let xs = [vis.x + 4.0, vis.x + vis.w / 2.0, vis.x + vis.w - 4.0];
        let insets = [4.0, 24.0, 48.0, vis.h / 4.0];
        let first = insets.iter().find_map(|dy| xs.iter().find_map(|&x| at(x, vis.y + dy)))?;
        let t = self.cached_text();
        let chars = t.chars().count();
        // Nothing under the bottom corners: the text ends inside the field.
        let last = insets.iter().find_map(|dy| xs.iter().rev().find_map(|&x| at(x, vis.y + vis.h - dy))).map_or(chars, |o| o as usize);
        let s = (first as usize).saturating_sub(VISIBLE_SLACK);
        let e = (last + VISIBLE_SLACK).min(chars);
        (e >= s).then(|| (char_to_u16(&t, s), char_to_u16(&t, e)))
    }

    pub fn line_for_index(&self, _idx: usize) -> Option<i64> {
        None
    }

    pub fn range_for_line(&self, _line: i64) -> Option<(usize, usize)> {
        None
    }

    /// The nearest ancestor scroll pane, if any (bounded walk).
    pub fn enclosing_scroll_area(&self) -> Option<AXElement> {
        let mut cur = self.parent()?;
        for _ in 0..8 {
            if matches!(cur.role_id(), Some(ROLE_SCROLL_PANE) | Some(ROLE_VIEWPORT)) {
                return Some(cur);
            }
            cur = cur.parent()?;
        }
        None
    }

    /// The top-level accessible (frame/window/dialog) above this element.
    fn toplevel(&self) -> Option<AXElement> {
        let mut cur = self.parent()?;
        for _ in 0..64 {
            if matches!(cur.role_id(), Some(ROLE_FRAME) | Some(ROLE_WINDOW) | Some(ROLE_DIALOG)) {
                return Some(cur);
            }
            cur = cur.parent()?;
        }
        None
    }
}

/// Drop a trailing newline from a line range so the underline stops at the last glyph.
fn trim_line_end(text: &str, a: i32, b: i32) -> i32 {
    let last = text.chars().nth((b - 1).max(0) as usize);
    if b - 1 > a && matches!(last, Some('\n') | Some('\r')) {
        b - 1
    } else {
        b
    }
}

/// The on-screen rect of the window that owns `pid`, when the desktop says
/// where windows are (Hyprland, X11). Without it there are no underlines.
fn window_for_pid(pid: i32) -> Option<Rect> {
    desktop::front().filter(|f| f.pid == pid)?.rect
}

/// The app that last took accessibility focus: off Hyprland, the app in front.
pub(super) struct FocusOwner {
    pub bus: String,
    pub pid: i32,
    pub app: String,
    /// Its window in screen coordinates (meaningful on X11 only).
    pub screen_rect: Option<Rect>,
}

pub(super) fn focus_owner() -> Option<FocusOwner> {
    let (bus, path) = focus::CURRENT.lock().ok()?.clone()?;
    let pid = bus_pid(&bus)?;
    if pid == std::process::id() as i32 {
        return None;
    }
    let app: String = prop(&bus, "/org/a11y/atspi/accessible/root", IFACE_ACCESSIBLE, "Name").unwrap_or_default();
    let el = AXElement::node(bus.clone(), path, pid);
    let screen_rect = el.toplevel().and_then(|t| t.extents(t.call(IFACE_COMPONENT, "GetExtents", &(COORD_SCREEN,))));
    Some(FocusOwner { bus, pid, app, screen_rect })
}

/// Offset from Hyprland's window corner to the corner AT-SPI's window
/// coordinates start at. Toolkits that draw their own shadow (GTK with
/// client-side decorations) count from the shadow's corner, which sits outside
/// the window Hyprland reports; the top-level accessible's window-relative box
/// says by how much. Cached per window.
fn calibration(el: &AXElement, win: &Rect) -> (f64, f64) {
    static CACHE: Mutex<Option<(u64, (f64, f64))>> = Mutex::new(None);
    let key = {
        let mut h = DefaultHasher::new();
        (el.pid, win.w as i64, win.h as i64).hash(&mut h);
        h.finish()
    };
    if let Ok(g) = CACHE.lock() {
        if let Some((k, v)) = *g {
            if k == key {
                return v;
            }
        }
    }
    let off = el.toplevel().and_then(|t| t.extents_window()).map(|f| frame_offset(&f, win)).unwrap_or((0.0, 0.0));
    if let Ok(mut g) = CACHE.lock() {
        *g = Some((key, off));
    }
    off
}

/// A character box inside a field's box, with a few pixels of slack for borders.
fn frame_holds(frame: &Rect, ch: &Rect) -> bool {
    const SLACK: f64 = 4.0;
    ch.x >= frame.x - SLACK && ch.y >= frame.y - SLACK && ch.x + ch.w <= frame.x + frame.w + SLACK && ch.y + ch.h <= frame.y + frame.h + SLACK
}

/// Given the top-level's window-relative box and the window Hyprland placed:
/// a box at a positive offset means the content starts inside a shadow margin.
fn frame_offset(frame: &Rect, win: &Rect) -> (f64, f64) {
    // A frame that already matches the window (no CSD shadow) needs no shift.
    let dx = if frame.x > 0.0 && frame.w <= win.w + 1.0 { -frame.x } else { 0.0 };
    let dy = if frame.y > 0.0 && frame.h <= win.h + 1.0 { -frame.y } else { 0.0 };
    (dx, dy)
}

/// Frontmost application as `(pid, app id, title)`. On Linux the app id is the
/// Wayland app id / X11 class, which plays the bundle id's role.
pub fn frontmost_app() -> Option<(i32, String, String)> {
    let w = desktop::front()?;
    let name = if w.title.is_empty() { w.class.clone() } else { app_name(&w.class) };
    Some((w.pid, w.class, name))
}

/// A readable app name from an app id (`org.gnome.TextEditor` -> `TextEditor`).
fn app_name(class: &str) -> String {
    class.rsplit('.').next().filter(|s| !s.is_empty()).unwrap_or(class).to_string()
}

/// Bring another app to the front (needed before a paste-based replacement).
pub fn activate_app(pid: i32) -> bool {
    desktop::focus_pid(pid)
}

/// Current pointer location in layout coordinates (logical points).
pub fn mouse_location_top_left() -> Option<(f64, f64)> {
    desktop::pointer()
}

/// Height of the main display in logical points.
pub fn primary_screen_height() -> Option<f64> {
    desktop::primary_monitor().map(|m| m.3)
}

/// Move the pointer (debug probe only: lets the e2e harness hover an underline).
pub fn post_mouse_move(x: f64, y: f64) -> bool {
    desktop::move_pointer(x, y)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn utf16_and_char_offsets_round_trip() {
        let t = "a😀b é tiếng Việt 中文";
        for (i, _) in t.chars().enumerate() {
            assert_eq!(u16_to_char(t, char_to_u16(t, i)), i);
        }
        // 😀 is two UTF-16 units: offsets 1 and 2 both fall inside it.
        assert_eq!(u16_to_char(t, 1), 1);
        assert_eq!(u16_to_char(t, 2), 1);
        assert_eq!(u16_to_char(t, 3), 2);
        assert_eq!(char_to_u16(t, 2), 3);
        // Past the end clamps.
        assert_eq!(u16_to_char(t, 999), t.chars().count());
        assert_eq!(char_to_u16(t, 999), t.encode_utf16().count());
    }

    #[test]
    fn combining_marks_count_as_their_own_characters() {
        // "e" + COMBINING ACUTE: two code points, two UTF-16 units.
        let t = "cafe\u{301} ok";
        assert_eq!(char_to_u16(t, 5), 5);
        assert_eq!(u16_to_char(t, 5), 5);
    }

    #[test]
    fn line_end_trims_newline_only() {
        let t = "one\ntwo";
        assert_eq!(trim_line_end(t, 0, 4), 3);
        assert_eq!(trim_line_end(t, 4, 7), 7);
        assert_eq!(trim_line_end("\n", 0, 1), 1, "a lone newline keeps its box");
    }

    #[test]
    fn csd_shadow_shifts_the_origin() {
        let win = Rect { x: 100.0, y: 50.0, w: 800.0, h: 600.0 };
        assert_eq!(frame_offset(&Rect { x: 0.0, y: 0.0, w: 800.0, h: 600.0 }, &win), (0.0, 0.0));
        assert_eq!(frame_offset(&Rect { x: 23.0, y: 15.0, w: 800.0, h: 600.0 }, &win), (-23.0, -15.0));
        assert_eq!(frame_offset(&Rect { x: 5.0, y: 5.0, w: 900.0, h: 600.0 }, &win), (0.0, -5.0));
    }

    #[test]
    fn state_changed_body_parses_both_signatures() {
        use std::collections::HashMap;
        use zbus::zvariant::Value;
        // at-spi2-core 2.48+: (siiva{sv}); older: (siiv(so)).
        let new = zbus::Message::signal("/org/a11y/atspi/accessible/1", "org.a11y.atspi.Event.Object", "StateChanged")
            .unwrap()
            .build(&("focused", 1i32, 0i32, Value::from(0i32), HashMap::<String, Value>::new()))
            .unwrap();
        assert_eq!(focus::gained(&new), Some(true));
        let old = zbus::Message::signal("/org/a11y/atspi/accessible/1", "org.a11y.atspi.Event.Object", "StateChanged")
            .unwrap()
            .build(&("focused", 0i32, 0i32, Value::from(0i32), (":1.5", zbus::zvariant::ObjectPath::try_from("/x").unwrap())))
            .unwrap();
        assert_eq!(focus::gained(&old), Some(false));
    }

    #[test]
    fn union_spans_first_and_last_character() {
        let first = Rect { x: 30.0, y: 14.0, w: 9.0, h: 19.0 };
        let last = Rect { x: 48.0, y: 14.0, w: 9.0, h: 19.0 };
        assert_eq!(first.union(&last), Rect { x: 30.0, y: 14.0, w: 27.0, h: 19.0 });
    }

    #[test]
    fn chromium_page_relative_box_is_outside_the_field() {
        // Values read from Chromium 156 on Wayland: the textarea at (8,96), the
        // caret box in WINDOW coords (page-relative) and in SCREEN coords.
        let field = Rect { x: 8.0, y: 96.0, w: 706.0, h: 206.0 };
        assert!(!frame_holds(&field, &Rect { x: 41.0, y: 11.0, w: 9.0, h: 20.0 }));
        assert!(frame_holds(&field, &Rect { x: 41.0, y: 99.0, w: 9.0, h: 20.0 }));
    }

    #[test]
    fn app_names_drop_reverse_dns() {
        assert_eq!(app_name("org.gnome.TextEditor"), "TextEditor");
        assert_eq!(app_name("firefox"), "firefox");
    }
}
