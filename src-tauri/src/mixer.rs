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

#[derive(Serialize, Clone)]
pub struct AudioOutput {
    pub id: String,
    pub name: String,
    pub default: bool,
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

pub fn list_outputs() -> Vec<AudioOutput> {
    #[cfg(windows)]
    {
        win::list_outputs()
    }
    #[cfg(not(windows))]
    {
        Vec::new()
    }
}

pub fn set_default_output(id: &str) -> Result<(), String> {
    #[cfg(windows)]
    {
        win::set_default_output(id)?;
        invalidate();
        Ok(())
    }
    #[cfg(not(windows))]
    {
        let _ = id;
        Err("Windows uniquement".into())
    }
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
    use super::{AppVolume, AudioOutput};
    use std::collections::{HashMap, HashSet};
    use windows::core::{Interface, IUnknown_Vtbl, GUID, HRESULT, IUnknown, PCWSTR, PWSTR};
    use windows::Win32::Devices::FunctionDiscovery::PKEY_Device_FriendlyName;
    use windows::Win32::Foundation::{BOOL, CloseHandle, MAX_PATH};
    use windows::Win32::Media::Audio::Endpoints::IAudioEndpointVolume;
    use windows::Win32::Media::Audio::{
        eCommunications, eConsole, eMultimedia, eRender, IAudioSessionControl2,
        IAudioSessionManager2, IMMDevice, IMMDeviceEnumerator, ISimpleAudioVolume,
        MMDeviceEnumerator, AudioSessionStateExpired, DEVICE_STATE_ACTIVE, ERole,
    };
    use windows::Win32::System::Com::{
        CoCreateInstance, CoInitializeEx, CoTaskMemFree, CLSCTX_ALL, COINIT_APARTMENTTHREADED,
        STGM_READ,
    };
    use windows::Win32::System::Com::StructuredStorage::{PropVariantClear, PropVariantToBSTR};
    use windows::Win32::UI::Shell::PropertiesSystem::IPropertyStore;
    use windows::Win32::System::Threading::{
        OpenProcess, QueryFullProcessImageNameW, PROCESS_NAME_WIN32, PROCESS_QUERY_LIMITED_INFORMATION,
    };

    const CLSID_POLICY_CONFIG: GUID = GUID::from_u128(0x870af99c_171d_4f9e_af0d_e63df40c2bc9);
    const CLSID_POLICY_CONFIG_VISTA: GUID = GUID::from_u128(0x294935ce_f637_4e7c_a41b_ab255460b862);

    #[allow(non_snake_case)]
    #[repr(C)]
    struct IPolicyConfig_Vtbl {
        pub base__: IUnknown_Vtbl,
        pub GetMixFormat: usize,
        pub GetDeviceFormat: usize,
        pub ResetDeviceFormat: usize,
        pub SetDeviceFormat: usize,
        pub GetProcessingPeriod: usize,
        pub SetProcessingPeriod: usize,
        pub GetShareMode: usize,
        pub SetShareMode: usize,
        pub GetPropertyValue: usize,
        pub SetPropertyValue: usize,
        pub SetDefaultEndpoint: unsafe extern "system" fn(*mut std::ffi::c_void, PCWSTR, ERole) -> HRESULT,
        pub SetEndpointVisibility: usize,
    }

    #[derive(Clone)]
    #[repr(transparent)]
    struct IPolicyConfig(IUnknown);

    unsafe impl Interface for IPolicyConfig {
        type Vtable = IPolicyConfig_Vtbl;
        const IID: GUID = GUID::from_u128(0xf8679f50_850a_41cf_9c72_430f290290c8);
    }

    #[allow(non_snake_case)]
    #[repr(C)]
    struct IPolicyConfigVista_Vtbl {
        pub base__: IUnknown_Vtbl,
        pub GetMixFormat: usize,
        pub GetDeviceFormat: usize,
        pub SetDeviceFormat: usize,
        pub GetProcessingPeriod: usize,
        pub SetProcessingPeriod: usize,
        pub GetShareMode: usize,
        pub SetShareMode: usize,
        pub GetPropertyValue: usize,
        pub SetPropertyValue: usize,
        pub SetDefaultEndpoint: unsafe extern "system" fn(*mut std::ffi::c_void, PCWSTR, ERole) -> HRESULT,
        pub SetEndpointVisibility: usize,
    }

    #[derive(Clone)]
    #[repr(transparent)]
    struct IPolicyConfigVista(IUnknown);

    unsafe impl Interface for IPolicyConfigVista {
        type Vtable = IPolicyConfigVista_Vtbl;
        const IID: GUID = GUID::from_u128(0x568b9108_44bf_40b4_9006_86afe5b5a620);
    }

    fn is_device_id(id: &str) -> bool {
        id.starts_with('{')
    }

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

    fn device_id(dev: &IMMDevice) -> Option<String> {
        unsafe {
            let id = dev.GetId().ok()?;
            let s = id.to_string().ok()?;
            CoTaskMemFree(Some(id.0 as _));
            Some(s)
        }
    }

    fn friendly_name(dev: &IMMDevice) -> Option<String> {
        unsafe {
            let store: IPropertyStore = dev.OpenPropertyStore(STGM_READ).ok()?;
            let mut var = store.GetValue(&PKEY_Device_FriendlyName).ok()?;
            let name = PropVariantToBSTR(&var).ok().map(|b| b.to_string());
            let _ = PropVariantClear(&mut var);
            name
        }
    }

    fn enum_render_devices(enumr: &IMMDeviceEnumerator) -> Vec<IMMDevice> {
        let mut out = Vec::new();
        let mut seen = HashSet::new();
        unsafe {
            if let Ok(col) = enumr.EnumAudioEndpoints(eRender, DEVICE_STATE_ACTIVE) {
                let n = col.GetCount().unwrap_or(0);
                for i in 0..n {
                    if let Ok(dev) = col.Item(i) {
                        if let Some(id) = device_id(&dev) {
                            if seen.insert(id) {
                                out.push(dev);
                            }
                        }
                    }
                }
            }
        }
        out
    }

    fn find_device(enumr: &IMMDeviceEnumerator, want: &str) -> Option<IMMDevice> {
        let want = want.to_lowercase();
        for dev in enum_render_devices(enumr) {
            if device_id(&dev).map(|id| id.to_lowercase() == want).unwrap_or(false) {
                return Some(dev);
            }
        }
        None
    }

    pub fn list_outputs() -> Vec<AudioOutput> {
        ensure_com();
        let Some(enumr) = enumerator() else {
            return Vec::new();
        };
        let default_id = default_device(&enumr).and_then(|d| device_id(&d));
        let mut list: Vec<AudioOutput> = enum_render_devices(&enumr)
            .into_iter()
            .filter_map(|dev| {
                let id = device_id(&dev)?;
                let name = friendly_name(&dev).unwrap_or_else(|| "Sortie".into());
                Some(AudioOutput {
                    default: default_id.as_ref() == Some(&id),
                    id,
                    name,
                })
            })
            .collect();
        list.sort_by(|a, b| {
            b.default
                .cmp(&a.default)
                .then(a.name.to_lowercase().cmp(&b.name.to_lowercase()))
        });
        list
    }

    fn call_set_default(
        set: unsafe extern "system" fn(*mut std::ffi::c_void, PCWSTR, ERole) -> HRESULT,
        this: *mut std::ffi::c_void,
        wide: &[u16],
    ) -> Result<(), String> {
        for role in [eConsole, eMultimedia, eCommunications] {
            unsafe { set(this, PCWSTR(wide.as_ptr()), role) }
                .ok()
                .map_err(|e| e.to_string())?;
        }
        Ok(())
    }

    pub fn set_default_output(id: &str) -> Result<(), String> {
        ensure_com();
        let wide: Vec<u16> = id.encode_utf16().chain(std::iter::once(0)).collect();
        let mut last = "PolicyConfig introuvable".to_string();
        unsafe {
            for clsid in [CLSID_POLICY_CONFIG, CLSID_POLICY_CONFIG_VISTA] {
                if let Ok(policy) = CoCreateInstance::<_, IPolicyConfig>(&clsid, None, CLSCTX_ALL) {
                    let this = Interface::as_raw(&policy);
                    let vt = &**(this as *const *const IPolicyConfig_Vtbl);
                    match call_set_default(vt.SetDefaultEndpoint, this, &wide) {
                        Ok(()) => {
                            std::thread::sleep(std::time::Duration::from_millis(80));
                            return Ok(());
                        }
                        Err(e) => last = e,
                    }
                }
                if let Ok(policy) =
                    CoCreateInstance::<_, IPolicyConfigVista>(&clsid, None, CLSCTX_ALL)
                {
                    let this = Interface::as_raw(&policy);
                    let vt = &**(this as *const *const IPolicyConfigVista_Vtbl);
                    match call_set_default(vt.SetDefaultEndpoint, this, &wide) {
                        Ok(()) => {
                            std::thread::sleep(std::time::Duration::from_millis(80));
                            return Ok(());
                        }
                        Err(e) => last = e,
                    }
                }
            }
        }
        Err(last)
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
            let id = device_id(dev)?;
            let name = friendly_name(dev).unwrap_or_else(|| "Sortie".into());
            Some(Raw {
                id,
                name,
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
        let mut raw = Vec::new();
        if let Some(def) = default_device(&enumr) {
            if let Some(row) = device_row(&def) {
                raw.push(row);
            }
        }
        for dev in enum_render_devices(&enumr) {
            session_rows(&dev, &mut raw);
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
        if want == "@device" {
            if let Some(def) = default_device(&enumr) {
                set_device(&def, volume, muted);
            }
            return;
        }
        if is_device_id(&want) {
            if let Some(dev) = find_device(&enumr, &want) {
                set_device(&dev, volume, muted);
            }
            return;
        }
        for dev in enum_render_devices(&enumr) {
            set_session(&dev, &want, volume, muted);
        }
    }
}
