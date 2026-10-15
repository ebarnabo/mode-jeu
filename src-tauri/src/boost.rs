//! Priorité au jeu et allègement de Mode Jeu pendant la session.
use serde::{Deserialize, Serialize};

#[derive(Serialize, Deserialize, Clone, Default)]
pub struct BoostSnapshot {
    pub boosted: Vec<(u32, u32)>,
    pub self_priority: Option<u32>,
    pub self_ecoqos: bool,
    pub timer_period: bool,
}

pub fn apply(keep: &[String], game_exes: &[String]) -> BoostSnapshot {
    let mut snap = BoostSnapshot::default();
    #[cfg(windows)]
    {
        snap.timer_period = timer_period(true);
        snap.self_priority = set_self_priority(below_normal());
        snap.self_ecoqos = set_self_ecoqos(true);
        std::thread::sleep(std::time::Duration::from_millis(180));
        snap.boosted = boost_games(keep, game_exes);
        trim_self();
    }
    #[cfg(not(windows))]
    {
        let _ = (keep, game_exes);
    }
    snap
}

pub fn restore(snap: &BoostSnapshot) {
    #[cfg(windows)]
    {
        set_self_ecoqos(false);
        if let Some(prio) = snap.self_priority {
            set_self_priority(prio);
        }
        for (pid, prio) in &snap.boosted {
            set_pid_priority(*pid, *prio);
        }
        if snap.timer_period {
            timer_period(false);
        }
    }
    #[cfg(not(windows))]
    {
        let _ = snap;
    }
}

#[cfg(windows)]
fn below_normal() -> u32 {
    windows::Win32::System::Threading::BELOW_NORMAL_PRIORITY_CLASS.0
}

#[cfg(windows)]
fn above_normal() -> u32 {
    windows::Win32::System::Threading::ABOVE_NORMAL_PRIORITY_CLASS.0
}

#[cfg(windows)]
fn timer_period(on: bool) -> bool {
    use windows::Win32::Media::{timeBeginPeriod, timeEndPeriod};
    unsafe {
        if on {
            timeBeginPeriod(1) == 0
        } else {
            let _ = timeEndPeriod(1);
            true
        }
    }
}

#[cfg(windows)]
fn set_self_priority(class: u32) -> Option<u32> {
    use windows::Win32::System::Threading::{
        GetCurrentProcess, GetPriorityClass, SetPriorityClass, PROCESS_CREATION_FLAGS,
    };
    unsafe {
        let proc = GetCurrentProcess();
        let old = GetPriorityClass(proc);
        if old == 0 {
            return None;
        }
        let _ = SetPriorityClass(proc, PROCESS_CREATION_FLAGS(class));
        Some(old)
    }
}

#[cfg(windows)]
fn set_self_ecoqos(on: bool) -> bool {
    use windows::Win32::System::Threading::{
        GetCurrentProcess, SetProcessInformation, ProcessPowerThrottling,
        PROCESS_POWER_THROTTLING_EXECUTION_SPEED, PROCESS_POWER_THROTTLING_STATE,
    };
    let mut state = PROCESS_POWER_THROTTLING_STATE {
        Version: 1,
        ControlMask: PROCESS_POWER_THROTTLING_EXECUTION_SPEED,
        StateMask: if on {
            PROCESS_POWER_THROTTLING_EXECUTION_SPEED
        } else {
            0
        },
    };
    unsafe {
        SetProcessInformation(
            GetCurrentProcess(),
            ProcessPowerThrottling,
            &mut state as *mut _ as *mut _,
            std::mem::size_of::<PROCESS_POWER_THROTTLING_STATE>() as u32,
        )
        .is_ok()
    }
}

#[cfg(windows)]
fn set_pid_priority(pid: u32, class: u32) {
    use windows::Win32::Foundation::CloseHandle;
    use windows::Win32::System::Threading::{
        OpenProcess, SetPriorityClass, PROCESS_CREATION_FLAGS, PROCESS_QUERY_LIMITED_INFORMATION,
        PROCESS_SET_INFORMATION,
    };
    unsafe {
        let Ok(h) = OpenProcess(
            PROCESS_SET_INFORMATION | PROCESS_QUERY_LIMITED_INFORMATION,
            false,
            pid,
        ) else {
            return;
        };
        let _ = SetPriorityClass(h, PROCESS_CREATION_FLAGS(class));
        let _ = CloseHandle(h);
    }
}

#[cfg(windows)]
fn trim_self() {
    use windows::Win32::System::ProcessStatus::EmptyWorkingSet;
    use windows::Win32::System::Threading::GetCurrentProcess;
    unsafe {
        let _ = EmptyWorkingSet(GetCurrentProcess());
    }
}

#[cfg(windows)]
fn boost_games(keep: &[String], game_exes: &[String]) -> Vec<(u32, u32)> {
    use windows::Win32::Foundation::CloseHandle;
    use windows::Win32::System::Diagnostics::ToolHelp::{
        CreateToolhelp32Snapshot, Process32FirstW, Process32NextW, PROCESSENTRY32W,
        TH32CS_SNAPPROCESS,
    };
    use windows::Win32::System::Threading::{
        GetPriorityClass, OpenProcess, SetPriorityClass, PROCESS_CREATION_FLAGS,
        PROCESS_QUERY_LIMITED_INFORMATION, PROCESS_SET_INFORMATION,
    };

    let self_pid = std::process::id();
    let games: Vec<String> = game_exes.iter().map(|k| k.to_lowercase()).collect();
    let keep: Vec<String> = keep.iter().map(|k| k.to_lowercase()).collect();
    let mut out = Vec::new();
    if games.is_empty() {
        return out;
    }

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
                let exe = utf16_exe(&pe.szExeFile);
                let pid = pe.th32ProcessID;
                if pid != 0
                    && pid != self_pid
                    && games.iter().any(|k| k == &exe)
                    && !keep.iter().any(|k| k == &exe)
                {
                    if let Ok(h) = OpenProcess(
                        PROCESS_SET_INFORMATION | PROCESS_QUERY_LIMITED_INFORMATION,
                        false,
                        pid,
                    ) {
                        let old = GetPriorityClass(h);
                        if old != 0 {
                            if SetPriorityClass(h, PROCESS_CREATION_FLAGS(above_normal())).is_ok() {
                                out.push((pid, old));
                            }
                        }
                        let _ = CloseHandle(h);
                    }
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

#[cfg(windows)]
fn utf16_exe(buf: &[u16]) -> String {
    let end = buf.iter().position(|&c| c == 0).unwrap_or(buf.len());
    String::from_utf16_lossy(&buf[..end]).to_lowercase()
}
