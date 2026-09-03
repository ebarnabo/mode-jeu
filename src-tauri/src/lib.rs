use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use std::path::PathBuf;
use std::process::Command;
use sysinfo::{ProcessesToUpdate, System};
use tauri::menu::{Menu, MenuItem};
use tauri::tray::{MouseButton, MouseButtonState, TrayIconBuilder, TrayIconEvent};
use tauri::{AppHandle, Manager, WindowEvent};

#[cfg(windows)]
use std::os::windows::process::CommandExt;
#[cfg(windows)]
const NO_WINDOW: u32 = 0x0800_0000;

/// Jamais tué, quelles que soient les options. Filet de sécurité dur.
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
    // Pilotes / overlays indispensables au jeu
    "radeonsoftware.exe", "amdow.exe", "nvcontainer.exe", "nvdisplay.container.exe",
];

/// Launchers, anti-cheats, overlays et outils gaming — protégés par défaut.
const SAFE_EXACT: &[&str] = &[
    // Steam
    "steam.exe", "steamwebhelper.exe", "steamservice.exe", "steamerrorreporter.exe",
    "gameoverlayui.exe", "gameoverlayui64.exe", "streamingclient.exe",
    // Epic
    "epicgameslauncher.exe", "epicwebhelper.exe", "epicgamesupdater.exe",
    "unrealcefsubprocess.exe",
    // EA / Origin
    "eadesktop.exe", "ealocalhostsvc.exe", "origin.exe", "originwebhelper.exe",
    // Battle.net / Blizzard
    "battle.net.exe", "blizzardupdateagent.exe",
    // GOG
    "galaxyclient.exe", "goggalaxy.exe",
    // Ubisoft
    "upc.exe", "ubisoftconnect.exe",
    // Riot / Vanguard
    "riotclientservices.exe", "riotclientux.exe", "riotclientuxrender.exe",
    "vgc.exe", "vgtray.exe",
    // Xbox / Microsoft Store
    "xboxapp.exe", "gamebar.exe", "gamebarftserver.exe", "gamebarpresencewriter.exe",
    "xboxgamespeech.exe",
    // GPU / overlays
    "nvidia share.exe", "nvsphelper64.exe", "nvidia notification.exe",
    "rtsshooksloader64.exe", "msiafterburner.exe",
    // Sync / cloud utiles au gaming
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
    pub high_performance: bool,
    pub protect_foreground: bool,
    pub stop_services: bool,
    pub services: Vec<String>,
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
            services: vec!["SysMain".into(), "WSearch".into()],
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

/// Un process est candidat s'il n'est pas dans C:\Windows et possède un chemin.
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

#[tauri::command]
fn get_config(app: AppHandle) -> Config {
    read_json::<Config>(&app, "config.json")
}

#[tauri::command]
fn save_config(app: AppHandle, config: Config) {
    write_json(&app, "config.json", &config);
}

#[tauri::command]
fn get_session(app: AppHandle) -> Session {
    read_json::<Session>(&app, "session.json")
}

#[tauri::command]
fn list_processes(app: AppHandle) -> Vec<ProcGroup> {
    scan(&read_json::<Config>(&app, "config.json"))
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
    dock_to_taskbar(&app);
    Ok(session)
}

#[tauri::command]
fn restore(app: AppHandle) -> Result<Session, String> {
    let empty = restore_session(&app);
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
    app.exit(0);
}

fn setup_tray(app: &AppHandle) -> tauri::Result<()> {
    let open = MenuItem::with_id(app, "open", "Ouvrir", true, None::<&str>)?;
    let quit = MenuItem::with_id(app, "quit", "Quitter", true, None::<&str>)?;
    let menu = Menu::with_items(app, &[&open, &quit])?;
    let mut tray = TrayIconBuilder::new()
        .tooltip("Mode Jeu")
        .menu(&menu)
        .show_menu_on_left_click(false)
        .on_menu_event(|app, event| match event.id.as_ref() {
            "open" => show_main(app),
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
        .setup(|app| {
            setup_tray(app.handle())?;
            Ok(())
        })
        .on_window_event(|window, event| {
            if let WindowEvent::CloseRequested { api, .. } = event {
                api.prevent_close();
                let _ = window.minimize();
            }
        })
        .invoke_handler(tauri::generate_handler![
            get_config,
            save_config,
            get_session,
            list_processes,
            activate,
            restore
        ])
        .run(tauri::generate_context!())
        .expect("erreur au lancement de l'application");
}
