//! Mixeur WASAPI : sortie + chaque app qui a une vraie session audio.
use serde::Serialize;
use std::sync::atomic::{AtomicU32, Ordering};
use std::sync::Mutex;
use std::time::{Duration, Instant};

#[derive(Serialize, Clone)]
pub struct AppVolume {
    pub id: String,
    pub name: String,
    pub volume: u8,
    pub muted: bool,
    pub kind: String,
}

static LAST_COUNT: AtomicU32 = AtomicU32::new(0);
static CACHE: Mutex<Option<(Instant, Vec<AppVolume>)>> = Mutex::new(None);

pub fn last_count() -> u32 {
    LAST_COUNT.load(Ordering::Relaxed)
}

fn remember(list: Vec<AppVolume>) -> Vec<AppVolume> {
    LAST_COUNT.store(list.len() as u32, Ordering::Relaxed);
    if let Ok(mut g) = CACHE.lock() {
        *g = Some((Instant::now(), list.clone()));
    }
    list
}

fn invalidate() {
    if let Ok(mut g) = CACHE.lock() {
        *g = None;
    }
}

pub fn list(fresh: bool) -> Vec<AppVolume> {
    if !fresh {
        if let Ok(g) = CACHE.lock() {
            if let Some((at, cached)) = g.as_ref() {
                if at.elapsed() < Duration::from_millis(400) {
                    return cached.clone();
                }
            }
        }
    }
    remember(enumerate())
}

pub fn set_volume(id: &str, volume: u8) -> Vec<AppVolume> {
    let level = (volume.min(100) as f32) / 100.0;
    apply(id, Some(level), Some(false));
    invalidate();
    list(true)
}

pub fn set_mute(id: &str, muted: bool) -> Vec<AppVolume> {
    apply(id, None, Some(muted));
    invalidate();
    list(true)
}

fn enumerate() -> Vec<AppVolume> {
    #[cfg(windows)]
    {
        win::enumerate()
    }
    #[cfg(not(windows))]
    {
        Vec::new()
    }
}

fn apply(id: &str, volume: Option<f32>, muted: Option<bool>) {
    #[cfg(windows)]
    {
        win::apply(id, volume, muted);
    }
    #[cfg(not(windows))]
    {
        let _ = (id, volume, muted);
    }
}

#[cfg(windows)]
mod win {
    use super::AppVolume;
    use std::collections::HashMap;
    use windows::core::{Interface, GUID, PWSTR};
    use windows::Win32::Foundation::{BOOL, CloseHandle, MAX_PATH};
    use windows::Win32::Media::Audio::Endpoints::IAudioEndpointVolume;
    use windows::Win32::Media::Audio::{
        eMultimedia, eRender, IAudioSessionControl2, IAudioSessionManager2, IMMDevice,
        IMMDeviceEnumerator, ISimpleAudioVolume, MMDeviceEnumerator, AudioSessionStateExpired,
        DEVICE_STATE_ACTIVE,
    };
    use windows::Win32::System::Com::{
        CoCreateInstance, CoInitializeEx, CLSCTX_ALL, COINIT_APARTMENTTHREADED,
    };
    use windows::Win32::System::Threading::{
        OpenProcess, QueryFullProcessImageNameW, PROCESS_NAME_WIN32, PROCESS_QUERY_LIMITED_INFORMATION,
    };

    /// WebView2 a déjà initialisé COM en STA. Ne jamais abandonner si le mode diffère.
    fn ensure_com() {
        unsafe {
            let _ = CoInitializeEx(None, COINIT_APARTMENTTHREADED);
        }
    }

    fn pretty(stem: &str) -> String {
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

    fn process_exe(pid: u32) -> Option<String> {
        unsafe {
            let proc = OpenProcess(PROCESS_QUERY_LIMITED_INFORMATION, false, pid).ok()?;
            let mut buf = [0u16; MAX_PATH as usize];
            let mut len = buf.len() as u32;
            let ok = QueryFullProcessImageNameW(
                proc,
                PROCESS_NAME_WIN32,
                PWSTR(buf.as_mut_ptr()),
                &mut len,
            )
            .is_ok();
            let _ = CloseHandle(proc);
            if !ok || len == 0 {
                return None;
            }
            let path = String::from_utf16_lossy(&buf[..len as usize]);
            let name = std::path::Path::new(&path)
                .file_name()
                .and_then(|s| s.to_str())
                .unwrap_or("app.exe")
                .to_lowercase();
            Some(name)
        }
    }

    fn enumerator() -> Option<IMMDeviceEnumerator> {
        unsafe { CoCreateInstance(&MMDeviceEnumerator, None, CLSCTX_ALL).ok() }
    }

    fn default_device(enumr: &IMMDeviceEnumerator) -> Option<IMMDevice> {
        unsafe { enumr.GetDefaultAudioEndpoint(eRender, eMultimedia).ok() }
    }

    fn collect_devices(enumr: &IMMDeviceEnumerator) -> Vec<IMMDevice> {
        let mut out = Vec::new();
        if let Some(def) = default_device(enumr) {
            out.push(def);
        }
        unsafe {
            if let Ok(col) = enumr.EnumAudioEndpoints(eRender, DEVICE_STATE_ACTIVE) {
                let n = col.GetCount().unwrap_or(0);
                for i in 0..n {
                    if let Ok(dev) = col.Item(i) {
                        out.push(dev);
                    }
                }
            }
        }
        out
    }

    struct Raw {
        id: String,
        name: String,
        volume: f32,
        muted: bool,
        kind: &'static str,
    }

    fn device_row(dev: &IMMDevice) -> Option<Raw> {
        unsafe {
            let vol: IAudioEndpointVolume = dev.Activate(CLSCTX_ALL, None).ok()?;
            let level = vol.GetMasterVolumeLevelScalar().ok()?;
            let muted = vol.GetMute().ok().map(|b| b.as_bool()).unwrap_or(false);
            Some(Raw {
                id: "@device".into(),
                name: "Sortie".into(),
                volume: level.clamp(0.0, 1.0),
                muted,
                kind: "device",
            })
        }
    }

    fn session_rows(dev: &IMMDevice, out: &mut Vec<Raw>) {
        unsafe {
            let Ok(manager) = dev.Activate::<IAudioSessionManager2>(CLSCTX_ALL, None) else {
                return;
            };
            let Ok(sess_enum) = manager.GetSessionEnumerator() else {
                return;
            };
            let count = sess_enum.GetCount().unwrap_or(0);
            for s in 0..count {
                let Ok(ctrl) = sess_enum.GetSession(s) else { continue };
                if ctrl.GetState().ok() == Some(AudioSessionStateExpired) {
                    continue;
                }
                let Ok(ctrl2) = ctrl.cast::<IAudioSessionControl2>() else { continue };
                let Ok(pid) = ctrl2.GetProcessId() else { continue };
                let Ok(simple) = ctrl2.cast::<ISimpleAudioVolume>() else { continue };
                let level = simple.GetMasterVolume().unwrap_or(0.0);
                let muted = simple.GetMute().ok().map(|b| b.as_bool()).unwrap_or(false);
                if pid == 0 {
                    out.push(Raw {
                        id: "@system".into(),
                        name: "Windows".into(),
                        volume: level.clamp(0.0, 1.0),
                        muted,
                        kind: "system",
                    });
                    continue;
                }
                let Some(exe) = process_exe(pid) else { continue };
                if exe == "audiodg.exe" || exe.starts_with("mode-jeu") {
                    continue;
                }
                out.push(Raw {
                    id: exe.clone(),
                    name: pretty(exe.trim_end_matches(".exe")),
                    volume: level.clamp(0.0, 1.0),
                    muted,
                    kind: "app",
                });
            }
        }
    }

    pub fn enumerate() -> Vec<AppVolume> {
        ensure_com();
        let Some(enumr) = enumerator() else {
            return Vec::new();
        };
        let devices = collect_devices(&enumr);
        let mut raw = Vec::new();
        if let Some(def) = devices.first() {
            if let Some(row) = device_row(def) {
                raw.push(row);
            }
        }
        for dev in &devices {
            session_rows(dev, &mut raw);
        }
        let mut by: HashMap<String, (String, f32, bool, u32, &'static str)> = HashMap::new();
        for s in raw {
            let e = by
                .entry(s.id.clone())
                .or_insert((s.name.clone(), 0.0, true, 0, s.kind));
            e.1 += s.volume;
            e.2 &= s.muted;
            e.3 += 1;
        }
        let mut list: Vec<AppVolume> = by
            .into_iter()
            .map(|(id, (name, sum, muted, n, kind))| AppVolume {
                id,
                name,
                volume: ((sum / n.max(1) as f32) * 100.0).round() as u8,
                muted,
                kind: kind.into(),
            })
            .collect();
        list.sort_by(|a, b| {
            let rank = |k: &str| match k {
                "device" => 0,
                "system" => 2,
                _ => 1,
            };
            rank(&a.kind)
                .cmp(&rank(&b.kind))
                .then(a.name.to_lowercase().cmp(&b.name.to_lowercase()))
        });
        list
    }

    fn set_session(dev: &IMMDevice, want: &str, volume: Option<f32>, muted: Option<bool>) {
        unsafe {
            let Ok(manager) = dev.Activate::<IAudioSessionManager2>(CLSCTX_ALL, None) else {
                return;
            };
            let Ok(sess_enum) = manager.GetSessionEnumerator() else {
                return;
            };
            let count = sess_enum.GetCount().unwrap_or(0);
            for s in 0..count {
                let Ok(ctrl) = sess_enum.GetSession(s) else { continue };
                let Ok(ctrl2) = ctrl.cast::<IAudioSessionControl2>() else { continue };
                let Ok(pid) = ctrl2.GetProcessId() else { continue };
                let id = if pid == 0 {
                    "@system".into()
                } else if let Some(exe) = process_exe(pid) {
                    exe
                } else {
                    continue;
                };
                if id != want {
                    continue;
                }
                let Ok(vol) = ctrl2.cast::<ISimpleAudioVolume>() else { continue };
                if let Some(level) = volume {
                    let _ = vol.SetMasterVolume(level.clamp(0.0, 1.0), std::ptr::null::<GUID>());
                }
                if let Some(m) = muted {
                    let _ = vol.SetMute(BOOL::from(m), std::ptr::null::<GUID>());
                }
            }
        }
    }

    fn set_device(dev: &IMMDevice, volume: Option<f32>, muted: Option<bool>) {
        unsafe {
            let Ok(vol) = dev.Activate::<IAudioEndpointVolume>(CLSCTX_ALL, None) else {
                return;
            };
            if let Some(level) = volume {
                let _ = vol.SetMasterVolumeLevelScalar(level.clamp(0.0, 1.0), std::ptr::null::<GUID>());
            }
            if let Some(m) = muted {
                let _ = vol.SetMute(BOOL::from(m), std::ptr::null::<GUID>());
            }
        }
    }

    pub fn apply(id: &str, volume: Option<f32>, muted: Option<bool>) {
        ensure_com();
        let Some(enumr) = enumerator() else {
            return;
        };
        let want = id.to_lowercase();
        let devices = collect_devices(&enumr);
        if want == "@device" {
            if let Some(def) = devices.first() {
                set_device(def, volume, muted);
            }
            return;
        }
        for dev in &devices {
            set_session(dev, &want, volume, muted);
        }
    }
}
