//! Lecture GPU AMD via ADL — contexte réutilisé (pas de create/destroy à chaque poll).
#![cfg(windows)]

use std::ffi::c_void;
use std::sync::Mutex;
use windows::core::PCSTR;
use windows::Win32::Foundation::HMODULE;
use windows::Win32::System::LibraryLoader::{GetProcAddress, LoadLibraryA};

type AdlMalloc = unsafe extern "C" fn(i32) -> *mut c_void;
type AdlMainCreate = unsafe extern "C" fn(AdlMalloc, i32, *mut *mut c_void) -> i32;
type AdlMainDestroy = unsafe extern "C" fn(*mut c_void) -> i32;
type AdlAdapterCount = unsafe extern "C" fn(*mut c_void, *mut i32) -> i32;
type AdlOd5TempGet = unsafe extern "C" fn(*mut c_void, i32, i32, *mut AdlTemperature) -> i32;
type AdlOd5ActivityGet = unsafe extern "C" fn(*mut c_void, i32, *mut AdlPmActivity) -> i32;
type AdlOdNTempGet = unsafe extern "C" fn(*mut c_void, i32, i32, *mut i32) -> i32;
type AdlPmLogGet = unsafe extern "C" fn(*mut c_void, i32, *mut AdlPmLogOutput) -> i32;

const ADL_OK: i32 = 0;
const ODN_GPU_EDGE_TEMP: i32 = 1;
const PMLOG_TEMP_EDGE: usize = 1;
const PMLOG_TEMP_HOTSPOT: usize = 7;
const PMLOG_INFO_ACTIVITY_GFX: usize = 18;
const PMLOG_MAX_SENSORS: usize = 256;

#[repr(C)]
struct AdlTemperature {
    size: i32,
    temperature: i32,
}

#[repr(C)]
struct AdlPmActivity {
    size: i32,
    engine_clock: i32,
    memory_clock: i32,
    vddc: i32,
    activity_percent: i32,
    current_performance_level: i32,
    current_bus_speed: i32,
    current_bus_lanes: i32,
    maximum_bus_lanes: i32,
    reserved: i32,
}

#[repr(C)]
#[derive(Clone, Copy)]
struct AdlSingleSensor {
    supported: i32,
    value: i32,
}

#[repr(C)]
struct AdlPmLogOutput {
    size: i32,
    sensors: [AdlSingleSensor; PMLOG_MAX_SENSORS],
}

struct AdlSession {
    _lib: HMODULE,
    _destroy: AdlMainDestroy,
    od5_temp: Option<AdlOd5TempGet>,
    od5_activity: Option<AdlOd5ActivityGet>,
    odn_temp: Option<AdlOdNTempGet>,
    pmlog: Option<AdlPmLogGet>,
    ctx: *mut c_void,
    adapters: i32,
}

unsafe impl Send for AdlSession {}

static SESSION: Mutex<Option<AdlSession>> = Mutex::new(None);
static LOAD_FAILED: std::sync::atomic::AtomicBool = std::sync::atomic::AtomicBool::new(false);

unsafe extern "C" fn adl_malloc(size: i32) -> *mut c_void {
    if size <= 0 {
        return std::ptr::null_mut();
    }
    let Ok(layout) = std::alloc::Layout::from_size_align(size as usize, 16) else {
        return std::ptr::null_mut();
    };
    std::alloc::alloc(layout) as *mut c_void
}

fn load_proc<T>(lib: HMODULE, name: &[u8]) -> Option<T> {
    unsafe {
        let p = GetProcAddress(lib, PCSTR::from_raw(name.as_ptr()))?;
        Some(std::mem::transmute_copy(&p))
    }
}

fn ensure_session(guard: &mut Option<AdlSession>) -> bool {
    if guard.is_some() {
        return true;
    }
    if LOAD_FAILED.load(std::sync::atomic::Ordering::Relaxed) {
        return false;
    }
    unsafe {
        for dll in [b"atiadlxx.dll\0", b"amdadlxx.dll\0", b"atiadlxy.dll\0"] {
            let Ok(lib) = LoadLibraryA(PCSTR::from_raw(dll.as_ptr())) else {
                continue;
            };
            if lib.is_invalid() {
                continue;
            }
            let Some(create) = load_proc::<AdlMainCreate>(lib, b"ADL2_Main_Control_Create\0")
            else {
                continue;
            };
            let Some(destroy) =
                load_proc::<AdlMainDestroy>(lib, b"ADL2_Main_Control_Destroy\0")
            else {
                continue;
            };
            let Some(count) =
                load_proc::<AdlAdapterCount>(lib, b"ADL2_Adapter_NumberOfAdapters_Get\0")
            else {
                continue;
            };

            let mut ctx: *mut c_void = std::ptr::null_mut();
            if create(adl_malloc, 1, &mut ctx) != ADL_OK || ctx.is_null() {
                continue;
            }
            let mut num = 0i32;
            if count(ctx, &mut num) != ADL_OK || num <= 0 {
                let _ = destroy(ctx);
                continue;
            }

            *guard = Some(AdlSession {
                _lib: lib,
                _destroy: destroy,
                od5_temp: load_proc(lib, b"ADL2_Overdrive5_Temperature_Get\0"),
                od5_activity: load_proc(lib, b"ADL2_Overdrive5_CurrentActivity_Get\0"),
                odn_temp: load_proc(lib, b"ADL2_OverdriveN_Temperature_Get\0"),
                pmlog: load_proc(lib, b"ADL2_New_QueryPMLogData_Get\0"),
                ctx,
                adapters: num.min(8),
            });
            return true;
        }
    }
    LOAD_FAILED.store(true, std::sync::atomic::Ordering::Relaxed);
    false
}

fn sensor_value(out: &AdlPmLogOutput, idx: usize) -> Option<i32> {
    let s = out.sensors.get(idx)?;
    (s.supported != 0).then_some(s.value)
}

pub fn available() -> bool {
    let mut guard = SESSION.lock().unwrap_or_else(|e| e.into_inner());
    ensure_session(&mut guard)
}

pub fn query_amd_gpu() -> (Option<f32>, Option<f32>) {
    let mut guard = SESSION.lock().unwrap_or_else(|e| e.into_inner());
    if !ensure_session(&mut guard) {
        return (None, None);
    }
    let Some(session) = guard.as_ref() else {
        return (None, None);
    };

    unsafe {
        let mut best_util: Option<f32> = None;
        let mut best_temp: Option<f32> = None;

        for adapter_index in 0..session.adapters {
            if let Some(pmlog) = session.pmlog {
                let mut out = AdlPmLogOutput {
                    size: std::mem::size_of::<AdlPmLogOutput>() as i32,
                    sensors: [AdlSingleSensor {
                        supported: 0,
                        value: 0,
                    }; PMLOG_MAX_SENSORS],
                };
                if pmlog(session.ctx, adapter_index, &mut out) == ADL_OK {
                    if let Some(v) = sensor_value(&out, PMLOG_INFO_ACTIVITY_GFX) {
                        let u = v as f32;
                        if (0.0..=100.0).contains(&u) {
                            best_util = Some(best_util.map_or(u, |b| b.max(u)));
                        }
                    }
                    for idx in [PMLOG_TEMP_EDGE, PMLOG_TEMP_HOTSPOT] {
                        if let Some(v) = sensor_value(&out, idx) {
                            let c = v as f32;
                            if (1.0..=110.0).contains(&c) {
                                best_temp = Some(best_temp.map_or(c, |b| b.max(c)));
                            }
                        }
                    }
                }
            }

            if best_util.is_none() {
                if let Some(od5) = session.od5_activity {
                    let mut act = std::mem::zeroed::<AdlPmActivity>();
                    act.size = std::mem::size_of::<AdlPmActivity>() as i32;
                    if od5(session.ctx, adapter_index, &mut act) == ADL_OK {
                        let u = act.activity_percent as f32;
                        if (0.0..=100.0).contains(&u) {
                            best_util = Some(u);
                        }
                    }
                }
            }

            if best_temp.is_none() {
                if let Some(odn) = session.odn_temp {
                    let mut t = 0i32;
                    if odn(session.ctx, adapter_index, ODN_GPU_EDGE_TEMP, &mut t) == ADL_OK {
                        let c = t as f32;
                        if (1.0..=110.0).contains(&c) {
                            best_temp = Some(c);
                        }
                    }
                }
            }

            if best_temp.is_none() {
                if let Some(od5t) = session.od5_temp {
                    let mut temp = AdlTemperature {
                        size: std::mem::size_of::<AdlTemperature>() as i32,
                        temperature: 0,
                    };
                    if od5t(session.ctx, adapter_index, 0, &mut temp) == ADL_OK {
                        let c = temp.temperature as f32 / 1000.0;
                        if (1.0..=110.0).contains(&c) {
                            best_temp = Some(c);
                        }
                    }
                }
            }

            if best_util.is_some() && best_temp.is_some() {
                break;
            }
        }

        (best_util, best_temp)
    }
}
