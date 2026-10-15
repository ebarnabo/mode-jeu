mod boost;
mod disks;
mod games;
mod media;
mod metrics;
mod overlay;
mod shelf;
mod software;
mod tweaks;
#[cfg(windows)]
mod amd_adl;

use serde::{Deserialize, Serialize};
use std::collections::{HashMap, HashSet};
use std::path::PathBuf;
use std::process::Command;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::Mutex;
use std::time::{Duration, Instant};
use sysinfo::{ProcessesToUpdate, System};
use tauri::menu::{Menu, MenuItem};
use tauri::tray::{MouseButton, MouseButtonState, TrayIconBuilder, TrayIconEvent};
use tauri::{AppHandle, DragDropEvent, Emitter, Manager, WindowEvent};

use overlay::{OverlayConfig, OverlayPayload};

#[cfg(windows)]
use std::os::windows::process::CommandExt;
#[cfg(windows)]
const NO_WINDOW: u32 = 0x0800_0000;

static SESSION_ON: AtomicBool = AtomicBool::new(false);

pub(crate) fn session_is_active() -> bool {
    SESSION_ON.load(Ordering::Relaxed)
}

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

fn default_true() -> bool {
    true
}

#[derive(Serialize, Clone)]
pub struct ProcGroup {
    pub key: String,
    pub name: String,
    pub memory_mb: u64,
    pub cpu_pct: f32,
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
    #[serde(default)]
    pub ultimate_performance: bool,
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
    #[serde(default = "default_true")]
    pub disable_game_dvr: bool,
    #[serde(default = "default_true")]
    pub enable_game_mode: bool,
    #[serde(default)]
    pub disable_notifications: bool,
    #[serde(default)]
    pub visual_effects_perf: bool,
    #[serde(default)]
    pub disable_transparency: bool,
    #[serde(default = "default_true")]
    pub disable_widgets: bool,
    #[serde(default)]
    pub disable_background_apps: bool,
    #[serde(default)]
    pub mmcss_gaming: bool,
    #[serde(default = "default_true")]
    pub cpu_unpark: bool,
    /// Boost agressif + préférence énergie à fond, secteur seulement.
    #[serde(default = "default_true")]
    pub cpu_boost: bool,
    /// Focus assist en « alarmes seulement » le temps de la session.
    #[serde(default = "default_true")]
    pub focus_assist: bool,
    /// Préférence GPU haute performance pour les jeux connus.
    #[serde(default = "default_true")]
    pub gpu_high_perf: bool,
    /// Ignore les apps sous ce seuil (Mo) à la fermeture — moins de bruit, plus rapide.
    #[serde(default = "default_min_close_mb")]
    pub min_close_mb: u64,
    /// Si false, les apps fermées ne sont pas relancées à la sortie du mode jeu.
    #[serde(default = "default_true")]
    pub reopen_closed_apps: bool,
    /// Surveille les lancements de jeux (Steam / Epic / choisis).
    #[serde(default = "default_true")]
    pub watch_games: bool,
    /// Exe en auto-activation (sans demander).
    #[serde(default)]
    pub auto_game_exes: Vec<String>,
    /// Jeux ajoutés manuellement depuis les process ouverts.
    #[serde(default)]
    pub extra_games: Vec<games::ExtraGame>,
    #[serde(default)]
    pub overlay: OverlayConfig,
}

fn default_services() -> Vec<String> {
    vec![
        "SysMain".into(),
        "WSearch".into(),
        "DiagTrack".into(),
    ]
}

fn default_min_close_mb() -> u64 {
    40
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
            ultimate_performance: false,
            protect_foreground: true,
            stop_services: false,
            services: default_services(),
            minimize_on_activate: true,
            start_with_windows: false,
            disable_game_dvr: true,
            enable_game_mode: true,
            disable_notifications: true,
            visual_effects_perf: false,
            disable_transparency: true,
            disable_widgets: true,
            disable_background_apps: true,
            mmcss_gaming: true,
            cpu_unpark: true,
            cpu_boost: true,
            focus_assist: true,
            gpu_high_perf: true,
            min_close_mb: default_min_close_mb(),
            reopen_closed_apps: true,
            watch_games: true,
            auto_game_exes: Vec::new(),
            extra_games: Vec::new(),
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
    #[serde(default)]
    pub tweaks: tweaks::TweaksSnapshot,
    #[serde(default)]
    pub boost: boost::BoostSnapshot,
    /// Identifiant du boot Windows au moment de l'activation (epoch sec du démarrage).
    /// Si le PC a redémarré, la session est considérée périmée.
    #[serde(default)]
    pub boot_id: Option<u64>,
}

/// Timestamp approximatif du dernier boot (secondes epoch).
fn current_boot_id() -> u64 {
    #[cfg(windows)]
    {
        use windows::Win32::System::SystemInformation::GetTickCount64;
        let uptime_ms = unsafe { GetTickCount64() };
        let now = std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .unwrap_or_default()
            .as_secs();
        return now.saturating_sub(uptime_ms / 1000);
    }
    #[cfg(not(windows))]
    {
        0
    }
}

fn boot_ids_match(stored: u64, current: u64) -> bool {
    stored.abs_diff(current) <= 3
}

/// Vérifie que « actif » correspond encore à ce boot. Sinon nettoie la session.
fn sanitize_session(app: &AppHandle) -> Session {
    let session = read_json::<Session>(app, "session.json");
    if !session.active {
        SESSION_ON.store(false, Ordering::SeqCst);
        return session;
    }

    let boot = current_boot_id();
    let still_valid = session
        .boot_id
        .is_some_and(|id| boot_ids_match(id, boot));

    if still_valid {
        SESSION_ON.store(true, Ordering::SeqCst);
        return session;
    }

    // Session fantôme (reboot, crash ancien format, etc.) :
    // on annule les tweaks système, sans relancer les apps (boot déjà passé).
    boost::restore(&session.boost);
    tweaks::restore(&session.tweaks);
    SESSION_ON.store(false, Ordering::SeqCst);
    if session.tweaks.previous_plan.is_none() {
        if let Some(plan) = &session.previous_plan {
            run("powercfg", &["/setactive", plan]);
        }
        for s in &session.stopped_services {
            run("sc", &["start", s]);
        }
    }

    let empty = Session::default();
    write_json(app, "session.json", &empty);
    empty
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

static CONFIG_CACHE: Mutex<Option<Config>> = Mutex::new(None);

fn load_config(app: &AppHandle) -> Config {
    if let Ok(guard) = CONFIG_CACHE.lock() {
        if let Some(c) = guard.as_ref() {
            return c.clone();
        }
    }
    let c = read_json::<Config>(app, "config.json");
    if let Ok(mut guard) = CONFIG_CACHE.lock() {
        *guard = Some(c.clone());
    }
    c
}

fn store_config(app: &AppHandle, config: &Config) {
    write_json(app, "config.json", config);
    if let Ok(mut guard) = CONFIG_CACHE.lock() {
        *guard = Some(config.clone());
    }
}

pub(crate) fn is_critical_name(name: &str) -> bool {
    is_critical(name)
}

fn is_critical(name: &str) -> bool {
    CRITICAL.contains(&name.to_lowercase().as_str())
}

pub(crate) fn is_safe_by_default(name: &str, path: &Option<String>) -> bool {
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

static PROC_SYS: Mutex<Option<System>> = Mutex::new(None);

fn with_proc_sys<R>(f: impl FnOnce(&mut System) -> R) -> R {
    let mut guard = PROC_SYS.lock().unwrap_or_else(|e| e.into_inner());
    if guard.is_none() {
        *guard = Some(System::new());
    }
    f(guard.as_mut().unwrap())
}

fn scan(cfg: &Config, with_cpu: bool) -> Vec<ProcGroup> {
    with_proc_sys(|sys| {
        // CPU% seulement sur l'onglet Apps ; à l'activation on n'a besoin que de la RAM.
        sys.refresh_processes(ProcessesToUpdate::All, with_cpu);

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
                cpu_pct: 0.0,
                instances: 0,
                path: path.clone(),
                protected,
                kept: cfg.keep.contains(&key),
                foreground: false,
            });
            entry.memory_mb += proc_.memory() / 1_048_576;
            if with_cpu {
                entry.cpu_pct += proc_.cpu_usage();
            }
            entry.instances += 1;
            if entry.path.is_none() {
                entry.path = path;
            }
            if Some(pid.as_u32()) == fg {
                entry.foreground = true;
            }
        }

        let mut v: Vec<ProcGroup> = map.into_values().filter(|p| p.memory_mb > 0).collect();
        v.sort_by(|a, b| b.memory_mb.cmp(&a.memory_mb));
        v
    })
}

struct ProcCache {
    at: Instant,
    keep_sig: u64,
    groups: Vec<ProcGroup>,
}

static PROC_CACHE: Mutex<Option<ProcCache>> = Mutex::new(None);

fn keep_signature(keep: &[String]) -> u64 {
    use std::collections::hash_map::DefaultHasher;
    use std::hash::{Hash, Hasher};
    let mut h = DefaultHasher::new();
    keep.hash(&mut h);
    h.finish()
}

fn list_processes_cached(cfg: &Config) -> Vec<ProcGroup> {
    let sig = keep_signature(&cfg.keep);
    if let Ok(guard) = PROC_CACHE.lock() {
        if let Some(c) = guard.as_ref() {
            // Cache plus long : la liste UI n'a pas besoin d'être temps réel.
            if c.keep_sig == sig && c.at.elapsed() < Duration::from_secs(12) {
                return c.groups.clone();
            }
        }
    }
    let groups = scan(cfg, false);
    if let Ok(mut guard) = PROC_CACHE.lock() {
        *guard = Some(ProcCache {
            at: Instant::now(),
            keep_sig: sig,
            groups: groups.clone(),
        });
    }
    groups
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
    let session = sanitize_session(app);
    overlay::sync_overlay(app, &cfg.overlay, session.active);
    overlay::emit_config(app, &cfg.overlay, session.active);
}

#[tauri::command]
fn get_config(app: AppHandle) -> Config {
    load_config(&app)
}

#[tauri::command]
fn save_config(app: AppHandle, config: Config) {
    store_config(&app, &config);
    sync_from_config(&app, &config);
}

#[tauri::command]
fn ensure_game_watch(app: AppHandle) {
    let cfg = load_config(&app);
    if cfg.watch_games {
        games::start_watcher(app);
    }
}

#[tauri::command]
fn get_session(app: AppHandle) -> Session {
    sanitize_session(&app)
}

#[tauri::command]
fn list_processes(app: AppHandle) -> Vec<ProcGroup> {
    list_processes_cached(&load_config(&app))
}

#[tauri::command]
fn list_monitors(app: AppHandle) -> Result<Vec<overlay::MonitorInfo>, String> {
    overlay::list_monitors(&app)
}

#[tauri::command]
fn get_metrics(app: AppHandle) -> metrics::Metrics {
    let cfg = load_config(&app);
    overlay::get_metrics(&app, &cfg.overlay)
}

#[tauri::command]
fn get_overlay_config(app: AppHandle) -> OverlayPayload {
    let cfg = load_config(&app);
    let session = sanitize_session(&app);
    overlay::payload(&app, &cfg.overlay, session.active)
}

#[tauri::command]
fn toggle_overlay_visibility(app: AppHandle) -> OverlayConfig {
    let mut cfg = load_config(&app);
    overlay::toggle_hidden(&mut cfg.overlay);
    store_config(&app, &cfg);
    sync_from_config(&app, &cfg);
    cfg.overlay
}

#[tauri::command]
fn list_games(app: AppHandle) -> Vec<games::GameEntry> {
    let cfg = load_config(&app);
    with_proc_sys(|sys| {
        sys.refresh_processes(ProcessesToUpdate::All, false);
        games::list_games(sys, &cfg.extra_games)
    })
}

#[tauri::command]
fn list_installed() -> Vec<software::InstalledApp> {
    software::list_installed()
}

#[tauri::command]
fn uninstall_app(id: String) -> Result<(), String> {
    software::uninstall(&id)
}

#[tauri::command]
fn open_install_folder(id: String) -> Result<(), String> {
    software::open_install_folder(&id)
}

#[tauri::command]
fn analyze_open_apps(app: AppHandle) -> software::AnalyzeReport {
    software::analyze(&scan(&load_config(&app), false))
}

#[tauri::command]
fn list_disks() -> Vec<disks::PhysicalDisk> {
    disks::list_disks()
}

#[tauri::command]
fn optimize_volume(letter: String) -> Result<disks::DiskJob, String> {
    disks::optimize_volume(&letter)
}

#[tauri::command]
fn analyze_fragmentation(letter: String) -> Result<disks::DiskJob, String> {
    disks::analyze_fragmentation(&letter)
}

#[tauri::command]
fn improve_volume(letter: String) -> Result<disks::DiskJob, String> {
    disks::improve_volume(&letter)
}

fn run(cmd: &str, args: &[&str]) -> Option<String> {
    let mut c = Command::new(cmd);
    c.args(args);
    #[cfg(windows)]
    c.creation_flags(NO_WINDOW);
    c.output().ok().map(|o| String::from_utf8_lossy(&o.stdout).to_string())
}

fn game_gpu_paths(groups: &[ProcGroup], cfg: &Config) -> Vec<String> {
    if !cfg.gpu_high_perf {
        return Vec::new();
    }
    let mut wanted: HashSet<String> = cfg
        .auto_game_exes
        .iter()
        .chain(cfg.extra_games.iter().map(|g| &g.exe))
        .map(|exe| {
            let mut name = exe.to_lowercase();
            if !name.ends_with(".exe") {
                name.push_str(".exe");
            }
            name
        })
        .collect();
    if let Some(fg) = groups.iter().find(|g| g.foreground) {
        if fg.path.as_deref().is_some_and(games::looks_like_game_path) {
            wanted.insert(fg.key.clone());
        }
    }
    let keys: Vec<String> = wanted.iter().cloned().collect();
    let mut seen = HashSet::new();
    let mut paths = Vec::new();
    for group in groups {
        if !wanted.contains(&group.key) {
            continue;
        }
        if let Some(path) = &group.path {
            if seen.insert(path.to_lowercase()) {
                paths.push(path.clone());
            }
        }
    }
    for path in games::installed_paths_for(&keys) {
        if seen.insert(path.to_lowercase()) {
            paths.push(path);
        }
    }
    paths
}

#[tauri::command]
fn activate(app: AppHandle) -> Result<Session, String> {
    let cfg = load_config(&app);
    let groups = scan(&cfg, false);

    let mut session = Session {
        active: true,
        boot_id: Some(current_boot_id()),
        ..Default::default()
    };

    let snap = tweaks::apply(&tweaks::TweakFlags {
        high_performance: cfg.high_performance,
        ultimate_performance: cfg.ultimate_performance,
        disable_game_dvr: cfg.disable_game_dvr,
        enable_game_mode: cfg.enable_game_mode,
        disable_notifications: cfg.disable_notifications,
        visual_effects_perf: cfg.visual_effects_perf,
        disable_transparency: cfg.disable_transparency,
        disable_widgets: cfg.disable_widgets,
        disable_background_apps: cfg.disable_background_apps,
        mmcss_gaming: cfg.mmcss_gaming,
        cpu_unpark: cfg.cpu_unpark,
        cpu_boost: cfg.cpu_boost,
        focus_assist: cfg.focus_assist,
        gpu_high_perf: cfg.gpu_high_perf,
        gpu_paths: game_gpu_paths(&groups, &cfg),
        stop_services: cfg.stop_services,
        services: cfg.services.clone(),
    });
    session.previous_plan = snap.previous_plan.clone();
    session.stopped_services = snap.stopped_services.clone();
    session.tweaks = snap;

    let min_mb = cfg.min_close_mb.max(0);
    let targets: Vec<&ProcGroup> = groups
        .iter()
        .filter(|g| {
            !g.protected
                && !g.kept
                && !is_safe_by_default(&g.key, &g.path)
                && !(cfg.protect_foreground && g.foreground)
                && g.memory_mb >= min_mb
        })
        .collect();

    // Réutilise le snapshot déjà rafraîchi par scan() — pas de 2e scan complet.
    with_proc_sys(|sys| {
        for g in &targets {
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
    });

    if let Ok(mut guard) = PROC_CACHE.lock() {
        *guard = None;
    }

    let mut game_exes = cfg.auto_game_exes.clone();
    game_exes.extend(cfg.extra_games.iter().map(|g| g.exe.clone()));
    if let Some(fg) = groups.iter().find(|g| g.foreground) {
        game_exes.push(fg.key.clone());
    }
    session.boost = boost::apply(&cfg.keep, &game_exes);
    SESSION_ON.store(true, Ordering::SeqCst);

    write_json(&app, "session.json", &session);
    overlay::sync_overlay(&app, &cfg.overlay, true);
    overlay::emit_config(&app, &cfg.overlay, true);
    let _ = app.emit("session-changed", &session);
    // Mode jeu : UI principale hors écran, sampler overlay seulement si besoin.
    sleep_main_ui(&app);
    Ok(session)
}

#[tauri::command]
fn restore(app: AppHandle) -> Result<Session, String> {
    let empty = restore_session(&app);
    let cfg = load_config(&app);
    overlay::sync_overlay(&app, &cfg.overlay, false);
    overlay::emit_config(&app, &cfg.overlay, false);
    let _ = app.emit("session-changed", &empty);
    show_main(&app);
    Ok(empty)
}

fn restore_session(app: &AppHandle) -> Session {
    let session = read_json::<Session>(app, "session.json");
    let cfg = load_config(app);
    boost::restore(&session.boost);
    SESSION_ON.store(false, Ordering::SeqCst);

    if cfg.reopen_closed_apps {
        for path in &session.closed {
            let mut c = Command::new(path);
            #[cfg(windows)]
            c.creation_flags(NO_WINDOW);
            let _ = c.spawn();
        }
    }

    tweaks::restore(&session.tweaks);
    if session.tweaks.previous_plan.is_none() {
        if let Some(plan) = &session.previous_plan {
            run("powercfg", &["/setactive", plan]);
        }
        if session.tweaks.stopped_services.is_empty() {
            for s in &session.stopped_services {
                run("sc", &["start", s]);
            }
        }
    }

    let empty = Session::default();
    write_json(app, "session.json", &empty);
    empty
}

fn refresh_overlay(app: &AppHandle) {
    let cfg = load_config(app);
    let active = read_json::<Session>(app, "session.json").active;
    overlay::sync_overlay(app, &cfg.overlay, active);
    overlay::emit_config(app, &cfg.overlay, active);
}

fn show_main(app: &AppHandle) {
    overlay::set_main_background(false);
    overlay::clear_notch_snooze();
    if let Some(w) = app.get_webview_window("main") {
        let _ = w.unminimize();
        let _ = w.show();
        let _ = w.set_focus();
    }
    refresh_overlay(app);
}

#[tauri::command]
fn open_main(app: AppHandle) {
    show_main(&app);
}

#[tauri::command]
fn hide_notch(app: AppHandle) {
    overlay::snooze_notch();
    refresh_overlay(&app);
}

#[tauri::command]
fn get_media() -> media::MediaNow {
    media::now()
}

#[tauri::command]
fn media_toggle() -> media::MediaNow {
    media::toggle()
}

#[tauri::command]
fn shelf_list() -> Vec<shelf::ShelfItem> {
    shelf::list()
}

#[tauri::command]
fn shelf_add(app: AppHandle, paths: Vec<String>) -> Vec<shelf::ShelfItem> {
    let items = shelf::add_paths(paths.into_iter().map(PathBuf::from).collect());
    shelf::set_drag_hover(false);
    shelf::emit(&app);
    refresh_overlay(&app);
    items
}

#[tauri::command]
fn shelf_remove(app: AppHandle, id: u64) -> Vec<shelf::ShelfItem> {
    let items = shelf::remove(id);
    shelf::emit(&app);
    refresh_overlay(&app);
    items
}

#[tauri::command]
fn shelf_clear(app: AppHandle) -> Vec<shelf::ShelfItem> {
    let items = shelf::clear();
    shelf::emit(&app);
    refresh_overlay(&app);
    items
}

#[tauri::command]
fn shelf_drag(app: AppHandle, id: u64) -> Result<(), String> {
    shelf::begin_drag(&app, id)
}

fn handle_overlay_drop(app: &AppHandle, ev: &DragDropEvent) {
    let cfg = load_config(app);
    let active = read_json::<Session>(app, "session.json").active;
    if !overlay::should_show_notch(app, &cfg.overlay, active) {
        return;
    }
    match ev {
        DragDropEvent::Enter { .. } => {
            if shelf::set_drag_hover(true) {
                refresh_overlay(app);
            }
            let _ = app.emit("notch-drag", true);
        }
        DragDropEvent::Over { .. } => {}
        DragDropEvent::Leave => {
            if shelf::set_drag_hover(false) {
                refresh_overlay(app);
            }
            let _ = app.emit("notch-drag", false);
        }
        DragDropEvent::Drop { paths, .. } => {
            shelf::set_drag_hover(false);
            shelf::add_paths(paths.clone());
            shelf::emit(app);
            let _ = app.emit("notch-drag", false);
            refresh_overlay(app);
        }
        _ => {}
    }
}

/// Endort le WebView principal pendant une session : moins de CPU/RAM pendant le jeu.
fn sleep_main_ui(app: &AppHandle) {
    overlay::set_main_background(true);
    if let Some(w) = app.get_webview_window("main") {
        let _ = w.minimize();
        let _ = w.hide();
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
    // Menu court, libellés ASCII-safe : évite certains bugs de rendu du menu
    // contextuel Windows (clic droit icône tray / barre des tâches).
    let open = MenuItem::with_id(app, "open", "Ouvrir", true, None::<&str>)?;
    let overlay_toggle =
        MenuItem::with_id(app, "overlay", "Overlay on/off", true, None::<&str>)?;
    let quit = MenuItem::with_id(app, "quit", "Quitter", true, None::<&str>)?;
    let menu = Menu::with_items(app, &[&open, &overlay_toggle, &quit])?;

    let mut builder = TrayIconBuilder::with_id("mode-jeu-tray")
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
            match event {
                TrayIconEvent::Click {
                    button: MouseButton::Left,
                    button_state: MouseButtonState::Up,
                    ..
                }
                | TrayIconEvent::DoubleClick {
                    button: MouseButton::Left,
                    ..
                } => show_main(tray.app_handle()),
                _ => {}
            }
        });

    if let Some(icon) = app.default_window_icon() {
        builder = builder.icon(icon.clone());
    }

    builder.build(app)?;
    Ok(())
}

#[derive(Serialize, Clone)]
pub struct SystemInfo {
    pub cpu: String,
    pub gpu: String,
    pub ram_gb: f32,
    pub tips: Vec<String>,
}

static SYSINFO_CACHE: Mutex<Option<SystemInfo>> = Mutex::new(None);

#[tauri::command]
fn get_system_info() -> SystemInfo {
    if let Ok(guard) = SYSINFO_CACHE.lock() {
        if let Some(info) = guard.as_ref() {
            return info.clone();
        }
    }

    let cpu = run(
        "powershell",
        &[
            "-NoProfile",
            "-NonInteractive",
            "-Command",
            "(Get-CimInstance Win32_Processor).Name",
        ],
    )
    .and_then(|s| s.lines().next().map(|l| l.trim().to_string()))
    .filter(|s| !s.is_empty())
    .unwrap_or_else(|| "CPU inconnu".into());

    let gpu = run(
        "powershell",
        &[
            "-NoProfile",
            "-NonInteractive",
            "-Command",
            "(Get-CimInstance Win32_VideoController | Select-Object -First 1 -ExpandProperty Name)",
        ],
    )
    .and_then(|s| s.lines().next().map(|l| l.trim().to_string()))
    .filter(|s| !s.is_empty())
    .unwrap_or_else(|| "GPU inconnu".into());

    let ram_gb = run(
        "powershell",
        &[
            "-NoProfile",
            "-NonInteractive",
            "-Command",
            "[math]::Round(((Get-CimInstance Win32_PhysicalMemory | Measure-Object Capacity -Sum).Sum/1GB),1)",
        ],
    )
    .and_then(|s| s.lines().find_map(|l| l.trim().parse::<f32>().ok()))
    .unwrap_or(0.0);

    let mut tips = Vec::new();
    let cpu_l = cpu.to_lowercase();
    let gpu_l = gpu.to_lowercase();

    if ram_gb > 0.0 && ram_gb <= 16.5 {
        tips.push(
            "16 Go RAM : ferme navigateurs et apps lourdes ; garde Discord seulement si besoin."
                .into(),
        );
    }
    if gpu_l.contains("radeon") || gpu_l.contains("amd") {
        tips.push(
            "GPU AMD : Adrenalin à jour ; Anti-Lag/Chill off en compétitif pour max FPS.".into(),
        );
        tips.push("Coupe Game DVR + Widgets : overhead inutile sur une 7900 XTX.".into());
    }
    if cpu_l.contains("ryzen") {
        tips.push(
            "Ryzen : Ultimate Performance + Game Mode = gains stables sans overclock.".into(),
        );
    }
    tips.push("MMCSS gaming : plus de budget CPU au jeu (admin recommandé).".into());
    tips.push("Seuil 40 Mo : ignore les petits helpers inutiles à fermer.".into());

    let info = SystemInfo {
        cpu: cpu.trim().to_string(),
        gpu: gpu.trim().to_string(),
        ram_gb,
        tips,
    };
    if let Ok(mut guard) = SYSINFO_CACHE.lock() {
        *guard = Some(info.clone());
    }
    info
}

#[cfg_attr(mobile, tauri::mobile_entry_point)]
pub fn run_app() {
    tauri::Builder::default()
        .plugin(tauri_plugin_process::init())
        .setup(|app| {
            #[cfg(windows)]
            app.handle()
                .plugin(tauri_plugin_updater::Builder::new().build())?;
            // Overlay hors barre des tâches / aperçus (évite le glitch au clic droit).
            if let Some(w) = app.get_webview_window("overlay") {
                let _ = w.set_skip_taskbar(true);
                let _ = w.set_title("");
                let _ = w.hide();
            }
            setup_tray(app.handle())?;
            // Invalide une session « active » fantôme (ex. après reboot).
            let _ = sanitize_session(app.handle());
            let cfg = load_config(app.handle());
            sync_from_config(app.handle(), &cfg);
            // Watcher jeux : seulement si l’utilisateur a déjà choisi des autos.
            if cfg.watch_games && !cfg.auto_game_exes.is_empty() {
                games::start_watcher(app.handle().clone());
            }
            Ok(())
        })
        .on_window_event(|window, event| {
            if window.label() == "overlay" {
                match event {
                    WindowEvent::CloseRequested { api, .. } => {
                        api.prevent_close();
                        let _ = window.hide();
                    }
                    WindowEvent::DragDrop(ev) => {
                        handle_overlay_drop(window.app_handle(), ev);
                    }
                    _ => {}
                }
                return;
            }
            if window.label() != "main" {
                return;
            }
            match event {
                WindowEvent::CloseRequested { api, .. } => {
                    api.prevent_close();
                    let _ = window.minimize();
                    overlay::set_main_background(true);
                    refresh_overlay(window.app_handle());
                }
                WindowEvent::Resized(_) => {
                    // Ne remet jamais « visible » ici : après Fermer, Windows
                    // envoie un resize avant d’être vraiment minimisé.
                    let bg = window.is_minimized().unwrap_or(false)
                        || !window.is_visible().unwrap_or(true);
                    if bg && overlay::set_main_background(true) {
                        refresh_overlay(window.app_handle());
                    }
                }
                WindowEvent::Focused(true) => {
                    let bg = window.is_minimized().unwrap_or(false)
                        || !window.is_visible().unwrap_or(true);
                    if overlay::set_main_background(bg) {
                        refresh_overlay(window.app_handle());
                    }
                }
                _ => {}
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
            open_main,
            hide_notch,
            get_media,
            media_toggle,
            shelf_list,
            shelf_add,
            shelf_remove,
            shelf_clear,
            shelf_drag,
            get_system_info,
            list_games,
            list_installed,
            uninstall_app,
            open_install_folder,
            analyze_open_apps,
            list_disks,
            optimize_volume,
            analyze_fragmentation,
            improve_volume,
            ensure_game_watch,
            activate,
            restore
        ])
        .run(tauri::generate_context!())
        .expect("erreur au lancement de l'application");
}
