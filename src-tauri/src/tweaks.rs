//! Tweaks Windows appliqués à l'activation du mode jeu (restaurés à la sortie).
use serde::{Deserialize, Serialize};

#[cfg(windows)]
use std::os::windows::process::CommandExt;
#[cfg(windows)]
const NO_WINDOW: u32 = 0x0800_0000;

const HIGH_PERF: &str = "8c5e7fda-e8bf-4a96-9a85-a6e23a8c635c";
const ULTIMATE: &str = "e9a42b02-d5df-448d-aa00-03f14749eb61";

#[derive(Serialize, Deserialize, Clone, Default)]
#[serde(default)]
pub struct TweaksSnapshot {
    pub previous_plan: Option<String>,
    pub visual_fx: Option<u32>,
    pub transparency: Option<u32>,
    pub game_dvr_app: Option<u32>,
    pub game_dvr_store: Option<u32>,
    pub game_mode_auto: Option<u32>,
    pub toast_enabled: Option<u32>,
    pub widgets_taskbar: Option<u32>,
    pub background_apps: Option<u32>,
    pub system_responsiveness: Option<u32>,
    pub network_throttling: Option<u32>,
    pub games_gpu_priority: Option<u32>,
    pub games_priority: Option<u32>,
    pub cpu_min_ac: Option<u32>,
    pub cpu_min_dc: Option<u32>,
    pub usb_suspend_ac: Option<u32>,
    pub usb_suspend_dc: Option<u32>,
    pub stopped_services: Vec<String>,
    /// UseNexusForGameBarEnabled avant la session. `set` vrai même si la valeur n'existait pas.
    pub gamebar_nexus: Option<u32>,
    pub gamebar_nexus_set: bool,
    pub game_mode_allow: Option<u32>,
    pub game_mode_allow_set: bool,
    pub games_scheduling: Option<String>,
    pub games_scheduling_set: bool,
    pub games_sfio: Option<String>,
    pub games_sfio_set: bool,
    /// PERFBOOSTMODE secteur seulement.
    pub perf_boost_ac: Option<u32>,
    /// PERFEPP secteur seulement (0 = performance max, utile sur un Ryzen).
    pub perf_epp_ac: Option<u32>,
    pub focus_mode: Option<u32>,
    pub focus_mode_set: bool,
    pub gpu_prefs: Vec<GpuPref>,
}

#[derive(Serialize, Deserialize, Clone, Default)]
pub struct GpuPref {
    pub path: String,
    pub previous: Option<String>,
}

fn run(cmd: &str, args: &[&str]) -> Option<String> {
    let mut c = std::process::Command::new(cmd);
    c.args(args);
    #[cfg(windows)]
    c.creation_flags(NO_WINDOW);
    c.output()
        .ok()
        .map(|o| String::from_utf8_lossy(&o.stdout).to_string())
}

fn reg_query_dword(path: &str, value: &str) -> Option<u32> {
    let out = run("reg", &["query", path, "/v", value])?;
    for token in out.split_whitespace() {
        if let Some(hex) = token.strip_prefix("0x").or_else(|| token.strip_prefix("0X")) {
            return u32::from_str_radix(hex, 16).ok();
        }
    }
    None
}

fn reg_query_sz(path: &str, value: &str) -> Option<String> {
    let out = run("reg", &["query", path, "/v", value])?;
    for line in out.lines() {
        let lower = line.to_lowercase();
        let Some(idx) = lower.find("reg_sz") else {
            continue;
        };
        let data = line[idx + "reg_sz".len()..].trim();
        if !data.is_empty() {
            return Some(data.to_string());
        }
    }
    None
}

fn reg_set_sz(path: &str, value: &str, data: &str) {
    let _ = run(
        "reg",
        &["add", path, "/v", value, "/t", "REG_SZ", "/d", data, "/f"],
    );
}

fn reg_delete_value(path: &str, value: &str) {
    let _ = run("reg", &["delete", path, "/v", value, "/f"]);
}

fn reg_set_dword(path: &str, value: &str, data: u32) {
    let _ = run(
        "reg",
        &[
            "add",
            path,
            "/v",
            value,
            "/t",
            "REG_DWORD",
            "/d",
            &data.to_string(),
            "/f",
        ],
    );
}

fn active_plan() -> Option<String> {
    let out = run("powercfg", &["/getactivescheme"])?;
    out.split_whitespace()
        .find(|t| t.len() == 36 && t.matches('-').count() == 4)
        .map(|s| s.to_string())
}

fn ensure_ultimate_plan() -> Option<String> {
    let list = run("powercfg", &["/list"]).unwrap_or_default();
    for line in list.lines() {
        if line.to_lowercase().contains("ultimate") {
            if let Some(guid) = line
                .split_whitespace()
                .find(|t| t.len() == 36 && t.matches('-').count() == 4)
            {
                return Some(guid.to_string());
            }
        }
    }
    let out = run("powercfg", &["-duplicatescheme", ULTIMATE])?;
    out.split_whitespace()
        .find(|t| t.len() == 36 && t.matches('-').count() == 4)
        .map(|s| s.to_string())
        .or_else(|| Some(ULTIMATE.to_string()))
}

pub struct TweakFlags {
    pub high_performance: bool,
    pub ultimate_performance: bool,
    pub disable_game_dvr: bool,
    pub enable_game_mode: bool,
    pub disable_notifications: bool,
    pub visual_effects_perf: bool,
    pub disable_transparency: bool,
    pub disable_widgets: bool,
    pub disable_background_apps: bool,
    pub mmcss_gaming: bool,
    pub cpu_unpark: bool,
    pub cpu_boost: bool,
    pub focus_assist: bool,
    pub gpu_high_perf: bool,
    pub gpu_paths: Vec<String>,
    pub stop_services: bool,
    pub services: Vec<String>,
}

pub fn apply(flags: &TweakFlags) -> TweaksSnapshot {
    let mut snap = TweaksSnapshot::default();

    if flags.ultimate_performance || flags.high_performance {
        snap.previous_plan = active_plan();
        if flags.ultimate_performance {
            if let Some(guid) = ensure_ultimate_plan() {
                run("powercfg", &["/setactive", &guid]);
            } else {
                run("powercfg", &["/setactive", HIGH_PERF]);
            }
        } else {
            run("powercfg", &["/setactive", HIGH_PERF]);
        }
    }

    if flags.disable_game_dvr {
        const GDVR: &str = r"HKCU\Software\Microsoft\Windows\CurrentVersion\GameDVR";
        const STORE: &str = r"HKCU\System\GameConfigStore";
        snap.game_dvr_app = reg_query_dword(GDVR, "AppCaptureEnabled");
        snap.game_dvr_store = reg_query_dword(STORE, "GameDVR_Enabled");
        reg_set_dword(GDVR, "AppCaptureEnabled", 0);
        reg_set_dword(STORE, "GameDVR_Enabled", 0);
        const BAR: &str = r"HKCU\Software\Microsoft\GameBar";
        snap.gamebar_nexus = reg_query_dword(BAR, "UseNexusForGameBarEnabled");
        snap.gamebar_nexus_set = true;
        reg_set_dword(BAR, "UseNexusForGameBarEnabled", 0);
    }

    if flags.enable_game_mode {
        const BAR: &str = r"HKCU\Software\Microsoft\GameBar";
        snap.game_mode_auto = reg_query_dword(BAR, "AutoGameModeEnabled");
        snap.game_mode_allow = reg_query_dword(BAR, "AllowAutoGameMode");
        snap.game_mode_allow_set = true;
        reg_set_dword(BAR, "AutoGameModeEnabled", 1);
        reg_set_dword(BAR, "AllowAutoGameMode", 1);
    }

    if flags.disable_notifications {
        const PUSH: &str = r"HKCU\Software\Microsoft\Windows\CurrentVersion\PushNotifications";
        snap.toast_enabled = reg_query_dword(PUSH, "ToastEnabled");
        reg_set_dword(PUSH, "ToastEnabled", 0);
    }

    if flags.visual_effects_perf {
        const FX: &str = r"HKCU\Software\Microsoft\Windows\CurrentVersion\Explorer\VisualEffects";
        snap.visual_fx = reg_query_dword(FX, "VisualFXSetting");
        reg_set_dword(FX, "VisualFXSetting", 2);
    }

    if flags.disable_transparency {
        const TH: &str = r"HKCU\Software\Microsoft\Windows\CurrentVersion\Themes\Personalize";
        snap.transparency = reg_query_dword(TH, "EnableTransparency");
        reg_set_dword(TH, "EnableTransparency", 0);
    }

    if flags.disable_widgets {
        const ADV: &str = r"HKCU\Software\Microsoft\Windows\CurrentVersion\Explorer\Advanced";
        snap.widgets_taskbar = reg_query_dword(ADV, "TaskbarDa");
        reg_set_dword(ADV, "TaskbarDa", 0);
    }

    if flags.disable_background_apps {
        const BG: &str =
            r"HKCU\Software\Microsoft\Windows\CurrentVersion\BackgroundAccessApplications";
        snap.background_apps = reg_query_dword(BG, "GlobalUserDisabled");
        reg_set_dword(BG, "GlobalUserDisabled", 1);
    }

    // MMCSS : laisse plus de CPU/réseau au jeu (HKLM, peut échouer sans admin).
    if flags.mmcss_gaming {
        const MM: &str =
            r"HKLM\SOFTWARE\Microsoft\Windows NT\CurrentVersion\Multimedia\SystemProfile";
        snap.system_responsiveness = reg_query_dword(MM, "SystemResponsiveness");
        snap.network_throttling = reg_query_dword(MM, "NetworkThrottlingIndex");
        // 10 laisse un peu de temps au son et à la souris. 0 peut les faire craquer.
        reg_set_dword(MM, "SystemResponsiveness", 10);
        reg_set_dword(MM, "NetworkThrottlingIndex", 0xffff_ffff);
        const GAMES: &str =
            r"HKLM\SOFTWARE\Microsoft\Windows NT\CurrentVersion\Multimedia\SystemProfile\Tasks\Games";
        snap.games_gpu_priority = reg_query_dword(GAMES, "GPU Priority");
        snap.games_priority = reg_query_dword(GAMES, "Priority");
        snap.games_scheduling = reg_query_sz(GAMES, "Scheduling Category");
        snap.games_scheduling_set = true;
        snap.games_sfio = reg_query_sz(GAMES, "SFIO Priority");
        snap.games_sfio_set = true;
        reg_set_dword(GAMES, "GPU Priority", 8);
        reg_set_dword(GAMES, "Priority", 6);
        reg_set_sz(GAMES, "Scheduling Category", "High");
        reg_set_sz(GAMES, "SFIO Priority", "High");
    }

    if flags.cpu_unpark {
        apply_cpu_unpark(&mut snap);
    }

    if flags.cpu_boost {
        apply_cpu_boost(&mut snap);
    }

    if flags.focus_assist {
        apply_focus(&mut snap);
    }

    if flags.gpu_high_perf {
        apply_gpu_prefs(&mut snap, &flags.gpu_paths);
    }

    if flags.stop_services {
        for s in &flags.services {
            if run("sc", &["stop", s]).is_some() {
                snap.stopped_services.push(s.clone());
            }
        }
    }

    snap
}

pub fn restore(snap: &TweaksSnapshot) {
    // Remettre le schéma de jeu avant de changer de plan, sinon High Performance reste modifié.
    restore_cpu_unpark(snap);
    restore_cpu_boost(snap);

    if let Some(plan) = &snap.previous_plan {
        run("powercfg", &["/setactive", plan]);
    }

    if let Some(v) = snap.game_dvr_app {
        reg_set_dword(
            r"HKCU\Software\Microsoft\Windows\CurrentVersion\GameDVR",
            "AppCaptureEnabled",
            v,
        );
    }
    if let Some(v) = snap.game_dvr_store {
        reg_set_dword(r"HKCU\System\GameConfigStore", "GameDVR_Enabled", v);
    }
    if snap.gamebar_nexus_set {
        const BAR: &str = r"HKCU\Software\Microsoft\GameBar";
        if let Some(v) = snap.gamebar_nexus {
            reg_set_dword(BAR, "UseNexusForGameBarEnabled", v);
        } else {
            reg_delete_value(BAR, "UseNexusForGameBarEnabled");
        }
    }
    if let Some(v) = snap.game_mode_auto {
        reg_set_dword(
            r"HKCU\Software\Microsoft\GameBar",
            "AutoGameModeEnabled",
            v,
        );
    }
    if snap.game_mode_allow_set {
        const BAR: &str = r"HKCU\Software\Microsoft\GameBar";
        if let Some(v) = snap.game_mode_allow {
            reg_set_dword(BAR, "AllowAutoGameMode", v);
        } else {
            reg_delete_value(BAR, "AllowAutoGameMode");
        }
    }
    if let Some(v) = snap.toast_enabled {
        reg_set_dword(
            r"HKCU\Software\Microsoft\Windows\CurrentVersion\PushNotifications",
            "ToastEnabled",
            v,
        );
    }
    if let Some(v) = snap.visual_fx {
        reg_set_dword(
            r"HKCU\Software\Microsoft\Windows\CurrentVersion\Explorer\VisualEffects",
            "VisualFXSetting",
            v,
        );
    }
    if let Some(v) = snap.transparency {
        reg_set_dword(
            r"HKCU\Software\Microsoft\Windows\CurrentVersion\Themes\Personalize",
            "EnableTransparency",
            v,
        );
    }
    if let Some(v) = snap.widgets_taskbar {
        reg_set_dword(
            r"HKCU\Software\Microsoft\Windows\CurrentVersion\Explorer\Advanced",
            "TaskbarDa",
            v,
        );
    }
    if let Some(v) = snap.background_apps {
        reg_set_dword(
            r"HKCU\Software\Microsoft\Windows\CurrentVersion\BackgroundAccessApplications",
            "GlobalUserDisabled",
            v,
        );
    }
    if let Some(v) = snap.system_responsiveness {
        reg_set_dword(
            r"HKLM\SOFTWARE\Microsoft\Windows NT\CurrentVersion\Multimedia\SystemProfile",
            "SystemResponsiveness",
            v,
        );
    }
    if let Some(v) = snap.network_throttling {
        reg_set_dword(
            r"HKLM\SOFTWARE\Microsoft\Windows NT\CurrentVersion\Multimedia\SystemProfile",
            "NetworkThrottlingIndex",
            v,
        );
    }
    const GAMES: &str =
        r"HKLM\SOFTWARE\Microsoft\Windows NT\CurrentVersion\Multimedia\SystemProfile\Tasks\Games";
    if let Some(v) = snap.games_gpu_priority {
        reg_set_dword(GAMES, "GPU Priority", v);
    }
    if let Some(v) = snap.games_priority {
        reg_set_dword(GAMES, "Priority", v);
    }
    if snap.games_scheduling_set {
        if let Some(v) = &snap.games_scheduling {
            reg_set_sz(GAMES, "Scheduling Category", v);
        } else {
            reg_delete_value(GAMES, "Scheduling Category");
        }
    }
    if snap.games_sfio_set {
        if let Some(v) = &snap.games_sfio {
            reg_set_sz(GAMES, "SFIO Priority", v);
        } else {
            reg_delete_value(GAMES, "SFIO Priority");
        }
    }

    restore_focus(snap);
    restore_gpu_prefs(snap);

    for s in &snap.stopped_services {
        run("sc", &["start", s]);
    }
}

fn powercfg_ac_index(sub: &str, setting: &str) -> Option<u32> {
    // /QH : le mode boost est souvent masqué, absent d'un simple /query.
    parse_power_index(&run("powercfg", &["/QH", "SCHEME_CURRENT", sub, setting])?, true)
}

fn powercfg_dc_index(sub: &str, setting: &str) -> Option<u32> {
    parse_power_index(&run("powercfg", &["/QH", "SCHEME_CURRENT", sub, setting])?, false)
}

fn parse_power_index(out: &str, ac: bool) -> Option<u32> {
    let mut hit = false;
    for line in out.lines() {
        let l = line.to_lowercase();
        if ac {
            if l.contains("current ac")
                || l.contains("alimentation ca")
                || l.contains("alimentation ac")
                || l.contains("courant alternatif")
            {
                hit = true;
            }
        } else if l.contains("current dc")
            || l.contains("alimentation cc")
            || l.contains("alimentation dc")
            || l.contains("courant continu")
        {
            hit = true;
        }
        if hit {
            if let Some(tok) = line
                .split_whitespace()
                .rev()
                .find(|t| t.len() > 2 && (t.starts_with("0x") || t.starts_with("0X")))
            {
                return u32::from_str_radix(&tok[2..], 16).ok();
            }
        }
    }
    None
}

fn set_power_index(ac: bool, sub: &str, setting: &str, value: u32) {
    let flag = if ac {
        "/setacvalueindex"
    } else {
        "/setdcvalueindex"
    };
    run(
        "powercfg",
        &[flag, "SCHEME_CURRENT", sub, setting, &value.to_string()],
    );
}

fn apply_cpu_unpark(snap: &mut TweaksSnapshot) {
    const SUB_CPU: &str = "SUB_PROCESSOR";
    const MIN: &str = "PROCTHROTTLEMIN";
    const SUB_USB: &str = "SUB_USB";
    const USB: &str = "USBSELECTSUSPEND";

    snap.cpu_min_ac = powercfg_ac_index(SUB_CPU, MIN);
    snap.cpu_min_dc = powercfg_dc_index(SUB_CPU, MIN);
    snap.usb_suspend_ac = powercfg_ac_index(SUB_USB, USB);
    snap.usb_suspend_dc = powercfg_dc_index(SUB_USB, USB);

    if snap.cpu_min_ac.is_some() {
        set_power_index(true, SUB_CPU, MIN, 100);
    }
    if snap.cpu_min_dc.is_some() {
        set_power_index(false, SUB_CPU, MIN, 100);
    }
    if snap.usb_suspend_ac.is_some() {
        set_power_index(true, SUB_USB, USB, 0);
    }
    if snap.usb_suspend_dc.is_some() {
        set_power_index(false, SUB_USB, USB, 0);
    }
    run("powercfg", &["/setactive", "SCHEME_CURRENT"]);
}

fn restore_cpu_unpark(snap: &TweaksSnapshot) {
    const SUB_CPU: &str = "SUB_PROCESSOR";
    const MIN: &str = "PROCTHROTTLEMIN";
    const SUB_USB: &str = "SUB_USB";
    const USB: &str = "USBSELECTSUSPEND";
    let mut changed = false;
    if let Some(v) = snap.cpu_min_ac {
        set_power_index(true, SUB_CPU, MIN, v);
        changed = true;
    }
    if let Some(v) = snap.cpu_min_dc {
        set_power_index(false, SUB_CPU, MIN, v);
        changed = true;
    }
    if let Some(v) = snap.usb_suspend_ac {
        set_power_index(true, SUB_USB, USB, v);
        changed = true;
    }
    if let Some(v) = snap.usb_suspend_dc {
        set_power_index(false, SUB_USB, USB, v);
        changed = true;
    }
    if changed {
        run("powercfg", &["/setactive", "SCHEME_CURRENT"]);
    }
}

/// Boost agressif + préférence énergie à 0, secteur seulement.
/// Sur un Ryzen 5600X, la préférence à 0 évite que les cœurs redescendent
/// pendant que la carte attend.
fn apply_cpu_boost(snap: &mut TweaksSnapshot) {
    const SUB_CPU: &str = "SUB_PROCESSOR";
    const BOOST: &str = "PERFBOOSTMODE";
    const EPP: &str = "PERFEPP";
    snap.perf_boost_ac = powercfg_ac_index(SUB_CPU, BOOST);
    snap.perf_epp_ac = powercfg_ac_index(SUB_CPU, EPP);
    let mut changed = false;
    if snap.perf_boost_ac.is_some() {
        // 2 = agressif (libellé Windows : Offensif).
        set_power_index(true, SUB_CPU, BOOST, 2);
        changed = true;
    }
    if snap.perf_epp_ac.is_some() {
        set_power_index(true, SUB_CPU, EPP, 0);
        changed = true;
    }
    if changed {
        run("powercfg", &["/setactive", "SCHEME_CURRENT"]);
    }
}

fn restore_cpu_boost(snap: &TweaksSnapshot) {
    const SUB_CPU: &str = "SUB_PROCESSOR";
    let mut changed = false;
    if let Some(v) = snap.perf_boost_ac {
        set_power_index(true, SUB_CPU, "PERFBOOSTMODE", v);
        changed = true;
    }
    if let Some(v) = snap.perf_epp_ac {
        set_power_index(true, SUB_CPU, "PERFEPP", v);
        changed = true;
    }
    if changed {
        run("powercfg", &["/setactive", "SCHEME_CURRENT"]);
    }
}

const GPU_KEY: &str = r"HKCU\Software\Microsoft\DirectX\UserGpuPreferences";
const GPU_HIGH: &str = "GpuPreference=2;";

fn apply_gpu_prefs(snap: &mut TweaksSnapshot, paths: &[String]) {
    let mut seen = std::collections::HashSet::new();
    for path in paths {
        let path = path.trim();
        if path.is_empty() || !seen.insert(path.to_lowercase()) {
            continue;
        }
        if snap.gpu_prefs.len() >= 24 {
            break;
        }
        let previous = reg_query_sz(GPU_KEY, path);
        if previous.as_deref() != Some(GPU_HIGH) {
            reg_set_sz(GPU_KEY, path, GPU_HIGH);
        }
        snap.gpu_prefs.push(GpuPref {
            path: path.to_string(),
            previous,
        });
    }
}

fn restore_gpu_prefs(snap: &TweaksSnapshot) {
    for pref in &snap.gpu_prefs {
        if let Some(prev) = &pref.previous {
            reg_set_sz(GPU_KEY, &pref.path, prev);
        } else {
            reg_delete_value(GPU_KEY, &pref.path);
        }
    }
}

fn apply_focus(snap: &mut TweaksSnapshot) {
    let Some(prev) = focus_query() else {
        return;
    };
    if focus_set(2) {
        snap.focus_mode = Some(prev);
        snap.focus_mode_set = true;
    }
}

fn restore_focus(snap: &TweaksSnapshot) {
    if snap.focus_mode_set {
        let _ = focus_set(snap.focus_mode.unwrap_or(0));
    }
}

/// Focus assist : 0 off, 1 priorité, 2 alarmes seulement.
/// Pas d'API publique. On passe par l'état WNF que Windows lui-même met à jour.
fn focus_query() -> Option<u32> {
    #[cfg(windows)]
    {
        focus::query()
    }
    #[cfg(not(windows))]
    {
        None
    }
}

fn focus_set(mode: u32) -> bool {
    #[cfg(windows)]
    {
        focus::set(mode)
    }
    #[cfg(not(windows))]
    {
        let _ = mode;
        false
    }
}

#[cfg(windows)]
mod focus {
    use std::ffi::c_void;

    /// WNF_SHEL_QUIET_MOMENT_SHELL_MODE_CHANGED
    const STATE: u64 = 0x0D83_063E_A3BF_5075;

    #[link(name = "ntdll")]
    extern "system" {
        fn ZwQueryWnfStateData(
            state_name: *const u64,
            type_id: *const c_void,
            explicit_scope: *const c_void,
            change_stamp: *mut u32,
            buffer: *mut c_void,
            buffer_size: *mut u32,
        ) -> i32;

        fn ZwUpdateWnfStateData(
            state_name: *const u64,
            buffer: *const c_void,
            length: u32,
            type_id: *const c_void,
            explicit_scope: *const c_void,
            matching_change_stamp: u32,
            check_stamp: i32,
        ) -> i32;
    }

    pub fn query() -> Option<u32> {
        unsafe {
            let state = STATE;
            let mut stamp = 0u32;
            let mut buf = [0u8; 4];
            let mut size = 4u32;
            let status = ZwQueryWnfStateData(
                &state,
                std::ptr::null(),
                std::ptr::null(),
                &mut stamp,
                buf.as_mut_ptr() as *mut c_void,
                &mut size,
            );
            if status != 0 || size < 4 {
                return None;
            }
            Some(u32::from_le_bytes(buf))
        }
    }

    fn write(mode: u32) -> bool {
        unsafe {
            let state = STATE;
            let buf = mode.to_le_bytes();
            ZwUpdateWnfStateData(
                &state,
                buf.as_ptr() as *const c_void,
                4,
                std::ptr::null(),
                std::ptr::null(),
                0,
                0,
            ) == 0
        }
    }

    pub fn set(mode: u32) -> bool {
        if mode == 0 {
            return write(0);
        }
        // Windows ignore parfois le passage direct. Off, puis le mode voulu.
        if !write(0) {
            return false;
        }
        std::thread::sleep(std::time::Duration::from_millis(40));
        write(mode)
    }
}
