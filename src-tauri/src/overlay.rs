use crate::metrics::{self, Metrics, MonitorRect};
use serde::{Deserialize, Serialize};
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::Mutex;
use std::time::{Duration, Instant};
use tauri::{AppHandle, Emitter, Manager, PhysicalPosition, PhysicalSize, WebviewWindow};

fn default_true() -> bool {
    true
}
fn default_interval() -> u32 {
    3000
}
fn default_opacity() -> u8 {
    88
}
fn default_position() -> String {
    "top-right".into()
}
fn default_notch_position() -> String {
    "top-center".into()
}

#[derive(Serialize, Deserialize, Clone)]
pub struct OverlayConfig {
    #[serde(default = "default_true")]
    pub enabled: bool,
    /// Afficher aussi hors mode jeu.
    #[serde(default)]
    pub always: bool,
    #[serde(default)]
    pub monitor_name: Option<String>,
    #[serde(default)]
    pub game_monitor_name: Option<String>,
    #[serde(default = "default_true")]
    pub show_cpu: bool,
    #[serde(default = "default_true")]
    pub show_gpu: bool,
    #[serde(default = "default_true")]
    pub show_temps: bool,
    #[serde(default = "default_true")]
    pub show_fps: bool,
    #[serde(default = "default_true")]
    pub show_ram: bool,
    /// Capsule type Notch Nook quand l’app est réduite (pas pendant un jeu).
    #[serde(default = "default_true")]
    pub notch: bool,
    #[serde(default = "default_notch_position")]
    pub notch_position: String,
    #[serde(default)]
    pub notch_monitor_name: Option<String>,
    /// Une capsule sur chaque écran. Les fenêtres en plus n’existent que tant que la notch est visible.
    #[serde(default)]
    pub notch_all_screens: bool,
    /// Capsule réduite à l’avatar. Un clic la rouvre.
    #[serde(default)]
    pub notch_compact: bool,
    #[serde(default = "default_true")]
    pub notch_show_ram: bool,
    #[serde(default = "default_true")]
    pub notch_show_cpu: bool,
    #[serde(default = "default_true")]
    pub notch_show_media: bool,
    /// Visage de la notch. Off : aucune mascotte, même en mode réduit.
    #[serde(default = "default_true")]
    pub notch_show_mascot: bool,
    #[serde(default = "default_opacity")]
    pub opacity: u8,
    #[serde(default = "default_position")]
    pub position: String,
    #[serde(default = "default_interval")]
    pub interval_ms: u32,
    /// Masquage manuel temporaire (tray).
    #[serde(default)]
    pub hidden: bool,
}

impl Default for OverlayConfig {
    fn default() -> Self {
        Self {
            enabled: true,
            always: false,
            monitor_name: None,
            game_monitor_name: None,
            show_cpu: true,
            show_gpu: true,
            show_temps: true,
            show_fps: true,
            show_ram: true,
            notch: true,
            notch_position: "top-center".into(),
            notch_monitor_name: None,
            notch_all_screens: false,
            notch_compact: false,
            notch_show_ram: true,
            notch_show_cpu: true,
            notch_show_media: true,
            notch_show_mascot: true,
            opacity: 88,
            position: "top-right".into(),
            interval_ms: 3000,
            hidden: false,
        }
    }
}

/// Config overlay + état session (non persisté).
#[derive(Serialize, Clone)]
pub struct OverlayPayload {
    #[serde(flatten)]
    pub config: OverlayConfig,
    pub session_active: bool,
    pub notch_visible: bool,
    pub music_row: bool,
    pub mixer_row: bool,
}

static MAIN_BG: AtomicBool = AtomicBool::new(false);
static NOTCH_SNOOZE: AtomicBool = AtomicBool::new(false);
static MAIN_QUIET: AtomicBool = AtomicBool::new(false);
static OVERLAY_QUIET: AtomicBool = AtomicBool::new(false);
/// Masque les overlay le temps du menu tray / aperçu barre des tâches.
static TRAY_YIELD: AtomicBool = AtomicBool::new(false);

/// true si l’état a changé. Endort aussi le WebView principal : WebView2
/// ne signale pas toujours `document.hidden` quand la fenêtre est minimisée.
pub fn set_main_background(app: &AppHandle, bg: bool) -> bool {
    let changed = MAIN_BG.swap(bg, Ordering::SeqCst) != bg;
    if changed {
        if let Some(win) = app.get_webview_window("main") {
            set_page_quiet(&win, bg);
        }
    }
    changed
}

fn set_page_quiet(win: &WebviewWindow, quiet: bool) {
    let label = win.label();
    if label != "overlay" && label != "main" {
        let _ = win.eval(quiet_js(quiet));
        return;
    }
    let flag = if label == "overlay" {
        &OVERLAY_QUIET
    } else {
        &MAIN_QUIET
    };
    if flag.swap(quiet, Ordering::SeqCst) == quiet {
        return;
    }
    let _ = win.eval(quiet_js(quiet));
}

fn quiet_js(quiet: bool) -> &'static str {
    if quiet {
        "document.documentElement.dataset.quiet='1'"
    } else {
        "delete document.documentElement.dataset.quiet"
    }
}

pub fn snooze_notch() {
    NOTCH_SNOOZE.store(true, Ordering::SeqCst);
}

pub fn clear_notch_snooze() {
    NOTCH_SNOOZE.store(false, Ordering::SeqCst);
}

static MUSIC_ROW: AtomicBool = AtomicBool::new(false);
static MIXER_ROW: AtomicBool = AtomicBool::new(false);

pub fn set_music_row(open: bool) {
    MUSIC_ROW.store(open, Ordering::SeqCst);
    if open {
        MIXER_ROW.store(false, Ordering::SeqCst);
    }
}

pub fn music_row() -> bool {
    MUSIC_ROW.load(Ordering::Relaxed)
}

pub fn set_mixer_row(open: bool) {
    MIXER_ROW.store(open, Ordering::SeqCst);
    if open {
        MUSIC_ROW.store(false, Ordering::SeqCst);
    }
}

pub fn mixer_row() -> bool {
    MIXER_ROW.load(Ordering::Relaxed)
}

fn main_is_background(app: &AppHandle) -> bool {
    if MAIN_BG.load(Ordering::Relaxed) {
        return true;
    }
    let Some(w) = app.get_webview_window("main") else {
        return true;
    };
    w.is_minimized().unwrap_or(false) || !w.is_visible().unwrap_or(true)
}

pub fn should_show_notch(app: &AppHandle, cfg: &OverlayConfig, game_active: bool) -> bool {
    cfg.notch
        && !NOTCH_SNOOZE.load(Ordering::Relaxed)
        && !game_active
        && main_is_background(app)
}

pub fn payload(app: &AppHandle, cfg: &OverlayConfig, session_active: bool) -> OverlayPayload {
    OverlayPayload {
        config: cfg.clone(),
        session_active,
        notch_visible: should_show_notch(app, cfg, session_active),
        music_row: music_row(),
        mixer_row: mixer_row(),
    }
}

pub fn emit_config(app: &AppHandle, cfg: &OverlayConfig, session_active: bool) {
    let _ = app.emit("overlay-config", payload(app, cfg, session_active));
}

#[derive(Serialize, Clone)]
pub struct MonitorInfo {
    pub name: String,
    pub x: i32,
    pub y: i32,
    pub width: u32,
    pub height: u32,
    pub is_primary: bool,
    pub scale: f64,
}

pub fn list_monitors(app: &AppHandle) -> Result<Vec<MonitorInfo>, String> {
    let monitors = app.available_monitors().map_err(|e| e.to_string())?;
    let primary_name = app
        .primary_monitor()
        .ok()
        .flatten()
        .and_then(|m| m.name().map(|s| s.to_string()));

    Ok(monitors
        .into_iter()
        .enumerate()
        .map(|(i, m)| {
            let pos = m.position();
            let size = m.size();
            let name = m
                .name()
                .map(|s| s.to_string())
                .unwrap_or_else(|| format!("Écran {}", i + 1));
            let is_primary = primary_name
                .as_ref()
                .map(|p| p == &name)
                .unwrap_or(i == 0);
            MonitorInfo {
                name,
                x: pos.x,
                y: pos.y,
                width: size.width,
                height: size.height,
                is_primary,
                scale: m.scale_factor(),
            }
        })
        .collect())
}

struct RectCache {
    at: Instant,
    key: Option<String>,
    rect: Option<MonitorRect>,
}

static GAME_RECT_CACHE: Mutex<Option<RectCache>> = Mutex::new(None);

pub fn resolve_game_rect(app: &AppHandle, cfg: &OverlayConfig) -> Option<MonitorRect> {
    let key = cfg.game_monitor_name.clone();
    if let Ok(guard) = GAME_RECT_CACHE.lock() {
        if let Some(c) = guard.as_ref() {
            if c.key == key && c.at.elapsed() < Duration::from_secs(15) {
                return c.rect;
            }
        }
    }

    let monitors = list_monitors(app).ok()?;
    let m = pick_monitor(&monitors, key.as_deref())
        .or_else(|| monitors.iter().find(|m| m.is_primary))
        .or_else(|| monitors.first())?;
    let rect = MonitorRect {
        x: m.x,
        y: m.y,
        width: m.width as i32,
        height: m.height as i32,
    };
    if let Ok(mut guard) = GAME_RECT_CACHE.lock() {
        *guard = Some(RectCache {
            at: Instant::now(),
            key,
            rect: Some(rect),
        });
    }
    Some(rect)
}

pub fn get_metrics(app: &AppHandle, cfg: &OverlayConfig) -> Metrics {
    metrics::collect(resolve_game_rect(app, cfg))
}

fn each_overlay_win(app: &AppHandle, mut f: impl FnMut(&WebviewWindow)) {
    if let Some(w) = app.get_webview_window("overlay") {
        f(&w);
    }
    for i in 0..8 {
        if let Some(w) = app.get_webview_window(&format!("notch-{i}")) {
            f(&w);
        }
    }
}

/// Le WebView2 always-on-top se superpose au menu tray et aux aperçus.
/// On le range le temps du clic droit, puis `refresh_overlay` le ramène.
pub fn yield_for_native_menu(app: &AppHandle) {
    TRAY_YIELD.store(true, Ordering::SeqCst);
    each_overlay_win(app, |w| {
        let _ = w.set_always_on_top(false);
        let _ = w.hide();
    });
}

pub fn restore_after_native_menu() {
    TRAY_YIELD.store(false, Ordering::SeqCst);
}

fn decorate_overlay_window(win: &WebviewWindow) {
    let _ = win.set_skip_taskbar(true);
    let _ = win.set_title("");
    #[cfg(windows)]
    win32::decorate(win);
}

pub fn sync_overlay(app: &AppHandle, cfg: &OverlayConfig, game_active: bool) {
    let Some(win) = app.get_webview_window("overlay") else {
        metrics::set_sample_mode(metrics::SAMPLE_OFF);
        return;
    };
    if TRAY_YIELD.load(Ordering::Relaxed) {
        let _ = win.hide();
        sync_mirrors(app, cfg, false);
        metrics::set_sample_mode(metrics::SAMPLE_OFF);
        return;
    }
    let notch = should_show_notch(app, cfg, game_active);
    let card = cfg.enabled && !cfg.hidden && (game_active || cfg.always) && !notch;
    if !notch && !card {
        set_page_quiet(&win, true);
        let _ = win.hide();
        metrics::set_sample_mode(metrics::SAMPLE_OFF);
        sync_mirrors(app, cfg, false);
        return;
    }
    set_page_quiet(&win, false);
    if let Err(e) = place_overlay(app, &win, cfg, notch) {
        eprintln!("overlay place: {e}");
    }
    let _ = win.set_ignore_cursor_events(!notch);
    let _ = win.set_always_on_top(true);
    let _ = win.show();
    decorate_overlay_window(&win);
    sync_mirrors(app, cfg, notch);
    metrics::set_sample_mode(if notch {
        metrics::SAMPLE_LIGHT
    } else {
        metrics::SAMPLE_FULL
    });
}

fn pick_monitor<'a>(monitors: &'a [MonitorInfo], name: Option<&str>) -> Option<&'a MonitorInfo> {
    if let Some(n) = name {
        if let Some(m) = monitors.iter().find(|m| m.name == n) {
            return Some(m);
        }
    }
    monitors.iter().find(|m| !m.is_primary).or(monitors.first())
}

fn pick_notch_monitor<'a>(
    monitors: &'a [MonitorInfo],
    name: Option<&str>,
) -> Option<&'a MonitorInfo> {
    if let Some(n) = name {
        if let Some(m) = monitors.iter().find(|m| m.name == n) {
            return Some(m);
        }
    }
    monitors.iter().find(|m| m.is_primary).or(monitors.first())
}

fn place_overlay(
    app: &AppHandle,
    win: &WebviewWindow,
    cfg: &OverlayConfig,
    notch: bool,
) -> Result<(), String> {
    let monitors = list_monitors(app)?;

    if notch {
        let mon = pick_notch_monitor(&monitors, cfg.notch_monitor_name.as_deref())
            .ok_or_else(|| "aucun moniteur".to_string())?;
        let scale = mon.scale.max(1.0);
        let (width, height) = notch_size(cfg, scale);
        place_notch_at(win, mon, &cfg.notch_position, width, height)?;
        return Ok(());
    }

    let mon = pick_monitor(&monitors, cfg.monitor_name.as_deref())
        .ok_or_else(|| "aucun moniteur".to_string())?;

    let mut lines = 0u32;
    if cfg.show_fps {
        lines += 1;
    }
    if cfg.show_cpu {
        lines += 1;
    }
    if cfg.show_gpu {
        lines += 1;
    }
    if cfg.show_temps {
        lines += 2;
    }
    if cfg.show_ram {
        lines += 1;
    }
    let lines = lines.max(1);

    let width = 300u32;
    let height = 44 + lines * 36 + 16;
    let margin = 18i32;
    let (x, y) = match cfg.position.as_str() {
        "top-left" => (mon.x + margin, mon.y + margin),
        "bottom-left" => (
            mon.x + margin,
            mon.y + mon.height as i32 - height as i32 - margin,
        ),
        "bottom-right" => (
            mon.x + mon.width as i32 - width as i32 - margin,
            mon.y + mon.height as i32 - height as i32 - margin,
        ),
        _ => (
            mon.x + mon.width as i32 - width as i32 - margin,
            mon.y + margin,
        ),
    };

    win.set_size(PhysicalSize::new(width, height))
        .map_err(|e| e.to_string())?;
    win.set_position(PhysicalPosition::new(x, y))
        .map_err(|e| e.to_string())?;
    Ok(())
}

pub fn toggle_hidden(cfg: &mut OverlayConfig) {
    cfg.hidden = !cfg.hidden;
}

fn notch_size(cfg: &OverlayConfig, scale: f64) -> (u32, u32) {
    let shelf = crate::shelf::expanded();
    let music = music_row();
    let mixer = mixer_row();
    let extra = !cfg.notch_compact && (shelf || music || mixer);
    let mut logical: f64 = if cfg.notch_show_mascot { 132.0 } else { 96.0 };
    if cfg.notch_show_ram {
        logical += 96.0;
    }
    if cfg.notch_show_cpu {
        logical += 84.0;
    }
    if cfg.notch_show_media {
        logical += 148.0;
    }
    logical += 36.0;
    if mixer {
        logical = logical.max(420.0);
    }
    if extra {
        logical = logical.max(560.0);
    }
    if cfg.notch_compact {
        logical = if cfg.notch_show_mascot { 52.0 } else { 92.0 };
    }
    let mixer_h = if mixer {
        52.0 + (crate::mixer::last_count().clamp(1, 6) as f64) * 40.0
    } else {
        0.0
    };
    let height = if cfg.notch_compact {
        if cfg.notch_show_mascot { 44.0 } else { 36.0 }
    } else if mixer && shelf {
        56.0 + mixer_h + 40.0
    } else if mixer {
        56.0 + mixer_h
    } else if shelf && music {
        148.0
    } else if extra {
        108.0
    } else {
        56.0
    };
    (
        (logical * scale).round() as u32,
        (height * scale).round() as u32,
    )
}

fn place_notch_at(
    win: &WebviewWindow,
    mon: &MonitorInfo,
    pos: &str,
    width: u32,
    height: u32,
) -> Result<(), String> {
    let scale = mon.scale.max(1.0);
    let inset = (20.0 * scale).round() as i32;
    let x = match pos {
        "top-left" | "bottom-left" => mon.x + inset,
        "top-right" | "bottom-right" => mon.x + mon.width as i32 - width as i32 - inset,
        _ => mon.x + (mon.width as i32 - width as i32) / 2,
    };
    let y = if pos.starts_with("bottom") {
        let taskbar = (48.0 * scale).round() as i32;
        mon.y + mon.height as i32 - height as i32 - taskbar
    } else {
        mon.y
    };
    win.set_size(PhysicalSize::new(width, height))
        .map_err(|e| e.to_string())?;
    win.set_position(PhysicalPosition::new(x, y))
        .map_err(|e| e.to_string())?;
    Ok(())
}

fn create_notch_window(app: &AppHandle, label: &str) -> Option<WebviewWindow> {
    use tauri::webview::WebviewWindowBuilder;
    use tauri::WebviewUrl;
    WebviewWindowBuilder::new(app, label, WebviewUrl::App("index.html?window=overlay".into()))
        .title(" ")
        .inner_size(220.0, 48.0)
        .resizable(false)
        .decorations(false)
        .transparent(true)
        .always_on_top(true)
        .skip_taskbar(true)
        .visible(false)
        .focused(false)
        .shadow(false)
        .additional_browser_args("--disable-background-networking --disable-component-update --disable-sync --disable-features=Translate,MediaRouter --mute-audio")
        .build()
        .ok()
        .map(|w| {
            decorate_overlay_window(&w);
            w
        })
}

fn sync_mirrors(app: &AppHandle, cfg: &OverlayConfig, notch: bool) {
    let monitors = list_monitors(app).unwrap_or_default();
    let chosen = pick_notch_monitor(&monitors, cfg.notch_monitor_name.as_deref()).map(|m| m.name.clone());
    let mut keep: Vec<String> = Vec::new();
    if notch && cfg.notch_all_screens {
        for (i, mon) in monitors.iter().enumerate().take(8) {
            if chosen.as_ref() == Some(&mon.name) {
                continue;
            }
            let label = format!("notch-{i}");
            let win = match app.get_webview_window(&label) {
                Some(w) => w,
                None => match create_notch_window(app, &label) {
                    Some(w) => w,
                    None => continue,
                },
            };
            let scale = mon.scale.max(1.0);
            let (width, height) = notch_size(cfg, scale);
            if place_notch_at(&win, mon, &cfg.notch_position, width, height).is_ok() {
                set_page_quiet(&win, false);
                let _ = win.set_always_on_top(true);
                let _ = win.show();
                decorate_overlay_window(&win);
                keep.push(label);
            }
        }
    }
    for i in 0..8 {
        let label = format!("notch-{i}");
        if keep.iter().any(|k| k == &label) {
            continue;
        }
        if let Some(w) = app.get_webview_window(&label) {
            let _ = w.destroy();
        }
    }
}

#[cfg(windows)]
mod win32 {
    use super::WebviewWindow;
    use std::ffi::c_void;
    use windows::core::w;
    use windows::Win32::Foundation::{BOOL, HANDLE, HWND, LPARAM, TRUE};
    use windows::Win32::Graphics::Dwm::{
        DwmSetWindowAttribute, DWMWA_DISALLOW_PEEK, DWMWA_EXCLUDED_FROM_PEEK,
    };
    use windows::Win32::System::Com::{CoCreateInstance, CLSCTX_ALL};
    use windows::Win32::UI::Shell::{ITaskbarList3, TaskbarList};
    use windows::Win32::UI::WindowsAndMessaging::{
        EnumChildWindows, GetWindowLongPtrW, SetPropW, SetWindowLongPtrW, SetWindowPos,
        GWL_EXSTYLE, SWP_FRAMECHANGED, SWP_NOACTIVATE, SWP_NOMOVE, SWP_NOSIZE, SWP_NOZORDER,
        WS_EX_APPWINDOW, WS_EX_TOOLWINDOW,
    };

    fn hwnd_of(win: &WebviewWindow) -> Option<HWND> {
        let raw = win.hwnd().ok()?;
        Some(HWND(raw.0 as *mut c_void))
    }

    static DECORATED: std::sync::OnceLock<std::sync::Mutex<std::collections::HashSet<isize>>> =
        std::sync::OnceLock::new();

    fn decorated_set() -> &'static std::sync::Mutex<std::collections::HashSet<isize>> {
        DECORATED.get_or_init(|| std::sync::Mutex::new(std::collections::HashSet::new()))
    }

    unsafe fn strip_taskbar(hwnd: HWND) {
        let mut ex = GetWindowLongPtrW(hwnd, GWL_EXSTYLE);
        ex |= WS_EX_TOOLWINDOW.0 as isize;
        ex &= !(WS_EX_APPWINDOW.0 as isize);
        SetWindowLongPtrW(hwnd, GWL_EXSTYLE, ex);
        let _ = SetWindowPos(
            hwnd,
            HWND::default(),
            0,
            0,
            0,
            0,
            SWP_NOMOVE | SWP_NOSIZE | SWP_NOZORDER | SWP_NOACTIVATE | SWP_FRAMECHANGED,
        );
        let _ = SetPropW(hwnd, w!("NonRudeHWND"), HANDLE(1usize as *mut c_void));
        let exclude = TRUE;
        let _ = DwmSetWindowAttribute(
            hwnd,
            DWMWA_EXCLUDED_FROM_PEEK,
            &exclude as *const BOOL as *const c_void,
            std::mem::size_of::<BOOL>() as u32,
        );
        let _ = DwmSetWindowAttribute(
            hwnd,
            DWMWA_DISALLOW_PEEK,
            &exclude as *const BOOL as *const c_void,
            std::mem::size_of::<BOOL>() as u32,
        );
    }

    pub fn decorate(win: &WebviewWindow) {
        let Some(hwnd) = hwnd_of(win) else {
            return;
        };
        let key = hwnd.0 as isize;
        unsafe {
            strip_taskbar(hwnd);
        }
        if let Ok(g) = decorated_set().lock() {
            if g.contains(&key) {
                return;
            }
        }
        unsafe {
            if let Ok(list) = CoCreateInstance::<_, ITaskbarList3>(&TaskbarList, None, CLSCTX_ALL) {
                let _ = list.HrInit();
                let _ = list.DeleteTab(hwnd);
                let boxed = Box::new(list);
                let ptr = Box::into_raw(boxed);
                let _ = EnumChildWindows(hwnd, Some(delete_child_tab), LPARAM(ptr as isize));
                drop(Box::from_raw(ptr));
            }
        }
        if let Ok(mut g) = decorated_set().lock() {
            g.insert(key);
        }
    }

    unsafe extern "system" fn delete_child_tab(child: HWND, lparam: LPARAM) -> BOOL {
        let list = &*(lparam.0 as *const ITaskbarList3);
        let _ = list.DeleteTab(child);
        TRUE
    }
}
