//! Inventaire des logiciels Windows, désinstallation propre, analyse des process ouverts.
use serde::Serialize;
use std::collections::HashSet;
use std::path::Path;
use std::process::Command;
use std::sync::Mutex;
use std::time::{Duration, Instant};
use winreg::enums::*;
use winreg::RegKey;

use super::ProcGroup;

#[cfg(windows)]
use std::os::windows::process::CommandExt;

const UNINSTALL_PATHS: &[(&str, bool)] = &[
    (r"SOFTWARE\Microsoft\Windows\CurrentVersion\Uninstall", false),
    (r"SOFTWARE\WOW6432Node\Microsoft\Windows\CurrentVersion\Uninstall", true),
];

/// Prompt intégré à l’analyseur (offline, règles conservatrices).
pub const ANALYZE_PROMPT: &str = "Pour chaque application ouverte : est-elle utile sur ce PC ? Lesquelles désinstaller d’urgence (bloat, essai antivirus, adware) ? Lesquelles ne pas désinstaller (drivers, launchers, sécurité) ? Lesquelles sont impossibles à désinstaller (Windows, services) ? En cas de doute : ne pas désinstaller.";

#[derive(Serialize, Clone)]
pub struct InstalledApp {
    pub id: String,
    pub name: String,
    pub publisher: String,
    pub version: String,
    pub size_mb: u64,
    pub location: Option<String>,
    pub uninstall: Option<String>,
    pub quiet_uninstall: Option<String>,
    pub can_uninstall: bool,
    pub locked: bool,
    pub source: String,
}

#[derive(Serialize, Clone)]
pub struct AdviceItem {
    pub id: String,
    pub name: String,
    pub exe: String,
    pub publisher: String,
    pub reason: String,
    pub memory_mb: u64,
    pub can_uninstall: bool,
}

#[derive(Serialize, Clone)]
pub struct AnalyzeReport {
    pub prompt: String,
    pub scanned: usize,
    pub urgent: Vec<AdviceItem>,
    pub useful: Vec<AdviceItem>,
    pub keep: Vec<AdviceItem>,
    pub locked: Vec<AdviceItem>,
}

static INSTALLED_CACHE: Mutex<Option<(Instant, Vec<InstalledApp>)>> = Mutex::new(None);

fn hive_lm() -> RegKey {
    RegKey::predef(HKEY_LOCAL_MACHINE)
}

fn hive_cu() -> RegKey {
    RegKey::predef(HKEY_CURRENT_USER)
}

fn dword(k: &RegKey, name: &str) -> u32 {
    k.get_value::<u32, _>(name).unwrap_or(0)
}

fn str_val(k: &RegKey, name: &str) -> String {
    k.get_value::<String, _>(name).unwrap_or_default()
}

fn is_kb_noise(name: &str) -> bool {
    let n = name.to_lowercase();
    n.contains("kb") && n.chars().filter(|c| c.is_ascii_digit()).count() >= 6
        || n.starts_with("security update")
        || n.starts_with("update for")
        || n.starts_with("hotfix")
        || n.contains("service pack")
}

fn locked_install(name: &str, publisher: &str, location: &str, no_remove: bool) -> bool {
    if no_remove {
        return true;
    }
    let n = name.to_lowercase();
    let p = publisher.to_lowercase();
    let loc = location.to_lowercase();
    if loc.contains(r"\windows\") || loc.contains(r"\windowsapps\") {
        return true;
    }
    if n.contains("microsoft visual c++")
        || n.contains("microsoft .net")
        || n.contains("webview")
        || n.contains("directx")
        || n.contains("mode jeu")
    {
        return true;
    }
    if (p.contains("microsoft") && (n.contains("windows") || n.contains("edge update") || n.contains("vc++")))
        || p.contains("windows publisher")
    {
        return true;
    }
    false
}

fn read_key(hive_label: &str, parent: &RegKey, key_name: &str, wow: bool) -> Option<InstalledApp> {
    let k = parent.open_subkey(key_name).ok()?;
    let name = str_val(&k, "DisplayName");
    if name.trim().is_empty() || is_kb_noise(&name) {
        return None;
    }
    if dword(&k, "SystemComponent") == 1 {
        return None;
    }
    let publisher = str_val(&k, "Publisher");
    let version = str_val(&k, "DisplayVersion");
    let location = str_val(&k, "InstallLocation");
    let uninstall = str_val(&k, "UninstallString");
    let quiet = str_val(&k, "QuietUninstallString");
    let no_remove = dword(&k, "NoRemove") == 1;
    let size_kb = dword(&k, "EstimatedSize") as u64;
    let loc_opt = if location.trim().is_empty() {
        None
    } else {
        Some(location.clone())
    };
    let locked = locked_install(&name, &publisher, &location, no_remove);
    let can = !locked && (!uninstall.trim().is_empty() || !quiet.trim().is_empty());
    Some(InstalledApp {
        id: format!("{hive_label}|{key_name}|{}", if wow { "32" } else { "64" }),
        name,
        publisher,
        version,
        size_mb: size_kb / 1024,
        location: loc_opt,
        uninstall: nonempty(uninstall),
        quiet_uninstall: nonempty(quiet),
        can_uninstall: can,
        locked,
        source: if wow { "Win32" } else { "Win64" }.into(),
    })
}

fn nonempty(s: String) -> Option<String> {
    let t = s.trim().to_string();
    if t.is_empty() {
        None
    } else {
        Some(t)
    }
}

fn collect_hive(hive: RegKey, label: &str) -> Vec<InstalledApp> {
    let mut out = Vec::new();
    for (path, wow) in UNINSTALL_PATHS {
        let Ok(parent) = hive.open_subkey(path) else {
            continue;
        };
        for name in parent.enum_keys().filter_map(|x| x.ok()) {
            if let Some(app) = read_key(label, &parent, &name, *wow) {
                out.push(app);
            }
        }
    }
    out
}

pub fn list_installed() -> Vec<InstalledApp> {
    let now = Instant::now();
    if let Ok(g) = INSTALLED_CACHE.lock() {
        if let Some((at, cached)) = g.as_ref() {
            if now.duration_since(*at) < Duration::from_secs(45) {
                return cached.clone();
            }
        }
    }
    let mut apps = collect_hive(hive_lm(), "HKLM");
    apps.extend(collect_hive(hive_cu(), "HKCU"));
    let mut seen = HashSet::new();
    apps.retain(|a| seen.insert(a.name.to_lowercase()));
    apps.sort_by(|a, b| a.name.to_lowercase().cmp(&b.name.to_lowercase()));
    if let Ok(mut g) = INSTALLED_CACHE.lock() {
        *g = Some((now, apps.clone()));
    }
    apps
}

fn invalidate_cache() {
    if let Ok(mut g) = INSTALLED_CACHE.lock() {
        *g = None;
    }
}

fn parse_cmdline(s: &str) -> Option<(String, Vec<String>)> {
    let t = s.trim();
    if t.is_empty() {
        return None;
    }
    let mut args: Vec<String> = Vec::new();
    let mut cur = String::new();
    let mut quote = false;
    for c in t.chars() {
        match c {
            '"' => quote = !quote,
            ' ' | '\t' if !quote => {
                if !cur.is_empty() {
                    args.push(std::mem::take(&mut cur));
                }
            }
            _ => cur.push(c),
        }
    }
    if !cur.is_empty() {
        args.push(cur);
    }
    if args.is_empty() {
        return None;
    }
    let exe = args.remove(0);
    Some((exe, args))
}

fn msi_product(uninstall: &str) -> Option<String> {
    let u = uninstall.to_lowercase();
    if !u.contains("msiexec") {
        return None;
    }
    for part in uninstall.split(|c: char| c == ' ' || c == '"') {
        let p = part.trim();
        if p.starts_with('{') && p.ends_with('}') {
            return Some(p.to_string());
        }
        if let Some(rest) = p.strip_prefix("/x") {
            if rest.starts_with('{') {
                return Some(rest.to_string());
            }
        }
        if let Some(rest) = p.strip_prefix("/i") {
            if rest.starts_with('{') {
                return Some(rest.to_string());
            }
        }
        if let Some(rest) = p.strip_prefix("/X") {
            if rest.starts_with('{') {
                return Some(rest.to_string());
            }
        }
    }
    None
}

pub fn uninstall(id: &str) -> Result<(), String> {
    let apps = {
        invalidate_cache();
        list_installed()
    };
    let app = apps
        .iter()
        .find(|a| a.id == id)
        .ok_or_else(|| "Logiciel introuvable.".to_string())?;
    if !app.can_uninstall || app.locked {
        return Err("Ce logiciel ne peut pas être désinstallé (Windows, driver ou protégé).".into());
    }
    let cmd = app
        .uninstall
        .as_deref()
        .or(app.quiet_uninstall.as_deref())
        .ok_or_else(|| "Pas de désinstalleur officiel.".to_string())?;

    if let Some(guid) = msi_product(cmd) {
        let mut c = Command::new("msiexec");
        c.args(["/x", &guid, "/passive", "/norestart"]);
        #[cfg(windows)]
        c.creation_flags(0);
        c.spawn()
            .map_err(|e| format!("Impossible de lancer msiexec : {e}"))?;
        invalidate_cache();
        return Ok(());
    }

    let (exe, args) = parse_cmdline(cmd).ok_or_else(|| "Commande de désinstallation invalide.".to_string())?;
    if !Path::new(&exe).exists() && !exe.to_lowercase().ends_with(".exe") {
        return Err("Désinstalleur introuvable sur le disque.".into());
    }
    let mut c = Command::new(&exe);
    c.args(&args);
    #[cfg(windows)]
    c.creation_flags(0);
    c.spawn()
        .map_err(|e| format!("Impossible de lancer le désinstalleur : {e}"))?;
    invalidate_cache();
    Ok(())
}

fn contains_any(hay: &str, needles: &[&str]) -> bool {
    needles.iter().any(|n| hay.contains(n))
}

fn is_driver_or_runtime(name: &str, publisher: &str, exe: &str) -> bool {
    let h = format!("{name} {publisher} {exe}").to_lowercase();
    contains_any(
        &h,
        &[
            "nvidia", "amd", "radeon", "realtek", "intel graphics", "chipset",
            "audio driver", "lan driver", "wifi", "bluetooth", "directx",
            "visual c++", ".net runtime", ".net desktop", "webview",
            "microsoft edge update", "windows security", "defender",
            "bitdefender", "kaspersky", "malwarebytes", "eset",
            "radeonsoftware", "amdow", "nvcontainer", "nvdisplay",
        ],
    )
}

fn is_launcher_or_game_stack(name: &str, publisher: &str, exe: &str) -> bool {
    let h = format!("{name} {publisher} {exe}").to_lowercase();
    contains_any(
        &h,
        &[
            "steam", "epic games", "battle.net", "blizzard", "ubisoft",
            "ea app", "ea desktop", "origin", "gog galaxy", "riot",
            "xbox", "game bar", "rivatuner", "msi afterburner", "rtss",
            "easy anticheat", "battleye", "vanguard",
        ],
    )
}

fn is_system_proc(exe: &str, path: &str) -> bool {
    let e = exe.to_lowercase();
    let p = path.to_lowercase();
    super::is_critical_name(&e)
        || p.contains(r"\windows\")
        || p.contains(r"\windowsapps\")
        || e == "mode-jeu.exe"
        || e == "msedgewebview2.exe"
        || e == "runtimebroker.exe"
}

fn is_urgent_bloat(name: &str, publisher: &str, exe: &str) -> bool {
    let h = format!("{name} {publisher} {exe}").to_lowercase();
    contains_any(
        &h,
        &[
            "mcafee", "norton", "avg antivirus", "avast", "pc app store",
            "wildtangent", "candy crush", "tiktok", "bubble witch",
            "disney+", "booking.com", "spotify web helper",
            "iobit", "driver booster", "advanced systemcare", "driverupdate",
            "toolbar", "coupon", "web companion", "opera news",
            "microsoft solitaire", "mixed reality", "feedback hub",
            "microsoft tips", "get started", "skype app", "cortana",
            "xbox console companion", "microsoft bing", "news app",
        ],
    )
}

fn match_installed<'a>(
    procs_name: &str,
    procs_path: &str,
    installed: &'a [InstalledApp],
) -> Option<&'a InstalledApp> {
    let n = procs_name.to_lowercase();
    let p = procs_path.to_lowercase();
    installed.iter().find(|a| {
        if let Some(loc) = &a.location {
            let l = loc.to_lowercase();
            if !l.is_empty() && p.starts_with(l.trim_end_matches('\\')) {
                return true;
            }
        }
        let dn = a.name.to_lowercase();
        dn.contains(&n) || (n.contains(&dn) && dn.len() > 4)
    })
}

pub fn analyze(procs: &[ProcGroup]) -> AnalyzeReport {
    let installed = list_installed();
    let mut urgent = Vec::new();
    let mut useful = Vec::new();
    let mut keep = Vec::new();
    let mut locked = Vec::new();
    let mut seen = HashSet::new();

    for g in procs {
        if !seen.insert(g.key.clone()) {
            continue;
        }
        let path = g.path.clone().unwrap_or_default();
        let inst = match_installed(&g.name, &path, &installed);
        let publisher = inst.map(|i| i.publisher.clone()).unwrap_or_default();
        let display = inst.map(|i| i.name.clone()).unwrap_or_else(|| g.name.clone());
        let can = inst.map(|i| i.can_uninstall).unwrap_or(false) && !g.protected;
        let id = inst
            .map(|i| i.id.clone())
            .unwrap_or_else(|| format!("proc:{}", g.key));

        let item = |reason: String, can_un: bool| AdviceItem {
            id: id.clone(),
            name: display.clone(),
            exe: g.key.clone(),
            publisher: publisher.clone(),
            reason,
            memory_mb: g.memory_mb,
            can_uninstall: can_un,
        };

        if is_system_proc(&g.key, &path) || g.protected {
            locked.push(item(
                "Composant Windows, service ou process protégé — désinstallation impossible.".into(),
                false,
            ));
            continue;
        }
        if is_driver_or_runtime(&display, &publisher, &g.key) {
            keep.push(item(
                "Driver, runtime ou sécurité : le retirer casse l’affichage, l’audio ou le jeu.".into(),
                false,
            ));
            continue;
        }
        if is_launcher_or_game_stack(&display, &publisher, &g.key) {
            keep.push(item(
                "Launcher, overlay ou anti-cheat : nécessaire pour lancer / protéger tes jeux.".into(),
                false,
            ));
            continue;
        }
        if is_urgent_bloat(&display, &publisher, &g.key) {
            urgent.push(item(
                "Bloat, essai antivirus ou app OEM inutile — candidat à une désinstall propre.".into(),
                can,
            ));
            continue;
        }
        useful.push(item(
            "Application utilisateur en cours : utile si tu t’en sers, sinon tu peux la fermer en mode jeu.".into(),
            can,
        ));
    }

    urgent.sort_by(|a, b| b.memory_mb.cmp(&a.memory_mb));
    useful.sort_by(|a, b| b.memory_mb.cmp(&a.memory_mb));
    keep.sort_by(|a, b| b.memory_mb.cmp(&a.memory_mb));
    locked.sort_by(|a, b| b.memory_mb.cmp(&a.memory_mb));

    AnalyzeReport {
        prompt: ANALYZE_PROMPT.into(),
        scanned: procs.len(),
        urgent,
        useful,
        keep,
        locked,
    }
}
