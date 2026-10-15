//! Tiroir temporaire type Yoink : chemins gardés le temps de les redéposer ailleurs.
use serde::Serialize;
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicBool, AtomicU64, Ordering};
use std::sync::Mutex;
use tauri::{AppHandle, Emitter, Manager};

const MAX_ITEMS: usize = 12;

#[derive(Serialize, Clone)]
pub struct ShelfItem {
    pub id: u64,
    pub path: String,
    pub name: String,
    pub is_dir: bool,
}

static ITEMS: Mutex<Vec<ShelfItem>> = Mutex::new(Vec::new());
static NEXT_ID: AtomicU64 = AtomicU64::new(1);
static DRAG_HOVER: AtomicBool = AtomicBool::new(false);

pub fn drag_hover() -> bool {
    DRAG_HOVER.load(Ordering::Relaxed)
}

pub fn set_drag_hover(on: bool) -> bool {
    DRAG_HOVER.swap(on, Ordering::SeqCst) != on
}

pub fn count() -> usize {
    ITEMS.lock().map(|g| g.len()).unwrap_or(0)
}

pub fn expanded() -> bool {
    drag_hover() || count() > 0
}

pub fn list() -> Vec<ShelfItem> {
    let Ok(mut g) = ITEMS.lock() else {
        return Vec::new();
    };
    g.retain(|it| Path::new(&it.path).exists());
    g.clone()
}

pub fn add_paths(paths: Vec<PathBuf>) -> Vec<ShelfItem> {
    let mut added = false;
    if let Ok(mut g) = ITEMS.lock() {
        for p in paths {
            if g.len() >= MAX_ITEMS {
                break;
            }
            let Ok(canon) = std::fs::canonicalize(&p) else {
                if !p.exists() {
                    continue;
                }
                // canonicalize peut échouer sur certains UNC ; on garde le path brut.
                insert(&mut g, p);
                added = true;
                continue;
            };
            if g.iter().any(|it| Path::new(&it.path) == canon) {
                continue;
            }
            insert(&mut g, canon);
            added = true;
        }
    }
    let _ = added;
    list()
}

fn insert(g: &mut Vec<ShelfItem>, path: PathBuf) {
    if g.len() >= MAX_ITEMS {
        return;
    }
    if g.iter().any(|it| Path::new(&it.path) == path.as_path()) {
        return;
    }
    let is_dir = path.is_dir();
    let name = path
        .file_name()
        .map(|s| s.to_string_lossy().into_owned())
        .filter(|s| !s.is_empty())
        .unwrap_or_else(|| path.to_string_lossy().into_owned());
    g.push(ShelfItem {
        id: NEXT_ID.fetch_add(1, Ordering::Relaxed),
        path: path.to_string_lossy().into_owned(),
        name,
        is_dir,
    });
}

pub fn remove(id: u64) -> Vec<ShelfItem> {
    if let Ok(mut g) = ITEMS.lock() {
        g.retain(|it| it.id != id);
    }
    list()
}

pub fn clear() -> Vec<ShelfItem> {
    if let Ok(mut g) = ITEMS.lock() {
        g.clear();
    }
    list()
}

fn path_of(id: u64) -> Result<PathBuf, String> {
    let g = ITEMS.lock().map_err(|_| "tiroir indisponible".to_string())?;
    g.iter()
        .find(|it| it.id == id)
        .map(|it| PathBuf::from(&it.path))
        .filter(|p| p.exists())
        .ok_or_else(|| "Fichier plus disponible.".to_string())
}

pub fn emit(app: &AppHandle) {
    let _ = app.emit("shelf-changed", list());
}

pub fn begin_drag(app: &AppHandle, id: u64) -> Result<(), String> {
    let path = path_of(id)?;
    let canon = std::fs::canonicalize(&path).unwrap_or(path);
    let win = app
        .get_webview_window("overlay")
        .ok_or_else(|| "overlay absent".to_string())?;
    let item = drag::DragItem::Files(vec![canon]);
    // PNG 1×1 transparent — aperçu minimal, zéro I/O.
    const PNG: &[u8] = &[
        0x89, 0x50, 0x4E, 0x47, 0x0D, 0x0A, 0x1A, 0x0A, 0x00, 0x00, 0x00, 0x0D, 0x49, 0x48, 0x44,
        0x52, 0x00, 0x00, 0x00, 0x01, 0x00, 0x00, 0x00, 0x01, 0x08, 0x06, 0x00, 0x00, 0x00, 0x1F,
        0x15, 0xC4, 0x89, 0x00, 0x00, 0x00, 0x0A, 0x49, 0x44, 0x41, 0x54, 0x78, 0x9C, 0x63, 0x00,
        0x01, 0x00, 0x00, 0x05, 0x00, 0x01, 0x0D, 0x0A, 0x2D, 0xB4, 0x00, 0x00, 0x00, 0x00, 0x49,
        0x45, 0x4E, 0x44, 0xAE, 0x42, 0x60, 0x82,
    ];
    drag::start_drag(
        &win,
        item,
        drag::Image::Raw(PNG.to_vec()),
        |_, _| {},
        drag::Options::default(),
    )
    .map_err(|e| format!("Glisser impossible : {e}"))
}
