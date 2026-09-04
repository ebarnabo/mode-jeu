//! Tweaks Windows appliqués à l'activation du mode jeu (restaurés à la sortie).
use serde::{Deserialize, Serialize};

#[cfg(windows)]
use std::os::windows::process::CommandExt;
#[cfg(windows)]
const NO_WINDOW: u32 = 0x0800_0000;

const HIGH_PERF: &str = "8c5e7fda-e8bf-4a96-9a85-a6e23a8c635c";
const ULTIMATE: &str = "e9a42b02-d5df-448d-aa00-03f14749eb61";

#[derive(Serialize, Deserialize, Clone, Default)]
pub struct TweaksSnapshot {
    pub previous_plan: Option<String>,
    pub visual_fx: Option<u32>,
    pub transparency: Option<u32>,
    pub game_dvr_app: Option<u32>,
    pub game_dvr_store: Option<u32>,
    pub game_mode_auto: Option<u32>,
    pub toast_enabled: Option<u32>,
    pub stopped_services: Vec<String>,
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
    // ... REG_DWORD    0x1
    for token in out.split_whitespace() {
        if let Some(hex) = token.strip_prefix("0x").or_else(|| token.strip_prefix("0X")) {
            return u32::from_str_radix(hex, 16).ok();
        }
    }
    None
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
    // Si déjà présent dans la liste, on le trouve ; sinon on duplique le schéma.
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
    }

    if flags.enable_game_mode {
        const BAR: &str = r"HKCU\Software\Microsoft\GameBar";
        snap.game_mode_auto = reg_query_dword(BAR, "AutoGameModeEnabled");
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
        reg_set_dword(FX, "VisualFXSetting", 2); // Ajuster pour de meilleures performances
    }

    if flags.disable_transparency {
        const TH: &str = r"HKCU\Software\Microsoft\Windows\CurrentVersion\Themes\Personalize";
        snap.transparency = reg_query_dword(TH, "EnableTransparency");
        reg_set_dword(TH, "EnableTransparency", 0);
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
    if let Some(v) = snap.game_mode_auto {
        reg_set_dword(
            r"HKCU\Software\Microsoft\GameBar",
            "AutoGameModeEnabled",
            v,
        );
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

    for s in &snap.stopped_services {
        run("sc", &["start", s]);
    }
}
