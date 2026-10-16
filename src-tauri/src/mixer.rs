//! Mixeur par application (sessions WASAPI réelles, pas un pourcentage inventé).
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
                if at.elapsed() < Duration::from_millis(700) {
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
    use windows::core::{Interface, PWSTR};
    use windows::Win32::Foundation::{CloseHandle, MAX_PATH};
    use windows::Win32::Media::Audio::{
        eRender, IAudioSessionControl2, IAudioSessionManager2, IMMDeviceEnumerator,
        ISimpleAudioVolume, MMDeviceEnumerator, AudioSessionStateExpired, DEVICE_STATE_ACTIVE,
    };
    use windows::Win32::System::Com::{
        CoCreateInstance, CoInitializeEx, CLSCTX_ALL, COINIT_MULTITHREADED,
    };
    use windows::Win32::System::Threading::{
        OpenProcess, QueryFullProcessImageNameW, PROCESS_NAME_WIN32, PROCESS_QUERY_LIMITED_INFORMATION,
    };

    fn com_ok() -> bool {
        let hr = unsafe { CoInitializeEx(None, COINIT_MULTITHREADED) };
        hr.is_ok() || hr == windows::core::HRESULT(1) // S_FALSE : déjà initialisé
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

    struct Raw {
        id: String,
        name: String,
        volume: f32,
        muted: bool,
    }

    fn sessions() -> Vec<Raw> {
        if !com_ok() {
            return Vec::new();
        }
        let mut out = Vec::new();
        unsafe {
            let Ok(enumerator) =
                CoCreateInstance::<_, IMMDeviceEnumerator>(&MMDeviceEnumerator, None, CLSCTX_ALL)
            else {
                return out;
            };
            let Ok(col) = enumerator.EnumAudioEndpoints(eRender, DEVICE_STATE_ACTIVE) else {
                return out;
            };
            let n = col.GetCount().unwrap_or(0);
            for i in 0..n {
                let Ok(dev) = col.Item(i) else { continue };
                let Ok(manager) = dev.Activate::<IAudioSessionManager2>(CLSCTX_ALL, None) else {
                    continue;
                };
                let Ok(sess_enum) = manager.GetSessionEnumerator() else {
                    continue;
                };
                let count = sess_enum.GetCount().unwrap_or(0);
                for s in 0..count {
                    let Ok(ctrl) = sess_enum.GetSession(s) else { continue };
                    if ctrl.GetState().ok() == Some(AudioSessionStateExpired) {
                        continue;
                    }
                    let Ok(ctrl2) = ctrl.cast::<IAudioSessionControl2>() else { continue };
                    let Ok(pid) = ctrl2.GetProcessId() else { continue };
                    if pid == 0 {
                        continue;
                    }
                    let Some(exe) = process_exe(pid) else { continue };
                    if exe == "audiodg.exe" || exe.starts_with("mode-jeu") {
                        continue;
                    }
                    let Ok(vol) = ctrl2.cast::<ISimpleAudioVolume>() else { continue };
                    let level = vol.GetMasterVolume().unwrap_or(0.0);
                    let muted = vol.GetMute().ok().map(|b| b.as_bool()).unwrap_or(false);
                    let name = pretty(exe.trim_end_matches(".exe"));
                    out.push(Raw {
                        id: exe,
                        name,
                        volume: level.clamp(0.0, 1.0),
                        muted,
                    });
                }
            }
        }
        out
    }

    pub fn enumerate() -> Vec<AppVolume> {
        let mut by: HashMap<String, (String, f32, bool, u32)> = HashMap::new();
        for s in sessions() {
            let e = by.entry(s.id.clone()).or_insert((s.name.clone(), 0.0, true, 0));
            e.1 += s.volume;
            e.2 &= s.muted;
            e.3 += 1;
        }
        let mut list: Vec<AppVolume> = by
            .into_iter()
            .map(|(id, (name, sum, muted, n))| AppVolume {
                id,
                name,
                volume: ((sum / n.max(1) as f32) * 100.0).round() as u8,
                muted,
            })
            .collect();
        list.sort_by(|a, b| a.name.to_lowercase().cmp(&b.name.to_lowercase()));
        list
    }

    pub fn apply(id: &str, volume: Option<f32>, muted: Option<bool>) {
        if !com_ok() {
            return;
        }
        let want = id.to_lowercase();
        unsafe {
            let Ok(enumerator) =
                CoCreateInstance::<_, IMMDeviceEnumerator>(&MMDeviceEnumerator, None, CLSCTX_ALL)
            else {
                return;
            };
            let Ok(col) = enumerator.EnumAudioEndpoints(eRender, DEVICE_STATE_ACTIVE) else {
                return;
            };
            let n = col.GetCount().unwrap_or(0);
            for i in 0..n {
                let Ok(dev) = col.Item(i) else { continue };
                let Ok(manager) = dev.Activate::<IAudioSessionManager2>(CLSCTX_ALL, None) else {
                    continue;
                };
                let Ok(sess_enum) = manager.GetSessionEnumerator() else {
                    continue;
                };
                let count = sess_enum.GetCount().unwrap_or(0);
                for s in 0..count {
                    let Ok(ctrl) = sess_enum.GetSession(s) else { continue };
                    let Ok(ctrl2) = ctrl.cast::<IAudioSessionControl2>() else { continue };
                    let Ok(pid) = ctrl2.GetProcessId() else { continue };
                    if pid == 0 {
                        continue;
                    }
                    let Some(exe) = process_exe(pid) else { continue };
                    if exe != want {
                        continue;
                    }
                    let Ok(vol) = ctrl2.cast::<ISimpleAudioVolume>() else { continue };
                    if let Some(level) = volume {
                        let _ = vol.SetMasterVolume(level.clamp(0.0, 1.0), std::ptr::null());
                    }
                    if let Some(m) = muted {
                        let _ = vol.SetMute(m, std::ptr::null());
                    }
                }
            }
        }
    }
}
