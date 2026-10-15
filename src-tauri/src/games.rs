//! Détection des jeux installés (Steam / Epic) et des process de jeu en cours.
use serde::{Deserialize, Serialize};
use std::collections::{HashMap, HashSet};
use std::fs;
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::Mutex;
use std::time::{Duration, Instant};
use sysinfo::System;
use tauri::{AppHandle, Emitter, Manager};

const SKIP_EXE: &[&str] = &[
    "crashhandler", "crashpad", "crashreporter", "unitycrash", "unins", "setup",
    "redist", "vcredist", "dxsetup", "easyanticheat", "eac", "battleye",
    "uninstall", "installer", "update", "overlay", "webhelper", "cef",
    "notification", "helper", "service",
];

#[derive(Serialize, Deserialize, Clone, Default)]
pub struct ExtraGame {
    pub exe: String,
    pub name: String,
}

#[derive(Serialize, Deserialize, Clone, PartialEq, Eq)]
pub struct GameEntry {
    pub id: String,
    pub name: String,
    pub exe: String,
    pub source: String,
    pub path: Option<String>,
    pub running: bool,
}

#[derive(Serialize, Clone)]
pub struct GameDetected {
    pub name: String,
    pub exe: String,
    pub source: String,
}

static WATCHER: AtomicBool = AtomicBool::new(false);
static SEEN: Mutex<Option<HashSet<String>>> = Mutex::new(None);
static LAST_PROMPT: Mutex<Option<(String, Instant)>> = Mutex::new(None);

fn skip_exe(name: &str) -> bool {
    let n = name.to_lowercase();
    SKIP_EXE.iter().any(|s| n.contains(s))
}

fn exe_key(name: &str) -> String {
    let mut n = name.to_lowercase();
    if !n.ends_with(".exe") {
        n.push_str(".exe");
    }
    n
}

fn vdf_values(content: &str, key: &str) -> Vec<String> {
    let needle = format!("\"{key}\"");
    let mut out = Vec::new();
    for line in content.lines() {
        let t = line.trim();
        let Some(rest) = t.strip_prefix(&needle) else {
            continue;
        };
        let rest = rest.trim();
        if let Some(start) = rest.find('"') {
            let rest = &rest[start + 1..];
            if let Some(end) = rest.find('"') {
                let v = rest[..end].replace("\\\\", "\\");
                if !v.is_empty() {
                    out.push(v);
                }
            }
        }
    }
    out
}

fn vdf_first(content: &str, key: &str) -> Option<String> {
    vdf_values(content, key).into_iter().next()
}

fn steam_root() -> Option<PathBuf> {
    let out = super::run(
        "reg",
        &[
            "query",
            r"HKCU\Software\Valve\Steam",
            "/v",
            "SteamPath",
        ],
    )?;
    for line in out.lines() {
        if line.contains("SteamPath") {
            if let Some(idx) = line.to_lowercase().find("reg_sz") {
                let p = line[idx + 6..].trim();
                if !p.is_empty() {
                    return Some(PathBuf::from(p));
                }
            }
        }
    }
    let def = PathBuf::from(r"C:\Program Files (x86)\Steam");
    def.exists().then_some(def)
}

fn steam_libraries(root: &Path) -> Vec<PathBuf> {
    let mut libs = vec![root.to_path_buf()];
    let vdf = root.join("steamapps").join("libraryfolders.vdf");
    if let Ok(txt) = fs::read_to_string(&vdf) {
        for p in vdf_values(&txt, "path") {
            let pb = PathBuf::from(p);
            if pb.exists() && !libs.iter().any(|x| x == &pb) {
                libs.push(pb);
            }
        }
    }
    libs
}

fn pick_game_exe(dir: &Path) -> Option<(String, PathBuf)> {
    let mut best: Option<(u64, PathBuf)> = None;
    let mut stack = vec![dir.to_path_buf()];
    let mut visited = 0u32;
    while let Some(cur) = stack.pop() {
        visited += 1;
        if visited > 400 {
            break;
        }
        let Ok(rd) = fs::read_dir(&cur) else {
            continue;
        };
        for e in rd.flatten() {
            let p = e.path();
            if p.is_dir() {
                let name = p.file_name().and_then(|s| s.to_str()).unwrap_or("");
                if name.eq_ignore_ascii_case("redist")
                    || name.eq_ignore_ascii_case("engine")
                    || name.starts_with('.')
                {
                    continue;
                }
                stack.push(p);
                continue;
            }
            let Some(ext) = p.extension() else {
                continue;
            };
            if !ext.eq_ignore_ascii_case("exe") {
                continue;
            }
            let fname = p.file_name().and_then(|s| s.to_str()).unwrap_or("");
            if skip_exe(fname) {
                continue;
            }
            let size = e.metadata().map(|m| m.len()).unwrap_or(0);
            if size < 200_000 {
                continue;
            }
            if best.as_ref().map(|(s, _)| size > *s).unwrap_or(true) {
                best = Some((size, p));
            }
        }
    }
    best.map(|(_, p)| {
        let exe = p
            .file_name()
            .and_then(|s| s.to_str())
            .unwrap_or("game.exe")
            .to_lowercase();
        (exe, p)
    })
}

fn scan_steam() -> Vec<GameEntry> {
    let Some(root) = steam_root() else {
        return Vec::new();
    };
    let mut games = Vec::new();
    for lib in steam_libraries(&root) {
        let apps = lib.join("steamapps");
        let Ok(rd) = fs::read_dir(&apps) else {
            continue;
        };
        for e in rd.flatten() {
            let name = e.file_name();
            let ns = name.to_string_lossy();
            if !ns.starts_with("appmanifest_") || !ns.ends_with(".acf") {
                continue;
            }
            let Ok(txt) = fs::read_to_string(e.path()) else {
                continue;
            };
            let title = vdf_first(&txt, "name").unwrap_or_else(|| "Jeu Steam".into());
            let installdir = vdf_first(&txt, "installdir").unwrap_or_default();
            if installdir.is_empty() {
                continue;
            }
            let dir = apps.join("common").join(&installdir);
            if !dir.is_dir() {
                continue;
            }
            let Some((exe, path)) = pick_game_exe(&dir) else {
                continue;
            };
            games.push(GameEntry {
                id: format!("steam:{exe}"),
                name: title,
                exe,
                source: "Steam".into(),
                path: Some(path.to_string_lossy().to_string()),
                running: false,
            });
        }
    }
    games
}

fn scan_epic() -> Vec<GameEntry> {
    let manifests = PathBuf::from(r"C:\ProgramData\Epic\EpicGamesLauncher\Data\Manifests");
    let Ok(rd) = fs::read_dir(&manifests) else {
        return Vec::new();
    };
    let mut games = Vec::new();
    for e in rd.flatten() {
        let p = e.path();
        if p.extension().and_then(|s| s.to_str()) != Some("item") {
            continue;
        }
        let Ok(txt) = fs::read_to_string(&p) else {
            continue;
        };
        let Ok(v) = serde_json::from_str::<serde_json::Value>(&txt) else {
            continue;
        };
        let title = v
            .get("DisplayName")
            .and_then(|x| x.as_str())
            .unwrap_or("Jeu Epic");
        let loc = v.get("InstallLocation").and_then(|x| x.as_str()).unwrap_or("");
        let launch = v
            .get("LaunchExecutable")
            .and_then(|x| x.as_str())
            .unwrap_or("");
        if loc.is_empty() || launch.is_empty() {
            continue;
        }
        let full = PathBuf::from(loc).join(launch.replace('/', "\\"));
        let exe = full
            .file_name()
            .and_then(|s| s.to_str())
            .unwrap_or("game.exe")
            .to_lowercase();
        if skip_exe(&exe) {
            continue;
        }
        games.push(GameEntry {
            id: format!("epic:{exe}"),
            name: title.to_string(),
            exe,
            source: "Epic".into(),
            path: Some(full.to_string_lossy().to_string()),
            running: false,
        });
    }
    games
}

pub(crate) fn looks_like_game_path(path: &str) -> bool {
    let p = path.to_lowercase();
    p.contains("\\steamapps\\common\\")
        || p.contains("\\epic games\\")
        || p.contains("\\xboxgames\\")
        || p.contains("\\riot games\\")
        || p.contains("\\battle.net\\")
        || p.contains("\\blizzard")
        || p.contains("\\ubisoft\\")
        || p.contains("\\ea games\\")
        || p.contains("\\origin games\\")
        || p.contains("\\gog galaxy\\games\\")
}

fn running_exes(sys: &System) -> HashMap<String, String> {
    let mut map = HashMap::new();
    for (_, proc_) in sys.processes() {
        let name = proc_.name().to_string_lossy().to_lowercase();
        if name.is_empty() || skip_exe(&name) {
            continue;
        }
        let exe = exe_key(&name);
        let path = proc_
            .exe()
            .map(|p| p.to_string_lossy().to_string())
            .unwrap_or_default();
        map.insert(exe, path);
    }
    map
}

fn merge(
    entries: Vec<GameEntry>,
    extra: &[ExtraGame],
    running: &HashMap<String, String>,
) -> Vec<GameEntry> {
    let mut by_exe: HashMap<String, GameEntry> = HashMap::new();
    for mut g in entries {
        g.running = running.contains_key(&g.exe);
        by_exe
            .entry(g.exe.clone())
            .and_modify(|old| {
                if g.running {
                    old.running = true;
                }
                if old.path.is_none() {
                    old.path = g.path.clone();
                }
            })
            .or_insert(g);
    }
    for e in extra {
        let exe = exe_key(&e.exe);
        if exe.is_empty() {
            continue;
        }
        let running_now = running.contains_key(&exe);
        by_exe
            .entry(exe.clone())
            .and_modify(|old| {
                if running_now {
                    old.running = true;
                }
            })
            .or_insert(GameEntry {
                id: format!("user:{exe}"),
                name: if e.name.trim().is_empty() {
                    pretty_name(exe.trim_end_matches(".exe"))
                } else {
                    e.name.clone()
                },
                exe: exe.clone(),
                source: "Choisi".into(),
                path: running.get(&exe).cloned(),
                running: running_now,
            });
    }
    for (exe, path) in running {
        if by_exe.contains_key(exe) {
            continue;
        }
        if !looks_like_game_path(path) {
            continue;
        }
        let stem = exe.trim_end_matches(".exe");
        by_exe.insert(
            exe.clone(),
            GameEntry {
                id: format!("run:{exe}"),
                name: pretty_name(stem),
                exe: exe.clone(),
                source: "Ouvert".into(),
                path: Some(path.clone()),
                running: true,
            },
        );
    }
    let mut v: Vec<GameEntry> = by_exe.into_values().collect();
    v.sort_by(|a, b| b.running.cmp(&a.running).then(a.name.cmp(&b.name)));
    v
}

fn pretty_name(stem: &str) -> String {
    stem.replace(['_', '-'], " ")
        .split_whitespace()
        .map(|w| {
            let mut c = w.chars();
            match c.next() {
                Some(f) => f.to_uppercase().collect::<String>() + c.as_str(),
                None => String::new(),
            }
        })
        .collect::<Vec<_>>()
        .join(" ")
}

static CATALOG: Mutex<Option<(Instant, Vec<GameEntry>)>> = Mutex::new(None);

fn installed_catalog() -> Vec<GameEntry> {
    let now = Instant::now();
    if let Ok(guard) = CATALOG.lock() {
        if let Some((at, cached)) = guard.as_ref() {
            if now.duration_since(*at) < Duration::from_secs(90) {
                return cached.clone();
            }
        }
    }
    let mut installed = scan_steam();
    installed.extend(scan_epic());
    if let Ok(mut guard) = CATALOG.lock() {
        *guard = Some((now, installed.clone()));
    }
    installed
}

pub(crate) fn installed_paths_for(exes: &[String]) -> Vec<String> {
    let wanted: HashSet<String> = exes.iter().map(|e| exe_key(e)).collect();
    if wanted.is_empty() {
        return Vec::new();
    }
    installed_catalog()
        .into_iter()
        .filter(|g| wanted.contains(&g.exe))
        .filter_map(|g| g.path)
        .collect()
}

pub fn list_games(sys: &System, extra: &[ExtraGame]) -> Vec<GameEntry> {
    merge(installed_catalog(), extra, &running_exes(sys))
}

fn find_known_game(exe: &str, extra: &[ExtraGame]) -> Option<GameEntry> {
    let key = exe_key(exe);
    if let Some(e) = extra.iter().find(|e| exe_key(&e.exe) == key) {
        return Some(GameEntry {
            id: format!("user:{key}"),
            name: if e.name.trim().is_empty() {
                pretty_name(key.trim_end_matches(".exe"))
            } else {
                e.name.clone()
            },
            exe: key,
            source: "Choisi".into(),
            path: None,
            running: true,
        });
    }
    installed_catalog().into_iter().find(|g| g.exe == key)
}

pub fn start_watcher(app: AppHandle) {
    if WATCHER.swap(true, Ordering::SeqCst) {
        return;
    }
    std::thread::Builder::new()
        .name("game-watch".into())
        .spawn(move || {
            if let Ok(mut g) = SEEN.lock() {
                *g = Some(running_exe_names());
            }
            std::thread::sleep(Duration::from_secs(8));
            loop {
                if crate::session_is_active() {
                    std::thread::sleep(Duration::from_secs(12));
                    continue;
                }
                tick(&app);
                std::thread::sleep(Duration::from_secs(8));
            }
        })
        .ok();
}

fn tick(app: &AppHandle) {
    let cfg = super::load_config(app);
    if !cfg.watch_games {
        return;
    }
    let now_set = running_exe_names();
    let mut seen_guard = SEEN.lock().unwrap_or_else(|e| e.into_inner());
    let seen = seen_guard.get_or_insert_with(HashSet::new);
    let newcomers: Vec<String> = now_set.difference(seen).cloned().collect();
    *seen = now_set;
    drop(seen_guard);

    if newcomers.is_empty() {
        return;
    }

    let auto: HashSet<String> = cfg
        .auto_game_exes
        .iter()
        .map(|e| exe_key(e))
        .collect();

    for exe in &newcomers {
        if !auto.contains(exe) {
            continue;
        }
        if let Ok(session) = super::activate(app.clone()) {
            let _ = app.emit("session-changed", &session);
        }
        let _ = app.emit(
            "game-auto-activated",
            GameDetected {
                name: pretty_name(exe.trim_end_matches(".exe")),
                exe: exe.clone(),
                source: "Auto".into(),
            },
        );
        return;
    }

    for exe in newcomers {
        let Some(game) = find_known_game(&exe, &cfg.extra_games) else {
            continue;
        };
        if recently_prompted(&exe) {
            continue;
        }
        mark_prompted(&exe);
        let payload = GameDetected {
            name: game.name.clone(),
            exe: game.exe.clone(),
            source: game.source.clone(),
        };
        if let Some(w) = app.get_webview_window("main") {
            let _ = w.unminimize();
            let _ = w.show();
            let _ = w.set_focus();
        }
        let _ = app.emit("game-detected", &payload);
        return;
    }
}

fn running_exe_names() -> HashSet<String> {
    #[cfg(windows)]
    {
        use windows::Win32::Foundation::CloseHandle;
        use windows::Win32::System::Diagnostics::ToolHelp::{
            CreateToolhelp32Snapshot, Process32FirstW, Process32NextW, PROCESSENTRY32W,
            TH32CS_SNAPPROCESS,
        };
        let mut out = HashSet::new();
        unsafe {
            let Ok(snap) = CreateToolhelp32Snapshot(TH32CS_SNAPPROCESS, 0) else {
                return out;
            };
            let mut pe = PROCESSENTRY32W {
                dwSize: std::mem::size_of::<PROCESSENTRY32W>() as u32,
                ..Default::default()
            };
            if Process32FirstW(snap, &mut pe).is_ok() {
                loop {
                    let end = pe
                        .szExeFile
                        .iter()
                        .position(|&c| c == 0)
                        .unwrap_or(pe.szExeFile.len());
                    let name = String::from_utf16_lossy(&pe.szExeFile[..end]);
                    if !name.is_empty() {
                        out.insert(exe_key(&name));
                    }
                    if Process32NextW(snap, &mut pe).is_err() {
                        break;
                    }
                }
            }
            let _ = CloseHandle(snap);
        }
        out
    }
    #[cfg(not(windows))]
    {
        HashSet::new()
    }
}

fn recently_prompted(exe: &str) -> bool {
    if let Ok(g) = LAST_PROMPT.lock() {
        if let Some((e, at)) = g.as_ref() {
            if e == exe && at.elapsed() < Duration::from_secs(20 * 60) {
                return true;
            }
        }
    }
    false
}

fn mark_prompted(exe: &str) {
    if let Ok(mut g) = LAST_PROMPT.lock() {
        *g = Some((exe.to_string(), Instant::now()));
    }
}
