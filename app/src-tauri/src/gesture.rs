//! The system-wide wheel — no Accessibility permission needed.
//!
//! Two ways to open it while Finder is in front:
//!   * HOLD: keep ⇧ / ⌘ / ⌃ pressed for a moment (no other key) → the wheel opens
//!     at the cursor with the options for Finder's current selection. Because it
//!     is on screen *before* a drag starts, Finder's drag enters it normally
//!     (`tauri://drag-enter/over/drop`) — a window shown mid-drag is not accepted
//!     as a drop target by macOS, which is why the first spike showed no options.
//!   * DRAG: press on a file, hold a modifier and move → same wheel, same files.
//!
//! Files always come from Finder's selection (JXA via osascript; the item under
//! the mouse is selected the moment the button goes down). The wheel's centre is
//! transparent, so a press there goes to Finder and the file can be dragged onto a
//! slice; slices themselves are clickable. Cursor, button and modifier state are
//! read from CoreGraphics' permission-free session state at 60 Hz.

use serde::Serialize;
use std::ffi::c_void;
use std::sync::atomic::{AtomicBool, AtomicU64, Ordering};
use std::thread;
use std::time::{Duration, Instant};
use tauri::{AppHandle, Emitter, LogicalPosition, Manager, WebviewUrl, WebviewWindowBuilder};

pub const WHEEL_LABEL: &str = "wheel";
/// Overlay window size in points (the wheel is drawn centred in it).
pub const SIZE: f64 = 420.0;
/// Ring geometry in points — must match `overlay.js` (R, r).
const RING_OUTER: f64 = 172.0;
const RING_INNER: f64 = 70.0;
/// AppKit's usual drag threshold.
const THRESHOLD: f64 = 4.0;
/// Hold a modifier this long (no other key) to open the wheel.
const HOLD: Duration = Duration::from_millis(500);
/// After the modifier is released, keep the wheel this long (time to grab the file).
const DISMISS_GRACE: Duration = Duration::from_millis(900);
/// If the webview never asks to be hidden (a JS error), hide anyway.
const SAFETY_HIDE: Duration = Duration::from_secs(20);

fn log(msg: &str) {
    eprintln!("[gesture] {msg}");
}

// ---------- permission-free CoreGraphics session state ----------

#[repr(C)]
#[derive(Clone, Copy)]
struct CGPoint {
    x: f64,
    y: f64,
}

#[link(name = "CoreGraphics", kind = "framework")]
extern "C" {
    fn CGEventCreate(source: *const c_void) -> *mut c_void;
    fn CGEventGetLocation(event: *const c_void) -> CGPoint;
    fn CGEventSourceButtonState(state_id: i32, button: u32) -> bool;
    fn CGEventSourceFlagsState(state_id: i32) -> u64;
    fn CGEventSourceKeyState(state_id: i32, key: u16) -> bool;
}
#[link(name = "CoreFoundation", kind = "framework")]
extern "C" {
    fn CFRelease(cf: *const c_void);
}

const COMBINED_SESSION_STATE: i32 = 0;
const LEFT_BUTTON: u32 = 0;
const FLAG_SHIFT: u64 = 1 << 17;
const FLAG_CONTROL: u64 = 1 << 18;
const FLAG_COMMAND: u64 = 1 << 20;
/// Modifier / function key codes that don't count as "typing".
const MODIFIER_KEYS: [u16; 10] = [54, 55, 56, 57, 58, 59, 60, 61, 62, 63];

struct Sample {
    x: f64,
    y: f64,
    left_down: bool,
    flags: u64,
}

/// Cursor (global, top-left origin, points), left button and modifier state.
fn sample() -> Option<Sample> {
    unsafe {
        let ev = CGEventCreate(std::ptr::null());
        if ev.is_null() {
            return None;
        }
        let p = CGEventGetLocation(ev);
        CFRelease(ev);
        Some(Sample {
            x: p.x,
            y: p.y,
            left_down: CGEventSourceButtonState(COMBINED_SESSION_STATE, LEFT_BUTTON),
            flags: CGEventSourceFlagsState(COMBINED_SESSION_STATE),
        })
    }
}

/// Is any ordinary (non-modifier) key down? Used to tell "holding ⇧" from typing.
fn typing() -> bool {
    (0u16..127).any(|k| !MODIFIER_KEYS.contains(&k) && unsafe { CGEventSourceKeyState(COMBINED_SESSION_STATE, k) })
}

// ---------- files: Finder's selection ----------

/// What Finder has selected right now. Needs the one-time Automation consent
/// ("Flick wants to control Finder"); takes ~0.2–0.8 s because osascript starts.
fn finder_selection() -> Vec<String> {
    const SCRIPT: &str = "ObjC.import('Foundation');\
        const f = Application('com.apple.finder');\
        f.selection().map(i => ObjC.unwrap($.NSURL.URLWithString(i.url()).path)).join('\\n');";
    match std::process::Command::new("/usr/bin/osascript")
        .args(["-l", "JavaScript", "-e", SCRIPT])
        .output()
    {
        Ok(o) if o.status.success() => String::from_utf8_lossy(&o.stdout)
            .lines()
            .map(|l| l.trim().to_string())
            .filter(|l| !l.is_empty() && std::path::Path::new(l).exists())
            .collect(),
        Ok(o) => {
            log(&format!("finder selection failed: {}", String::from_utf8_lossy(&o.stderr).trim()));
            Vec::new()
        }
        Err(e) => {
            log(&format!("finder selection could not run osascript: {e}"));
            Vec::new()
        }
    }
}

/// Only react while Finder is in front.
fn finder_is_frontmost() -> bool {
    use objc2_app_kit::NSWorkspace;
    #[allow(unused_unsafe)]
    unsafe {
        NSWorkspace::sharedWorkspace()
            .frontmostApplication()
            .and_then(|app| app.bundleIdentifier())
            .map(|id| id.to_string() == "com.apple.finder")
            .unwrap_or(false)
    }
}

// ---------- modes ----------

#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize)]
#[serde(rename_all = "lowercase")]
pub enum Mode {
    Convert,
    Tools,
    Actions,
}

/// ⌘ → Tools, ⌃ → Actions, ⇧ → Convert (checked in that order so ⇧⌘ is Tools).
fn mode_of(flags: u64) -> Option<Mode> {
    if flags & FLAG_COMMAND != 0 {
        Some(Mode::Tools)
    } else if flags & FLAG_CONTROL != 0 {
        Some(Mode::Actions)
    } else if flags & FLAG_SHIFT != 0 {
        Some(Mode::Convert)
    } else {
        None
    }
}

// ---------- overlay window ----------

#[derive(Serialize, Clone)]
struct WheelEvent {
    state: &'static str,
    #[serde(skip_serializing_if = "Option::is_none")]
    mode: Option<Mode>,
    #[serde(skip_serializing_if = "Option::is_none")]
    x: Option<f64>,
    #[serde(skip_serializing_if = "Option::is_none")]
    y: Option<f64>,
    #[serde(skip_serializing_if = "Option::is_none")]
    paths: Option<Vec<String>>,
}

fn ev(state: &'static str) -> WheelEvent {
    WheelEvent { state, mode: None, x: None, y: None, paths: None }
}

fn emit(app: &AppHandle, e: WheelEvent) {
    if let Some(w) = app.get_webview_window(WHEEL_LABEL) {
        let _ = w.emit("wheel", e);
    }
}

static WHEEL_OPEN: AtomicBool = AtomicBool::new(false);
static FILES_SENT: AtomicBool = AtomicBool::new(false);
/// Incremented on every opening, so late results from a previous one are dropped.
static OPEN_GEN: AtomicU64 = AtomicU64::new(0);

/// Deliver files to the wheel unless this opening already has them.
fn deliver_files(app: &AppHandle, gen: u64, source: &str, files: Vec<String>) -> bool {
    if OPEN_GEN.load(Ordering::SeqCst) != gen || !WHEEL_OPEN.load(Ordering::SeqCst) {
        return false;
    }
    if files.is_empty() {
        emit(app, ev("nofiles"));
        return false;
    }
    if FILES_SENT.swap(true, Ordering::SeqCst) {
        return false;
    }
    log(&format!("files from {source}: {files:?}"));
    let mut e = ev("files");
    e.paths = Some(files);
    emit(app, e);
    true
}

/// Create the hidden, transparent overlay once at startup.
pub fn create_overlay(app: &AppHandle) -> tauri::Result<()> {
    WebviewWindowBuilder::new(app, WHEEL_LABEL, WebviewUrl::App("overlay.html".into()))
        .title("Flick wheel")
        .inner_size(SIZE, SIZE)
        .decorations(false)
        .transparent(true)
        .shadow(false)
        .resizable(false)
        .always_on_top(true)
        .visible_on_all_workspaces(true)
        .skip_taskbar(true)
        .focusable(false)
        .focused(false)
        .accept_first_mouse(true)
        .visible(false)
        .build()?;
    Ok(())
}

/// Place, show and raise the overlay — all on the main thread, in one step, so
/// a hide queued right after can never be overtaken by a stale raise.
fn open_wheel(app: &AppHandle, x: f64, y: f64, mode: Mode) {
    WHEEL_OPEN.store(true, Ordering::SeqCst);
    let mut e = ev("open");
    e.mode = Some(mode);
    emit(app, e);
    let h = app.clone();
    let _ = app.run_on_main_thread(move || {
        if let Some(w) = h.get_webview_window(WHEEL_LABEL) {
            let _ = w.set_position(LogicalPosition::new(x - SIZE / 2.0, y - SIZE / 2.0));
            let _ = w.show();
            if let Ok(ptr) = w.ns_window() {
                use objc2_app_kit::{NSWindow, NSWindowCollectionBehavior};
                #[allow(unused_unsafe)]
                unsafe {
                    let win: &NSWindow = &*ptr.cast::<NSWindow>();
                    win.setCollectionBehavior(
                        NSWindowCollectionBehavior::CanJoinAllSpaces
                            | NSWindowCollectionBehavior::FullScreenAuxiliary
                            | NSWindowCollectionBehavior::Transient,
                    );
                    // The cursor starts in the transparent centre: let presses there reach
                    // Finder (grab the file). `set_click_through` flips this as the cursor
                    // moves onto the ring, so slices take clicks — a WebView-backed window
                    // does not fall through its transparent pixels by itself.
                    win.setIgnoresMouseEvents(true);
                    win.orderFrontRegardless();
                }
            }
        }
    });
}

/// Make the overlay transparent to the mouse (true) or clickable (false).
fn set_click_through(app: &AppHandle, ignore: bool) {
    let h = app.clone();
    let _ = app.run_on_main_thread(move || {
        if let Some(w) = h.get_webview_window(WHEEL_LABEL) {
            if let Ok(ptr) = w.ns_window() {
                use objc2_app_kit::NSWindow;
                #[allow(unused_unsafe)]
                unsafe {
                    let win: &NSWindow = &*ptr.cast::<NSWindow>();
                    win.setIgnoresMouseEvents(ignore);
                }
            }
        }
    });
}

fn close_wheel(app: &AppHandle) {
    WHEEL_OPEN.store(false, Ordering::SeqCst);
    let h = app.clone();
    let _ = app.run_on_main_thread(move || {
        if let Some(w) = h.get_webview_window(WHEEL_LABEL) {
            let _ = w.emit("wheel", ev("close"));
            let _ = w.hide();
        }
    });
}

/// The webview got the files from the OS drag.
#[tauri::command]
pub fn files_received() {
    FILES_SENT.store(true, Ordering::SeqCst);
}

/// Kept for compatibility; a no-op now.
#[tauri::command]
pub fn drop_received() {}

/// The webview is done (result shown, or nothing to do); hide the overlay.
#[tauri::command]
pub fn hide_wheel(app: AppHandle) {
    close_wheel(&app);
}

fn open_and_fetch(app: &AppHandle, x: f64, y: f64, mode: Mode, why: &str) {
    let gen = OPEN_GEN.fetch_add(1, Ordering::SeqCst) + 1;
    FILES_SENT.store(false, Ordering::SeqCst);
    log(&format!("open #{gen} ({why}) at ({x:.0},{y:.0}) mode={mode:?}"));
    open_wheel(app, x, y, mode);
    let app2 = app.clone();
    thread::spawn(move || {
        let files = finder_selection();
        if !deliver_files(&app2, gen, "finder selection", files) {
            log("finder selection: nothing delivered (empty, or wheel already closed)");
        }
    });
}

/// Start the 60 Hz poll on a background thread.
pub fn start(app: AppHandle) {
    thread::spawn(move || {
        let mut prev_down = false;
        let mut press_at: Option<(f64, f64)> = None; // where the button went down
        let mut drag_rejected = false;
        let mut hold_since: Option<Instant> = None;
        let mut mode_held: Option<Mode> = None;
        let mut origin: Option<(f64, f64)> = None; // overlay top-left while open
        let mut last_cursor: Option<(i64, i64)> = None;
        let mut dismiss_at: Option<Instant> = None;
        let mut safety_at: Option<Instant> = None;
        let mut over_ring: Option<bool> = None; // last click-through state sent
        log("poll thread started");
        loop {
            if let Some(s) = sample() {
                let mode = mode_of(s.flags);
                let open = WHEEL_OPEN.load(Ordering::SeqCst);
                if !open {
                    origin = None;
                }

                // --- button edges ---
                if s.left_down && !prev_down {
                    press_at = Some((s.x, s.y));
                    drag_rejected = false;
                    dismiss_at = None; // interacting (grabbing the file / clicking a slice)
                }
                if !s.left_down && prev_down {
                    press_at = None;
                    if open {
                        if let Some((ox, oy)) = origin {
                            let mut e = ev("release");
                            e.x = Some(s.x - ox);
                            e.y = Some(s.y - oy);
                            emit(&app, e);
                            log(&format!("release at ({:.0},{:.0}) rel", s.x - ox, s.y - oy));
                        }
                        if mode.is_none() {
                            dismiss_at = Some(Instant::now() + DISMISS_GRACE);
                        }
                    }
                }
                prev_down = s.left_down;

                // --- trigger DRAG: modifier + moved past threshold with the button down ---
                if !open {
                    if let (Some(m), Some((px, py)), false) = (mode, press_at, drag_rejected) {
                        let moved = (s.x - px).abs() > THRESHOLD || (s.y - py).abs() > THRESHOLD;
                        if moved {
                            if finder_is_frontmost() {
                                origin = Some((s.x - SIZE / 2.0, s.y - SIZE / 2.0));
                                last_cursor = None;
                                dismiss_at = None;
                                safety_at = Some(Instant::now() + SAFETY_HIDE);
                                open_and_fetch(&app, s.x, s.y, m, "drag");
                            } else {
                                drag_rejected = true;
                            }
                        }
                    }
                }

                // --- trigger HOLD: modifier held, nothing typed, button up ---
                if !open && !s.left_down {
                    match mode {
                        Some(m) => {
                            if typing() {
                                hold_since = None;
                            } else if hold_since.is_none() {
                                hold_since = Some(Instant::now());
                                mode_held = Some(m);
                            } else if let Some(t) = hold_since {
                                if t.elapsed() >= HOLD && finder_is_frontmost() {
                                    hold_since = None;
                                    origin = Some((s.x - SIZE / 2.0, s.y - SIZE / 2.0));
                                    last_cursor = None;
                                    dismiss_at = None;
                                    safety_at = Some(Instant::now() + SAFETY_HIDE);
                                    open_and_fetch(&app, s.x, s.y, mode_held.unwrap_or(m), "hold");
                                }
                            }
                        }
                        None => hold_since = None,
                    }
                } else if mode.is_none() {
                    hold_since = None;
                }

                // --- while open ---
                if open {
                    // Switching the held key swaps the wheel.
                    if let Some(m) = mode {
                        if mode_held != Some(m) {
                            mode_held = Some(m);
                            let mut e = ev("mode");
                            e.mode = Some(m);
                            emit(&app, e);
                        }
                        dismiss_at = None;
                    } else if dismiss_at.is_none() && !s.left_down {
                        // Modifier released with nothing in hand: close after a grace period.
                        dismiss_at = Some(Instant::now() + DISMISS_GRACE);
                    }
                    if let Some((ox, oy)) = origin {
                        let cur = ((s.x - ox).round() as i64, (s.y - oy).round() as i64);
                        if last_cursor != Some(cur) {
                            last_cursor = Some(cur);
                            let mut e = ev("cursor");
                            e.x = Some(cur.0 as f64);
                            e.y = Some(cur.1 as f64);
                            emit(&app, e);
                            // Clickable only while the cursor is over the ring of slices.
                            let dx = cur.0 as f64 - SIZE / 2.0;
                            let dy = cur.1 as f64 - SIZE / 2.0;
                            let dist = (dx * dx + dy * dy).sqrt();
                            let on_ring = dist >= RING_INNER - 6.0 && dist <= RING_OUTER + 24.0;
                            if over_ring != Some(on_ring) {
                                over_ring = Some(on_ring);
                                set_click_through(&app, !on_ring);
                            }
                        }
                    }
                    if let Some(d) = dismiss_at {
                        if Instant::now() >= d && !s.left_down {
                            dismiss_at = None;
                            emit(&app, ev("dismiss"));
                            log("dismiss (modifier released)");
                        }
                    }
                    if let Some(d) = safety_at {
                        if Instant::now() >= d {
                            safety_at = None;
                            log("safety hide");
                            close_wheel(&app);
                        }
                    }
                } else {
                    dismiss_at = None;
                    safety_at = None;
                    over_ring = None;
                }
            }
            thread::sleep(Duration::from_millis(16));
        }
    });
}
