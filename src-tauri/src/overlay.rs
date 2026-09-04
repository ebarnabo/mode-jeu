use crate::metrics::{self, Metrics, MonitorRect};
use serde::{Deserialize, Serialize};
use std::sync::Mutex;
use std::time::{Duration, Instant};
use tauri::{AppHandle, Manager, PhysicalPosition, PhysicalSize, WebviewWindow};

fn default_true() -> bool {
    true
}
fn default_interval() -> u32 {
    2000
}
fn default_opacity() -> u8 {
    88
}
fn default_position() -> String {
    "top-right".into()
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
            opacity: 88,
            position: "top-right".into(),
            interval_ms: 2000,
            hidden: false,
        }
    }
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

pub fn sync_overlay(app: &AppHandle, cfg: &OverlayConfig, game_active: bool) {
    let Some(win) = app.get_webview_window("overlay") else {
        metrics::set_sampling(false);
        return;
    };
    let should_show = cfg.enabled && !cfg.hidden && (game_active || cfg.always);
    if !should_show {
        let _ = win.hide();
        metrics::set_sampling(false);
        return;
    }
    if let Err(e) = place_overlay(app, &win, cfg) {
        eprintln!("overlay place: {e}");
    }
    let _ = win.set_skip_taskbar(true);
    let _ = win.set_ignore_cursor_events(true);
    let _ = win.set_always_on_top(true);
    let _ = win.show();
    metrics::set_sampling(true);
}

fn pick_monitor<'a>(monitors: &'a [MonitorInfo], name: Option<&str>) -> Option<&'a MonitorInfo> {
    if let Some(n) = name {
        if let Some(m) = monitors.iter().find(|m| m.name == n) {
            return Some(m);
        }
    }
    monitors.iter().find(|m| !m.is_primary).or(monitors.first())
}

fn place_overlay(app: &AppHandle, win: &WebviewWindow, cfg: &OverlayConfig) -> Result<(), String> {
    let monitors = list_monitors(app)?;
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

    // Overlay plus large : valeurs lisibles + temps séparés.
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
