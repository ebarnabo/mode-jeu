mod metrics;
mod overlay;

use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use std::path::PathBuf;
use std::process::Command;
use sysinfo::{ProcessesToUpdate, System};
use tauri::menu::{Menu, MenuItem};
use tauri::tray::{MouseButton, MouseButtonState, TrayIconBuilder, TrayIconEvent};
use tauri::{AppHandle, Emitter, Manager, WindowEvent};

use overlay::OverlayConfig;

#[cfg(windows)]
use std::os::windows::process::CommandExt;
#[cfg(windows)]
const NO_WINDOW: u32 = 0x0800_0000;

const CRITICAL: &[&str] = &[
    "system", "registry", "smss.exe", "csrss.exe", "wininit.exe", "winlogon.exe",
    "services.exe", "lsass.exe", "svchost.exe", "dwm.exe", "explorer.exe",
    "fontdrvhost.exe", "sihost.exe", "taskhostw.exe", "ctfmon.exe", "audiodg.exe",
    "conhost.exe", "runtimebroker.exe", "shellexperiencehost.exe", "startmenuexperiencehost.exe",
    "searchhost.exe", "textinputhost.exe", "lockapp.exe", "memory compression",
    "wudfhost.exe", "spoolsv.exe", "taskmgr.exe", "mode-jeu.exe",
    "dllhost.exe", "wmiprvse.exe", "searchindexer.exe", "searchprotocolhost.exe",
    "applicationframehost.exe", "systemsettings.exe", "msedgewebview2.exe",
    "securityhealthsystray.exe", "securityhealthservice.exe", "msmpeng.exe", "nissrv.exe",
    "widgetservice.exe", "widgets.exe", "phoneexperiencehost.exe",
    "radeonsoftware.exe", "amdow.exe", "nvcontainer.exe", "nvdisplay.container.exe",
];

const SAFE_EXACT: &[&str] = &[
    "steam.exe", "steamwebhelper.exe", "steamservice.exe", "steamerrorreporter.exe",
    "gameoverlayui.exe", "gameoverlayui64.exe", "streamingclient.exe",
    "epicgameslauncher.exe", "epicwebhelper.exe", "epicgamesupdater.exe",
    "unrealcefsubprocess.exe",
    "eadesktop.exe", "ealocalhostsvc.exe", "origin.exe", "originwebhelper.exe",
    "battle.net.exe", "blizzardupdateagent.exe",
    "galaxyclient.exe", "goggalaxy.exe",
    "upc.exe", "ubisoftconnect.exe",
    "riotclientservices.exe", "riotclientux.exe", "riotclientuxrender.exe",
    "vgc.exe", "vgtray.exe",
    "xboxapp.exe", "gamebar.exe", "gamebarftserver.exe", "gamebarpresencewriter.exe",
    "xboxgamespeech.exe",
    "nvidia share.exe", "nvsphelper64.exe", "nvidia notification.exe",
    "rtsshooksloader64.exe", "msiafterburner.exe",
    "onedrive.exe", "onedrivestandaloneupdater.exe",
];

const SAFE_PREFIXES: &[&str] = &[
    "steam", "epic", "eadesktop", "ealocal", "origin", "battle.net", "blizzard",
    "galaxy", "gog", "ubisoft", "uplay", "upc", "riotclient", "vgc", "vgtray",
    "xbox", "gamebar", "gameoverlay", "nvidia", "nvcontainer", "nvdisplay",
    "amd", "radeon", "rivatuner", "rtss", "msiafterburner",
];

const SAFE_PATH_FRAGMENTS: &[&str] = &[
    "\\steam\\", "\\steamapps\\", "\\epic games\\", "\\electronic arts\\",
    "\\ea games\\", "\\origin games\\", "\\battle.net\\", "\\blizzard\\",
    "\\gog galaxy\\", "\\riot games\\", "\\ubisoft game launcher\\",
    "\\ubisoft\\ubisoft game launcher\\", "\\xboxgames\\", "\\nvidia corporation\\",
    "\\amd\\", "\\common files\\ea\\",
];

const HIGH_PERF_GUID: &str = "8c5e7fda-e8bf-4a96-9a85-a6e23a8c635c";

fn default_true() -> bool {
    true
}

#[derive(Serialize, Clone)]
pub struct ProcGroup {
    pub key: String,
    pub name: String,
    pub memory_mb: u64,
    pub instances: usize,
    pub path: Option<String>,
    pub protected: bool,
    pub kept: bool,
    pub foreground: bool,
}

#[derive(Serialize, Deserialize, Clone)]
pub struct Config {
    pub keep: Vec<String>,
    #[serde(default = "default_true")]
    pub high_performance: bool,
    #[serde(default = "default_true")]
    pub protect_foreground: bool,
    #[serde(default)]
    pub stop_services: bool,
    #[serde(default = "default_services")]
    pub services: Vec<String>,
    #[serde(default = "default_true")]
    pub minimize_on_activate: bool,
    #[serde(default)]
    pub start_with_windows: bool,
    #[serde(default)]
    pub overlay: OverlayConfig,
}

fn default_services() -> Vec<String> {
    vec!["SysMain".into(), "WSearch".into()]
}

impl Default for Config {
    fn default() -> Self {
        Self {
            keep: vec![
                "steam.exe".into(),
                "epicgameslauncher.exe".into(),
                "discord.exe".into(),
            ],
            high_performance: true,
            protect_foreground: true,
            stop_services: false,
            services: default_services(),
            minimize_on_activate: true,
            start_with_windows: false,
            overlay: OverlayConfig::default(),
        }
    }
}

#[derive(Serialize, Deserialize, Default, Clone)]
pub struct Session {
    pub active: bool,
    pub closed: Vec<String>,
    pub closed_names: Vec<String>,
    pub freed_mb: u64,
    pub previous_plan: Option<String>,
    pub stopped_services: Vec<String>,
}

fn dir(app: &AppHandle) -> PathBuf {
    let d = app.path().app_config_dir().unwrap_or_else(|_| PathBuf::from("."));
    let _ = std::fs::create_dir_all(&d);
    d
}

fn read_json<T: for<'a> Deserialize<'a> + Default>(app: &AppHandle, file: &str) -> T {
    std::fs::read_to_string(dir(app).join(file))
        .ok()
        .and_then(|s| serde_json::from_str(&s).ok())
        .unwrap_or_default()
}

fn write_json<T: Serialize>(app: &AppHandle, file: &str, v: &T) {
    if let Ok(s) = serde_json::to_string_pretty(v) {
        let _ = std::fs::write(dir(app).join(file), s);
    }
}

fn is_critical(name: &str) -> bool {
    CRITICAL.contains(&name.to_lowercase().as_str())
}

fn is_safe_by_default(name: &str, path: &Option<String>) -> bool {
    let key = name.to_lowercase();
    if SAFE_EXACT.contains(&key.as_str()) {
        return true;
    }
    if SAFE_PREFIXES.iter().any(|p| key.starts_with(p)) {
        return true;
    }
    if let Some(p) = path {
        let lp = p.to_lowercase();
        if SAFE_PATH_FRAGMENTS.iter().any(|f| lp.contains(f)) {
            return true;
        }
        if key == "agent.exe" && (lp.contains("battle.net") || lp.contains("blizzard")) {
            return true;
        }
    }
    false
}

fn is_candidate(path: &Option<String>) -> bool {
    match path {
        Some(p) => {
            let lp = p.to_lowercase();
            !lp.starts_with("c:\\windows") && !lp.is_empty()
        }
        None => false,
    }
}

#[cfg(windows)]
fn foreground_pid() -> Option<u32> {
    use windows::Win32::UI::WindowsAndMessaging::{GetForegroundWindow, GetWindowThreadProcessId};
    unsafe {
        let hwnd = GetForegroundWindow();
        if hwnd.is_invalid() {
            return None;
        }
        let mut pid = 0u32;
        GetWindowThreadProcessId(hwnd, Some(&mut pid));
        (pid != 0).then_some(pid)
    }
}

#[cfg(not(windows))]
fn foreground_pid() -> Option<u32> {
    None
}

fn scan(cfg: &Config) -> Vec<ProcGroup> {
    let mut sys = System::new();
    sys.refresh_processes(ProcessesToUpdate::All, true);
    let fg = foreground_pid();
    let self_pid = std::process::id();
    let mut map: HashMap<String, ProcGroup> = HashMap::new();

    for (pid, proc_) in sys.processes() {
        let name = proc_.name().to_string_lossy().to_string();
        let key = name.to_lowercase();
        if key.is_empty() || pid.as_u32() == self_pid {
            continue;
        }
        let path = proc_.exe().map(|p| p.to_string_lossy().to_string());
        let protected =
            is_critical(&key) || is_safe_by_default(&key, &path) || !is_candidate(&path);
        let entry = map.entry(key.clone()).or_insert_with(|| ProcGroup {
            key: key.clone(),
            name: name.trim_end_matches(".exe").to_string(),
            memory_mb: 0,
            instances: 0,
            path: path.clone(),
            protected,
            kept: cfg.keep.contains(&key),
            foreground: false,
        });
        entry.memory_mb += proc_.memory() / 1_048_576;
        entry.instances += 1;
        if entry.path.is_none() {
            entry.path = path;
        }
        if Some(pid.as_u32()) == fg {
            entry.foreground = true;
        }
    }

    let mut v: Vec<ProcGroup> = map.into_values().filter(|p| p.memory_mb > 0).collect();
    v.sort_by(|a, b| {
        b.protected
            .cmp(&a.protected)
            .reverse()
            .then(b.memory_mb.cmp(&a.memory_mb))
    });
    v
}

fn apply_startup(enabled: bool) {
    #[cfg(windows)]
    {
        let Ok(exe) = std::env::current_exe() else {
            return;
        };
        let value = format!("\"{}\"", exe.display());
        if enabled {
            let _ = run(
                "reg",
                &[
                    "add",
                    r"HKCU\Software\Microsoft\Windows\CurrentVersion\Run",
                    "/v",
                    "ModeJeu",
                    "/t",
                    "REG_SZ",
                    "/d",
                    &value,
                    "/f",
                ],
            );
        } else {
            let _ = run(
                "reg",
                &[
                    "delete",
                    r"HKCU\Software\Microsoft\Windows\CurrentVersion\Run",
                    "/v",
                    "ModeJeu",
                    "/f",
                ],
            );
        }
    }
    #[cfg(not(windows))]
    let _ = enabled;
}

fn sync_from_config(app: &AppHandle, cfg: &Config) {
    apply_startup(cfg.start_with_windows);
    let session = read_json::<Session>(app, "session.json");
    overlay::sync_overlay(app, &cfg.overlay, session.active);
    let _ = app.emit("overlay-config", &cfg.overlay);
}

#[tauri::command]
fn get_config(app: AppHandle) -> Config {
    read_json::<Config>(&app, "config.json")
}

#[tauri::command]
fn save_config(app: AppHandle, config: Config) {
    write_json(&app, "config.json", &config);
    sync_from_config(&app, &config);
}

#[tauri::command]
fn get_session(app: AppHandle) -> Session {
    read_json::<Session>(&app, "session.json")
}

#[tauri::command]
fn list_processes(app: AppHandle) -> Vec<ProcGroup> {
    scan(&read_json::<Config>(&app, "config.json"))
}

#[tauri::command]
fn list_monitors(app: AppHandle) -> Result<Vec<overlay::MonitorInfo>, String> {
    overlay::list_monitors(&app)
}

#[tauri::command]
fn get_metrics(app: AppHandle) -> metrics::Metrics {
    let cfg = read_json::<Config>(&app, "config.json");
    overlay::get_metrics(&app, &cfg.overlay)
}

#[tauri::command]
fn get_overlay_config(app: AppHandle) -> OverlayConfig {
    read_json::<Config>(&app, "config.json").overlay
}

#[tauri::command]
fn toggle_overlay_visibility(app: AppHandle) -> OverlayConfig {
    let mut cfg = read_json::<Config>(&app, "config.json");
    overlay::toggle_hidden(&mut cfg.overlay);
    write_json(&app, "config.json", &cfg);
    sync_from_config(&app, &cfg);
    cfg.overlay
}

fn run(cmd: &str, args: &[&str]) -> Option<String> {
    let mut c = Command::new(cmd);
    c.args(args);
    #[cfg(windows)]
    c.creation_flags(NO_WINDOW);
    c.output().ok().map(|o| String::from_utf8_lossy(&o.stdout).to_string())
}

fn active_plan() -> Option<String> {
    let out = run("powercfg", &["/getactivescheme"])?;
    out.split_whitespace()
        .find(|t| t.len() == 36 && t.matches('-').count() == 4)
        .map(|s| s.to_string())
}

#[tauri::command]
fn activate(app: AppHandle) -> Result<Session, String> {
    let cfg = read_json::<Config>(&app, "config.json");
    let groups = scan(&cfg);

    let mut session = Session {
        active: true,
        ..Default::default()
    };

    if cfg.high_performance {
        session.previous_plan = active_plan();
        run("powercfg", &["/setactive", HIGH_PERF_GUID]);
    }

    let targets: Vec<&ProcGroup> = groups
        .iter()
        .filter(|g| {
            !g.protected
                && !g.kept
                && !is_safe_by_default(&g.key, &g.path)
                && !(cfg.protect_foreground && g.foreground)
        })
        .collect();

    let mut sys = System::new();
    sys.refresh_processes(ProcessesToUpdate::All, true);

    for g in targets {
        let mut killed = false;
        for (_, p) in sys.processes() {
            if p.name().to_string_lossy().to_lowercase() == g.key && p.kill() {
                killed = true;
            }
        }
        if killed {
            session.freed_mb += g.memory_mb;
            session.closed_names.push(g.name.clone());
            if let Some(path) = &g.path {
                session.closed.push(path.clone());
            }
        }
    }

    if cfg.stop_services {
        for s in &cfg.services {
            if run("sc", &["stop", s]).is_some() {
                session.stopped_services.push(s.clone());
            }
        }
    }

    write_json(&app, "session.json", &session);
    overlay::sync_overlay(&app, &cfg.overlay, true);
    let _ = app.emit("overlay-config", &cfg.overlay);
    if cfg.minimize_on_activate {
        dock_to_taskbar(&app);
    }
    Ok(session)
}

#[tauri::command]
fn restore(app: AppHandle) -> Result<Session, String> {
    let empty = restore_session(&app);
    let cfg = read_json::<Config>(&app, "config.json");
    overlay::sync_overlay(&app, &cfg.overlay, false);
    let _ = app.emit("overlay-config", &cfg.overlay);
    show_main(&app);
    Ok(empty)
}

fn restore_session(app: &AppHandle) -> Session {
    let session = read_json::<Session>(app, "session.json");

    for path in &session.closed {
        let mut c = Command::new(path);
        #[cfg(windows)]
        c.creation_flags(NO_WINDOW);
        let _ = c.spawn();
    }
    for s in &session.stopped_services {
        run("sc", &["start", s]);
    }
    if let Some(plan) = &session.previous_plan {
        run("powercfg", &["/setactive", plan]);
    }

    let empty = Session::default();
    write_json(app, "session.json", &empty);
    empty
}

fn show_main(app: &AppHandle) {
    if let Some(w) = app.get_webview_window("main") {
        let _ = w.unminimize();
        let _ = w.show();
        let _ = w.set_focus();
    }
}

fn dock_to_taskbar(app: &AppHandle) {
    if let Some(w) = app.get_webview_window("main") {
        let _ = w.minimize();
    }
}

fn quit_app(app: &AppHandle) {
    let _ = restore_session(app);
    if let Some(w) = app.get_webview_window("overlay") {
        let _ = w.hide();
    }
    app.exit(0);
}

fn setup_tray(app: &AppHandle) -> tauri::Result<()> {
    let open = MenuItem::with_id(app, "open", "Ouvrir", true, None::<&str>)?;
    let overlay_toggle =
        MenuItem::with_id(app, "overlay", "Afficher / masquer l'overlay", true, None::<&str>)?;
    let quit = MenuItem::with_id(app, "quit", "Quitter", true, None::<&str>)?;
    let menu = Menu::with_items(app, &[&open, &overlay_toggle, &quit])?;
    let mut tray = TrayIconBuilder::new()
        .tooltip("Mode Jeu")
        .menu(&menu)
        .show_menu_on_left_click(false)
        .on_menu_event(|app, event| match event.id.as_ref() {
            "open" => show_main(app),
            "overlay" => {
                let _ = toggle_overlay_visibility(app.clone());
            }
            "quit" => quit_app(app),
            _ => {}
        })
        .on_tray_icon_event(|tray, event| {
            if let TrayIconEvent::Click {
                button: MouseButton::Left,
                button_state: MouseButtonState::Up,
                ..
            } = event
            {
                show_main(tray.app_handle());
            }
        });
    if let Some(icon) = app.default_window_icon() {
        tray = tray.icon(icon.clone());
    }
    tray.build(app)?;
    Ok(())
}

#[cfg_attr(mobile, tauri::mobile_entry_point)]
pub fn run_app() {
    tauri::Builder::default()
        .plugin(tauri_plugin_process::init())
        .setup(|app| {
            #[cfg(windows)]
            app.handle()
                .plugin(tauri_plugin_updater::Builder::new().build())?;
            setup_tray(app.handle())?;
            let cfg = read_json::<Config>(app.handle(), "config.json");
            sync_from_config(app.handle(), &cfg);
            Ok(())
        })
        .on_window_event(|window, event| {
            if let WindowEvent::CloseRequested { api, .. } = event {
                if window.label() == "overlay" {
                    api.prevent_close();
                    let _ = window.hide();
                    return;
                }
                api.prevent_close();
                let _ = window.minimize();
            }
        })
        .invoke_handler(tauri::generate_handler![
            get_config,
            save_config,
            get_session,
            list_processes,
            list_monitors,
            get_metrics,
            get_overlay_config,
            toggle_overlay_visibility,
            activate,
            restore
        ])
        .run(tauri::generate_context!())
        .expect("erreur au lancement de l'application");
}
