//! Lecture / pause du média Windows actuellement exposé au SMTC.
use serde::Serialize;

#[derive(Serialize, Clone, Default)]
pub struct MediaNow {
    pub playing: bool,
    pub title: Option<String>,
    pub artist: Option<String>,
}

#[cfg(windows)]
mod win {
    use super::MediaNow;
    use std::mem::size_of;
    use std::sync::Mutex;
    use windows::Media::Control::{
        GlobalSystemMediaTransportControlsSessionManager as Manager,
        GlobalSystemMediaTransportControlsSessionPlaybackStatus as Status,
    };
    use windows::Win32::UI::Input::KeyboardAndMouse::{
        SendInput, INPUT, INPUT_0, INPUT_KEYBOARD, KEYBDINPUT, KEYEVENTF_KEYUP, VIRTUAL_KEY,
    };

    static MGR: Mutex<Option<Manager>> = Mutex::new(None);
    static CACHE: Mutex<Option<(std::time::Instant, MediaNow)>> = Mutex::new(None);

    fn manager() -> Option<Manager> {
        if let Ok(g) = MGR.lock() {
            if let Some(m) = g.as_ref() {
                return Some(m.clone());
            }
        }
        let m = Manager::RequestAsync().ok()?.get().ok()?;
        if let Ok(mut g) = MGR.lock() {
            *g = Some(m.clone());
        }
        Some(m)
    }

    pub fn now() -> MediaNow {
        if let Ok(g) = CACHE.lock() {
            if let Some((at, cached)) = g.as_ref() {
                if at.elapsed() < std::time::Duration::from_millis(2500) {
                    return cached.clone();
                }
            }
        }
        let fresh = now_uncached();
        if let Ok(mut g) = CACHE.lock() {
            *g = Some((std::time::Instant::now(), fresh.clone()));
        }
        fresh
    }

    fn now_uncached() -> MediaNow {
        let Some(mgr) = manager() else {
            return MediaNow::default();
        };
        let Ok(session) = mgr.GetCurrentSession() else {
            return MediaNow::default();
        };
        let playing = session
            .GetPlaybackInfo()
            .ok()
            .and_then(|i| i.PlaybackStatus().ok())
            .map(|s| s == Status::Playing)
            .unwrap_or(false);
        let props = session
            .TryGetMediaPropertiesAsync()
            .ok()
            .and_then(|op| op.get().ok());
        let title = props
            .as_ref()
            .and_then(|p| p.Title().ok())
            .map(|s| s.to_string())
            .filter(|s| !s.is_empty());
        let artist = props
            .as_ref()
            .and_then(|p| p.Artist().ok())
            .map(|s| s.to_string())
            .filter(|s| !s.is_empty());
        MediaNow {
            playing,
            title,
            artist,
        }
    }

    pub fn next() -> MediaNow {
        if !act(|s| s.TrySkipNextAsync()) {
            media_key(0xB0);
        }
        now()
    }

    pub fn previous() -> MediaNow {
        if !act(|s| s.TrySkipPreviousAsync()) {
            media_key(0xB1);
        }
        now()
    }

    pub fn toggle() -> MediaNow {
        if !act(|s| s.TryTogglePlayPauseAsync()) {
            media_key(0xB3);
        }
        now()
    }

    fn act<F>(f: F) -> bool
    where
        F: FnOnce(
            windows::Media::Control::GlobalSystemMediaTransportControlsSession,
        ) -> windows::core::Result<windows::Foundation::IAsyncOperation<bool>>,
    {
        if let Some(mgr) = manager() {
            if let Ok(session) = mgr.GetCurrentSession() {
                let _ = f(session).and_then(|op| op.get());
                if let Ok(mut g) = CACHE.lock() {
                    *g = None;
                }
                return true;
            }
        }
        false
    }

    fn media_key(vk: u16) {
        let key = VIRTUAL_KEY(vk);
        let down = INPUT {
            r#type: INPUT_KEYBOARD,
            Anonymous: INPUT_0 {
                ki: KEYBDINPUT {
                    wVk: key,
                    wScan: 0,
                    dwFlags: Default::default(),
                    time: 0,
                    dwExtraInfo: 0,
                },
            },
        };
        let up = INPUT {
            r#type: INPUT_KEYBOARD,
            Anonymous: INPUT_0 {
                ki: KEYBDINPUT {
                    wVk: key,
                    wScan: 0,
                    dwFlags: KEYEVENTF_KEYUP,
                    time: 0,
                    dwExtraInfo: 0,
                },
            },
        };
        let inputs = [down, up];
        unsafe {
            let _ = SendInput(&inputs, size_of::<INPUT>() as i32);
        }
    }
}

#[cfg(not(windows))]
mod win {
    use super::MediaNow;
    pub fn now() -> MediaNow {
        MediaNow::default()
    }
    pub fn toggle() -> MediaNow {
        MediaNow::default()
    }
    pub fn next() -> MediaNow {
        MediaNow::default()
    }
    pub fn previous() -> MediaNow {
        MediaNow::default()
    }
}

pub fn now() -> MediaNow {
    win::now()
}

pub fn toggle() -> MediaNow {
    win::toggle()
}

pub fn next() -> MediaNow {
    win::next()
}

pub fn previous() -> MediaNow {
    win::previous()
}
