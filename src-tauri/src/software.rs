//! Inventaire des logiciels Windows, désinstallation propre, analyse des process ouverts.
use serde::Serialize;
use std::collections::{HashMap, HashSet};
use std::path::Path;
use std::sync::Mutex;
use std::time::{Duration, Instant};
use winreg::enums::*;
use winreg::RegKey;

use super::ProcGroup;

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
    /// Dossier à ouvrir, s’il est connu et n’est pas une racine générique.
    pub folder: Option<String>,
    /// Nom court du groupe si une autre entrée recouvre celle-ci. Vide sinon.
    pub duplicate_group: String,
    /// Autres noms du même groupe.
    pub duplicate_with: Vec<String>,
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
    let folder = guess_folder(&location, &uninstall);
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
        folder,
        duplicate_group: String::new(),
        duplicate_with: Vec::new(),
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

fn is_generic_dir(path: &str) -> bool {
    let p = path.trim().trim_end_matches(['\\', '/']).to_lowercase();
    if p.len() <= 3 {
        return true;
    }
    p.ends_with(r":\program files")
        || p.ends_with(r":\program files (x86)")
        || p.ends_with(r":\windows")
        || p.ends_with(r":\windows\system32")
        || p.ends_with(r":\users")
}

/// Clé de recouvrement : même produit, en ignorant l’architecture et un numéro de version final.
fn duplicate_key(name: &str) -> String {
    let mut n = name.to_lowercase();
    for ch in ['®', '™', '©', '(', ')', '[', ']'] {
        n = n.replace(ch, " ");
    }
    for tag in [
        "64-bit", "32-bit", "64 bits", "32 bits", "x64", "x86", "win64", "win32",
    ] {
        n = n.replace(tag, " ");
    }
    let mut parts: Vec<&str> = n.split_whitespace().collect();
    if let Some(last) = parts.last().copied() {
        let t = last.trim_start_matches('v');
        let dotted = t.contains('.') && t.chars().all(|c| c.is_ascii_digit() || c == '.');
        if dotted {
            parts.pop();
        }
    }
    parts.join(" ")
}

fn guess_folder(location: &str, uninstall: &str) -> Option<String> {
    let loc = location.trim().trim_matches('"');
    if !loc.is_empty() {
        let p = Path::new(loc);
        if p.is_dir() && !is_generic_dir(loc) {
            return Some(p.to_string_lossy().trim_end_matches(['\\', '/']).to_string());
        }
        if p.is_file() {
            if let Some(parent) = p.parent() {
                let s = parent.to_string_lossy();
                if parent.is_dir() && !is_generic_dir(&s) {
                    return Some(s.trim_end_matches(['\\', '/']).to_string());
                }
            }
        }
    }
    let u = uninstall.trim();
    if u.is_empty() || u.to_lowercase().contains("msiexec") {
        return None;
    }
    let (exe, _, dir) = resolve_command(u).ok()?;
    if exe.to_lowercase().contains("msiexec") {
        return None;
    }
    let candidate = dir.unwrap_or_else(|| {
        Path::new(&exe)
            .parent()
            .map(|p| p.to_string_lossy().into_owned())
            .unwrap_or_default()
    });
    if candidate.is_empty() || is_generic_dir(&candidate) || !Path::new(&candidate).is_dir() {
        return None;
    }
    Some(candidate.trim_end_matches(['\\', '/']).to_string())
}

fn find_root(parent: &mut [usize], mut i: usize) -> usize {
    while parent[i] != i {
        parent[i] = parent[parent[i]];
        i = parent[i];
    }
    i
}

fn union_root(parent: &mut [usize], a: usize, b: usize) {
    let ra = find_root(parent, a);
    let rb = find_root(parent, b);
    if ra != rb {
        parent[rb] = ra;
    }
}

fn annotate_duplicates(apps: &mut [InstalledApp]) {
    let n = apps.len();
    if n < 2 {
        return;
    }
    let mut parent: Vec<usize> = (0..n).collect();
    let mut by_key: HashMap<String, usize> = HashMap::new();
    let mut by_folder: HashMap<String, usize> = HashMap::new();
    for i in 0..n {
        let key = duplicate_key(&apps[i].name);
        if key.chars().count() >= 3 {
            if let Some(j) = by_key.get(&key).copied() {
                union_root(&mut parent, i, j);
            } else {
                by_key.insert(key, i);
            }
        }
        if let Some(folder) = &apps[i].folder {
            let f = folder.trim_end_matches(['\\', '/']).to_lowercase();
            if !is_generic_dir(&f) {
                if let Some(j) = by_folder.get(&f).copied() {
                    union_root(&mut parent, i, j);
                } else {
                    by_folder.insert(f, i);
                }
            }
        }
    }
    let mut groups: HashMap<usize, Vec<usize>> = HashMap::new();
    for i in 0..n {
        let root = find_root(&mut parent, i);
        groups.entry(root).or_default().push(i);
    }
    for members in groups.values() {
        if members.len() < 2 {
            continue;
        }
        let label = members
            .iter()
            .map(|&i| apps[i].name.clone())
            .min_by_key(|s| s.chars().count())
            .unwrap_or_default();
        for &i in members {
            apps[i].duplicate_group = label.clone();
            apps[i].duplicate_with = members
                .iter()
                .filter(|&&j| j != i)
                .map(|&j| apps[j].name.clone())
                .collect();
        }
    }
}

pub fn open_install_folder(id: &str) -> Result<(), String> {
    let apps = list_installed();
    let app = apps
        .iter()
        .find(|a| a.id == id)
        .ok_or_else(|| "Logiciel introuvable.".to_string())?;
    let folder = app
        .folder
        .clone()
        .filter(|p| !p.is_empty())
        .ok_or_else(|| "Dossier d’installation inconnu.".to_string())?;
    if !Path::new(&folder).is_dir() {
        return Err("Ce dossier n’existe plus.".into());
    }
    std::process::Command::new("explorer")
        .arg(&folder)
        .spawn()
        .map_err(|_| "Impossible d’ouvrir l’explorateur.".to_string())?;
    Ok(())
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
    annotate_duplicates(&mut apps);
    apps.sort_by(|a, b| {
        let ad = !a.duplicate_group.is_empty();
        let bd = !b.duplicate_group.is_empty();
        bd.cmp(&ad)
            .then_with(|| a.duplicate_group.to_lowercase().cmp(&b.duplicate_group.to_lowercase()))
            .then_with(|| a.name.to_lowercase().cmp(&b.name.to_lowercase()))
    });
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

fn guid_like(s: &str) -> bool {
    let t = s.trim();
    t.len() >= 38 && t.starts_with('{') && t.ends_with('}')
}

fn msi_product(uninstall: &str) -> Option<String> {
    let u = uninstall.to_lowercase();
    if !u.contains("msiexec") {
        return None;
    }
    for part in uninstall.split(|c: char| c == ' ' || c == '"' || c == ',') {
        let p = part.trim().trim_start_matches("/i").trim_start_matches("/x").trim_start_matches("/I").trim_start_matches("/X");
        let p = p.trim();
        if guid_like(p) {
            return Some(p.to_string());
        }
    }
    None
}

fn join_args(args: &[String]) -> String {
    args.iter()
        .map(|a| {
            if a.is_empty() {
                "\"\"".into()
            } else if a.contains(char::is_whitespace) && !a.starts_with('"') {
                format!("\"{a}\"")
            } else {
                a.clone()
            }
        })
        .collect::<Vec<_>>()
        .join(" ")
}

/// Recolle un UninstallString non quoté du type `C:\Program Files\App\unins000.exe /SILENT`.
fn resolve_command(cmd: &str) -> Result<(String, Vec<String>, Option<String>), String> {
    let (exe0, args0) = parse_cmdline(cmd).ok_or_else(|| "Commande de désinstallation invalide.".to_string())?;
    let mut tokens = vec![exe0];
    tokens.extend(args0);

    let first = tokens[0].clone();
    let first_l = first.to_lowercase();
    if first_l.contains("msiexec") {
        return Ok((
            r"C:\Windows\System32\msiexec.exe".into(),
            tokens.into_iter().skip(1).collect(),
            None,
        ));
    }

    let mut acc = String::new();
    for i in 0..tokens.len() {
        if i > 0 {
            acc.push(' ');
        }
        acc.push_str(&tokens[i]);
        let candidate = acc.trim_matches('"').to_string();
        if Path::new(&candidate).is_file() {
            let dir = Path::new(&candidate)
                .parent()
                .map(|p| p.to_string_lossy().into_owned());
            return Ok((candidate, tokens[i + 1..].to_vec(), dir));
        }
    }

    if Path::new(&first).exists() || first_l.ends_with(".exe") {
        let dir = Path::new(&first)
            .parent()
            .filter(|p| !p.as_os_str().is_empty())
            .map(|p| p.to_string_lossy().into_owned());
        return Ok((first, tokens.into_iter().skip(1).collect(), dir));
    }

    Err("Désinstalleur introuvable sur le disque.".into())
}

struct UninstallTarget {
    exe: String,
    params: String,
    dir: Option<String>,
    elevate: bool,
}

fn target_from_app(app: &InstalledApp) -> Result<UninstallTarget, String> {
    let elevate = app.id.starts_with("HKLM");
    let key = app.id.split('|').nth(1).unwrap_or("");

    let msi = if guid_like(key) {
        Some(key.to_string())
    } else {
        app.uninstall
            .as_deref()
            .and_then(msi_product)
            .or_else(|| app.quiet_uninstall.as_deref().and_then(msi_product))
    };

    if let Some(guid) = msi {
        return Ok(UninstallTarget {
            exe: r"C:\Windows\System32\msiexec.exe".into(),
            params: format!("/x {guid}"),
            dir: None,
            elevate,
        });
    }

    let cmd = app
        .uninstall
        .as_deref()
        .or(app.quiet_uninstall.as_deref())
        .ok_or_else(|| "Pas de désinstalleur officiel.".to_string())?;

    let (exe, args, dir) = resolve_command(cmd)?;
    let exe_l = exe.to_lowercase();
    if !(exe_l.ends_with(".exe") || exe_l.contains("msiexec") || exe_l.contains("rundll32")) {
        return Err("Désinstalleur non officiel (pas un .exe Windows).".into());
    }
    Ok(UninstallTarget {
        exe,
        params: join_args(&args),
        dir,
        elevate,
    })
}

#[cfg(windows)]
fn launch_and_wait(target: &UninstallTarget) -> Result<u32, String> {
    use std::mem::{size_of, zeroed};
    use windows::core::PCWSTR;
    use windows::Win32::Foundation::{CloseHandle, HANDLE, HWND, WAIT_TIMEOUT};
    use windows::Win32::System::Threading::{GetExitCodeProcess, WaitForSingleObject};
    use windows::Win32::UI::Shell::{SEE_MASK_NOCLOSEPROCESS, SHELLEXECUTEINFOW, ShellExecuteExW};
    use windows::Win32::UI::WindowsAndMessaging::SW_SHOWNORMAL;

    fn wide(s: &str) -> Vec<u16> {
        s.encode_utf16().chain(std::iter::once(0)).collect()
    }

    let try_verb = |elevate: bool| -> Result<u32, String> {
        let verb = wide(if elevate { "runas" } else { "open" });
        let file = wide(&target.exe);
        let parameters = wide(&target.params);
        let directory = target.dir.as_deref().map(wide);
        let mut info: SHELLEXECUTEINFOW = unsafe { zeroed() };
        info.cbSize = size_of::<SHELLEXECUTEINFOW>() as u32;
        info.fMask = SEE_MASK_NOCLOSEPROCESS;
        info.hwnd = HWND::default();
        info.lpVerb = PCWSTR(verb.as_ptr());
        info.lpFile = PCWSTR(file.as_ptr());
        info.lpParameters = if target.params.is_empty() {
            PCWSTR::null()
        } else {
            PCWSTR(parameters.as_ptr())
        };
        if let Some(d) = &directory {
            info.lpDirectory = PCWSTR(d.as_ptr());
        }
        info.nShow = SW_SHOWNORMAL.0 as i32;

        unsafe {
            ShellExecuteExW(&mut info).map_err(|e| {
                let code = e.code().0 as u32;
                if code == 0x8007_04C7 || code == 1223 {
                    "Élévation refusée. Accepte l’UAC, ou lance Mode Jeu en administrateur.".into()
                } else {
                    format!("Impossible de lancer le désinstalleur : {e}")
                }
            })?;
            let proc = info.hProcess;
            if proc.is_invalid() || proc == HANDLE::default() {
                return Ok(0);
            }
            const MAX_MS: u32 = 30 * 60 * 1000;
            let wr = WaitForSingleObject(proc, MAX_MS);
            let mut code = 0u32;
            let _ = GetExitCodeProcess(proc, &mut code);
            let _ = CloseHandle(proc);
            if wr == WAIT_TIMEOUT {
                return Ok(0);
            }
            Ok(code)
        }
    };

    match try_verb(target.elevate) {
        Ok(code) => Ok(code),
        Err(e) if target.elevate && !e.contains("Élévation refusée") => {
            try_verb(false).or(Err(e))
        }
        Err(e) => Err(e),
    }
}

#[cfg(not(windows))]
fn launch_and_wait(_target: &UninstallTarget) -> Result<u32, String> {
    Err("Désinstallation Windows uniquement.".into())
}

fn uninstall_ok(code: u32) -> bool {
    matches!(code, 0 | 1641 | 3010)
}

pub fn uninstall(id: &str) -> Result<(), String> {
    if id.starts_with("proc:") {
        return Err("Cette entrée n’a pas de désinstalleur Windows lié.".into());
    }
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

    let target = target_from_app(app)?;
    let code = launch_and_wait(&target)?;

    invalidate_cache();

    if code == 1602 {
        return Err("Désinstallation annulée.".into());
    }
    if !uninstall_ok(code) {
        let still = list_installed().iter().any(|a| a.id == id);
        if still {
            return Err(format!(
                "Le désinstalleur s’est fermé (code {code}) et l’app est encore installée. Réessaie, ou lance Mode Jeu en administrateur."
            ));
        }
    }
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

#[derive(Serialize, Clone)]
pub struct MusicApp {
    pub name: String,
    pub path: String,
    pub added: bool,
}

const MUSIC_NAMES: &[&str] = &[
    "spotify", "deezer", "tidal", "apple music", "itunes", "foobar", "aimp", "musicbee",
    "vlc", "winamp", "clementine", "strawberry", "doppler", "cider", "amazon music",
    "youtube music", "plexamp", "audirvana", "mediamonkey", "groove music", "media player",
];

fn is_music_name(name: &str) -> bool {
    let n = name.to_lowercase();
    MUSIC_NAMES.iter().any(|h| n.contains(h))
}

fn push_exe(out: &mut Vec<MusicApp>, name: &str, path: String, seen: &mut HashSet<String>) {
    let key = path.to_lowercase();
    if !Path::new(&path).is_file() || !seen.insert(key) {
        return;
    }
    out.push(MusicApp {
        name: name.to_string(),
        path,
        added: false,
    });
}

fn exe_in_dir(dir: &Path, hint: &str) -> Option<String> {
    let entries = std::fs::read_dir(dir).ok()?;
    let hint = hint.to_lowercase();
    let mut fallback = None;
    for entry in entries.flatten() {
        let path = entry.path();
        if path.extension().and_then(|e| e.to_str()).map(|e| e.eq_ignore_ascii_case("exe")) != Some(true) {
            continue;
        }
        let file = path.file_name().unwrap_or_default().to_string_lossy().to_lowercase();
        if file.contains("unins") || file.contains("update") || file.contains("crash") || file.contains("setup") {
            continue;
        }
        if hint.split_whitespace().any(|w| w.len() > 3 && file.contains(w)) {
            return Some(path.to_string_lossy().into_owned());
        }
        if fallback.is_none() {
            fallback = Some(path.to_string_lossy().into_owned());
        }
    }
    fallback
}

pub fn discover_music() -> Vec<MusicApp> {
    let mut out = Vec::new();
    let mut seen = HashSet::new();
    if let Ok(appdata) = std::env::var("APPDATA") {
        push_exe(&mut out, "Spotify", format!(r"{appdata}\Spotify\Spotify.exe"), &mut seen);
    }
    if let Ok(local) = std::env::var("LOCALAPPDATA") {
        push_exe(&mut out, "Deezer", format!(r"{local}\Deezer\Deezer.exe"), &mut seen);
        push_exe(&mut out, "TIDAL", format!(r"{local}\TIDAL\TIDAL.exe"), &mut seen);
        push_exe(&mut out, "Amazon Music", format!(r"{local}\Amazon Music\Amazon Music.exe"), &mut seen);
    }
    for root in [std::env::var("ProgramFiles").ok(), std::env::var("ProgramFiles(x86)").ok()].into_iter().flatten() {
        push_exe(&mut out, "VLC", format!(r"{root}\VideoLAN\VLC\vlc.exe"), &mut seen);
        push_exe(&mut out, "foobar2000", format!(r"{root}\foobar2000\foobar2000.exe"), &mut seen);
        push_exe(&mut out, "AIMP", format!(r"{root}\AIMP\AIMP.exe"), &mut seen);
        push_exe(&mut out, "MusicBee", format!(r"{root}\MusicBee\MusicBee.exe"), &mut seen);
        push_exe(&mut out, "Winamp", format!(r"{root}\Winamp\winamp.exe"), &mut seen);
    }
    for app in list_installed() {
        if !is_music_name(&app.name) {
            continue;
        }
        let Some(folder) = app.folder.as_deref() else { continue };
        let Some(exe) = exe_in_dir(Path::new(folder), &app.name) else { continue };
        push_exe(&mut out, &app.name, exe, &mut seen);
    }
    out.sort_by(|a, b| a.name.to_lowercase().cmp(&b.name.to_lowercase()));
    out
}

#[cfg(test)]
mod tests {
    use super::*;

    fn sample(name: &str, folder: Option<&str>) -> InstalledApp {
        InstalledApp {
            id: name.into(),
            name: name.into(),
            publisher: String::new(),
            version: String::new(),
            size_mb: 0,
            location: folder.map(str::to_string),
            uninstall: None,
            quiet_uninstall: None,
            can_uninstall: true,
            locked: false,
            source: "Win64".into(),
            folder: folder.map(str::to_string),
            duplicate_group: String::new(),
            duplicate_with: Vec::new(),
        }
    }

    #[test]
    fn same_product_ignores_architecture_and_version() {
        assert_eq!(
            duplicate_key("Google Chrome (64-bit)"),
            duplicate_key("Google Chrome")
        );
        assert_eq!(duplicate_key("Discord 1.0.9152"), duplicate_key("Discord"));
    }

    #[test]
    fn visual_c_years_stay_distinct() {
        assert_ne!(
            duplicate_key("Microsoft Visual C++ 2015"),
            duplicate_key("Microsoft Visual C++ 2022")
        );
    }

    #[test]
    fn groups_shared_name_or_folder() {
        let mut apps = vec![
            sample("Steam", Some(r"C:\Program Files (x86)\Steam")),
            sample("Steam (64-bit)", Some(r"D:\Games\Steam")),
            sample("Autre", Some(r"C:\Program Files (x86)\Steam")),
            sample("Notepad++", Some(r"C:\Program Files\Notepad++")),
        ];
        annotate_duplicates(&mut apps);
        assert!(!apps[0].duplicate_group.is_empty());
        assert_eq!(apps[0].duplicate_with.len(), 2);
        assert!(!apps[2].duplicate_group.is_empty());
        assert!(apps[3].duplicate_group.is_empty());
    }
}
