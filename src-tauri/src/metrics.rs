use serde::Serialize;
use std::sync::Mutex;
use std::time::{Duration, Instant};
use sysinfo::{Components, CpuRefreshKind, MemoryRefreshKind, RefreshKind, System};

#[cfg(windows)]
use std::os::windows::process::CommandExt;
#[cfg(windows)]
const NO_WINDOW: u32 = 0x0800_0000;

#[derive(Serialize, Clone, Default)]
pub struct Metrics {
    pub cpu_pct: f32,
    pub ram_pct: f32,
    pub gpu_pct: Option<f32>,
    pub cpu_temp_c: Option<f32>,
    pub gpu_temp_c: Option<f32>,
    pub fps: Option<f32>,
}

struct Cache {
    sys: System,
    components: Components,
    last_cpu: Instant,
    last_gpu: Instant,
    last_temp: Instant,
    last_fps: Instant,
    gpu_pct: Option<f32>,
    gpu_temp: Option<f32>,
    cpu_temp: Option<f32>,
    fps: Option<f32>,
}

impl Cache {
    fn new() -> Self {
        let mut sys = System::new_with_specifics(
            RefreshKind::new()
                .with_cpu(CpuRefreshKind::everything())
                .with_memory(MemoryRefreshKind::everything()),
        );
        sys.refresh_cpu_all();
        Self {
            sys,
            components: Components::new_with_refreshed_list(),
            last_cpu: Instant::now() - Duration::from_secs(2),
            last_gpu: Instant::now() - Duration::from_secs(5),
            last_temp: Instant::now() - Duration::from_secs(5),
            last_fps: Instant::now() - Duration::from_secs(2),
            gpu_pct: None,
            gpu_temp: None,
            cpu_temp: None,
            fps: None,
        }
    }
}

static CACHE: Mutex<Option<Cache>> = Mutex::new(None);

#[derive(Clone, Copy)]
pub struct MonitorRect {
    pub x: i32,
    pub y: i32,
    pub width: i32,
    pub height: i32,
}

pub fn collect(game_monitor: Option<MonitorRect>) -> Metrics {
    let mut guard = CACHE.lock().unwrap_or_else(|e| e.into_inner());
    if guard.is_none() {
        *guard = Some(Cache::new());
    }
    let cache = guard.as_mut().unwrap();
    let now = Instant::now();

    if now.duration_since(cache.last_cpu) >= Duration::from_millis(400) {
        cache.sys.refresh_cpu_usage();
        cache.sys.refresh_memory();
        cache.last_cpu = now;
    }

    if now.duration_since(cache.last_gpu) >= Duration::from_secs(2) {
        let (util, temp) = query_gpu();
        cache.gpu_pct = util;
        cache.gpu_temp = temp;
        cache.last_gpu = now;
    }

    if now.duration_since(cache.last_temp) >= Duration::from_secs(3) {
        cache.components.refresh();
        cache.cpu_temp = cpu_temp_from_components(&cache.components);
        cache.last_temp = now;
    }

    if now.duration_since(cache.last_fps) >= Duration::from_millis(800) {
        cache.fps = query_fps(game_monitor);
        cache.last_fps = now;
    }

    let cpu_pct = cache.sys.global_cpu_usage();
    let ram_pct = if cache.sys.total_memory() > 0 {
        (cache.sys.used_memory() as f32 / cache.sys.total_memory() as f32) * 100.0
    } else {
        0.0
    };

    Metrics {
        cpu_pct,
        ram_pct,
        gpu_pct: cache.gpu_pct,
        cpu_temp_c: cache.cpu_temp,
        gpu_temp_c: cache.gpu_temp,
        fps: cache.fps,
    }
}

fn cpu_temp_from_components(components: &Components) -> Option<f32> {
    let mut best: Option<f32> = None;
    for c in components.iter() {
        let t = c.temperature();
        if !t.is_finite() || !(1.0..=110.0).contains(&t) {
            continue;
        }
        let label = c.label().to_lowercase();
        if label.contains("cpu")
            || label.contains("package")
            || label.contains("tdie")
            || label.contains("tctl")
            || label.contains("computer")
        {
            return Some(t);
        }
        best = Some(best.map_or(t, |b| b.max(t)));
    }
    best
}

fn query_gpu() -> (Option<f32>, Option<f32>) {
    let out = run_hidden(
        "nvidia-smi",
        &[
            "--query-gpu=utilization.gpu,temperature.gpu",
            "--format=csv,noheader,nounits",
        ],
    );
    if let Some(text) = out {
        if let Some(line) = text.lines().next() {
            let parts: Vec<_> = line.split(',').map(|s| s.trim()).collect();
            if parts.len() >= 2 {
                return (parts[0].parse().ok(), parts[1].parse().ok());
            }
        }
    }
    (None, None)
}

fn run_hidden(cmd: &str, args: &[&str]) -> Option<String> {
    let mut c = std::process::Command::new(cmd);
    c.args(args);
    #[cfg(windows)]
    c.creation_flags(NO_WINDOW);
    c.output()
        .ok()
        .filter(|o| o.status.success())
        .map(|o| String::from_utf8_lossy(&o.stdout).to_string())
}

fn query_fps(game_monitor: Option<MonitorRect>) -> Option<f32> {
    #[cfg(windows)]
    {
        let pid = game_monitor.and_then(top_window_pid_on_monitor);
        if let Some(fps) = read_rtss_fps(pid) {
            return Some(fps);
        }
    }
    let _ = game_monitor;
    None
}

#[cfg(windows)]
fn top_window_pid_on_monitor(mon: MonitorRect) -> Option<u32> {
    use windows::Win32::Foundation::{BOOL, HWND, LPARAM, RECT};
    use windows::Win32::UI::WindowsAndMessaging::{
        EnumWindows, GetWindowRect, GetWindowThreadProcessId, IsIconic, IsWindowVisible,
    };

    struct Acc {
        mon: MonitorRect,
        best_area: i32,
        best_pid: u32,
    }

    unsafe extern "system" fn callback(hwnd: HWND, lparam: LPARAM) -> BOOL {
        let acc = &mut *(lparam.0 as *mut Acc);
        if !IsWindowVisible(hwnd).as_bool() || IsIconic(hwnd).as_bool() {
            return BOOL(1);
        }
        let mut rect = RECT::default();
        if GetWindowRect(hwnd, &mut rect).is_err() {
            return BOOL(1);
        }
        let area = intersection_area(acc.mon, rect);
        if area > acc.best_area && area > (acc.mon.width * acc.mon.height) / 4 {
            let mut pid = 0u32;
            GetWindowThreadProcessId(hwnd, Some(&mut pid));
            if pid != 0 && pid != std::process::id() {
                acc.best_area = area;
                acc.best_pid = pid;
            }
        }
        BOOL(1)
    }

    let mut acc = Acc {
        mon,
        best_area: 0,
        best_pid: 0,
    };
    unsafe {
        let _ = EnumWindows(Some(callback), LPARAM(&mut acc as *mut _ as isize));
    }
    (acc.best_pid != 0).then_some(acc.best_pid)
}

#[cfg(windows)]
fn intersection_area(mon: MonitorRect, rect: windows::Win32::Foundation::RECT) -> i32 {
    let x1 = mon.x.max(rect.left);
    let y1 = mon.y.max(rect.top);
    let x2 = (mon.x + mon.width).min(rect.right);
    let y2 = (mon.y + mon.height).min(rect.bottom);
    ((x2 - x1).max(0)) * ((y2 - y1).max(0))
}

#[cfg(windows)]
fn read_rtss_fps(pid: Option<u32>) -> Option<f32> {
    use windows::core::HSTRING;
    use windows::Win32::Foundation::CloseHandle;
    use windows::Win32::System::Memory::{
        MapViewOfFile, OpenFileMappingW, UnmapViewOfFile, FILE_MAP_READ,
    };

    unsafe {
        let mapping = OpenFileMappingW(
            FILE_MAP_READ.0,
            false,
            &HSTRING::from("RTSSSharedMemory_V2"),
        )
        .ok()?;
        let view = MapViewOfFile(mapping, FILE_MAP_READ, 0, 0, 0);
        if view.Value.is_null() {
            let _ = CloseHandle(mapping);
            return None;
        }

        let base = view.Value as *const u8;
        let signature = std::ptr::read_unaligned(base as *const u32);
        if signature != 0x5353_5452 {
            let _ = UnmapViewOfFile(view);
            let _ = CloseHandle(mapping);
            return None;
        }

        let app_entry_size = std::ptr::read_unaligned(base.add(8) as *const u32) as usize;
        let app_arr_offset = std::ptr::read_unaligned(base.add(12) as *const u32) as usize;
        let app_arr_size = std::ptr::read_unaligned(base.add(16) as *const u32) as usize;
        if app_entry_size < 280 || app_arr_size == 0 || app_arr_size > 256 {
            let _ = UnmapViewOfFile(view);
            let _ = CloseHandle(mapping);
            return None;
        }

        let mut best = 0f32;
        for i in 0..app_arr_size {
            let entry = base.add(app_arr_offset + i * app_entry_size);
            let process_id = std::ptr::read_unaligned(entry as *const u32);
            if process_id == 0 {
                continue;
            }
            let time0 = std::ptr::read_unaligned(entry.add(268) as *const u32);
            let time1 = std::ptr::read_unaligned(entry.add(272) as *const u32);
            let frames = std::ptr::read_unaligned(entry.add(276) as *const u32);
            let frame_time = std::ptr::read_unaligned(entry.add(280) as *const u32);

            let mut fps = 0f32;
            if time1 > time0 && frames > 0 {
                fps = 1000.0 * frames as f32 / (time1 - time0) as f32;
            } else if frame_time > 0 && frame_time < 1_000_000 {
                fps = 1_000_000.0 / frame_time as f32;
            }
            if !(1.0..=500.0).contains(&fps) {
                continue;
            }
            if pid == Some(process_id) {
                best = fps;
                break;
            }
            best = best.max(fps);
        }

        let _ = UnmapViewOfFile(view);
        let _ = CloseHandle(mapping);
        (best > 0.0).then_some(best)
    }
}
