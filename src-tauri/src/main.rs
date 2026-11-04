#![cfg_attr(not(debug_assertions), windows_subsystem = "windows")]

mod engine;

use engine::{Drive, Item, Root, Rows, TNode};
use serde::Serialize;
use std::{
    path::PathBuf,
    sync::{
        atomic::{AtomicBool, Ordering::Relaxed},
        Arc, Mutex,
    },
    time::Duration,
};
use tauri::{async_runtime::spawn_blocking, AppHandle, Emitter, State};

struct AppState(Mutex<Option<Root>>);

/// Exécute `f` sur la racine analysée ; erreur lisible si rien n'est analysé ou si le chemin n'existe pas.
fn with_root<T>(state: &AppState, f: impl FnOnce(&Root) -> Option<T>) -> Result<T, String> {
    let g = state.0.lock().map_err(|e| e.to_string())?;
    g.as_ref().and_then(f).ok_or_else(|| "Élément introuvable dans l'analyse (rescannez).".to_string())
}

#[tauri::command]
fn initial_path() -> Option<String> {
    std::env::args().nth(1).map(|a| engine::normalize(&a)).filter(|p| p.is_dir()).map(|p| p.to_string_lossy().into_owned())
}

#[tauri::command]
fn drives() -> Vec<Drive> {
    engine::drives()
}

#[tauri::command]
async fn scan(app: AppHandle, state: State<'_, AppState>, path: String) -> Result<Item, String> {
    let root = engine::normalize(&path);
    if !root.is_dir() {
        return Err(format!("Dossier introuvable : {path}"));
    }
    engine::SEEN.store(0, Relaxed);
    let stop = Arc::new(AtomicBool::new(false));
    let (s2, a2) = (stop.clone(), app.clone());
    std::thread::spawn(move || {
        while !s2.load(Relaxed) {
            let _ = a2.emit("scan-progress", engine::SEEN.load(Relaxed));
            std::thread::sleep(Duration::from_millis(100));
        }
    });
    let (rp, name) = (root.clone(), engine::display_name(&root));
    let node = spawn_blocking(move || engine::scan(&rp, name)).await.map_err(|e| e.to_string());
    stop.store(true, Relaxed);
    let node = node?;
    let item = engine::item(&node, &root, engine::display_name(&root));
    *state.0.lock().map_err(|e| e.to_string())? = Some(Root { path: root, node });
    Ok(item)
}

#[derive(Serialize)]
struct Page {
    items: Vec<Item>,
    more: usize,
}

#[tauri::command]
fn children(state: State<AppState>, path: String, limit: usize) -> Result<Page, String> {
    let p = PathBuf::from(&path);
    with_root(&state, |r| {
        let n = engine::find(r, &p)?;
        Some(Page {
            items: n.children.iter().take(limit).map(|c| engine::item(c, &p.join(&c.name), c.name.clone())).collect(),
            more: n.children.len().saturating_sub(limit),
        })
    })
}

#[tauri::command]
fn info(state: State<AppState>, path: String) -> Result<Item, String> {
    let p = PathBuf::from(&path);
    with_root(&state, |r| engine::find(r, &p).map(|n| engine::item(n, &p, engine::display_name(&p))))
}

#[tauri::command]
fn treemap(state: State<AppState>, path: String) -> Result<TNode, String> {
    let p = PathBuf::from(&path);
    with_root(&state, |r| engine::find(r, &p).map(|n| engine::treemap(n, &p, 3)))
}

#[tauri::command]
fn caches(state: State<AppState>, path: String) -> Result<Rows, String> {
    let p = PathBuf::from(&path);
    with_root(&state, |r| engine::find(r, &p).map(|n| engine::find_caches(n, &p)))
}

#[tauri::command]
async fn dupes(state: State<'_, AppState>, path: String, min_size: u64) -> Result<Rows, String> {
    let p = PathBuf::from(&path);
    let mut files = vec![];
    with_root(&state, |r| {
        engine::find(r, &p).map(|n| engine::collect_files(n, &p, min_size, &mut files))
    })?;
    spawn_blocking(move || engine::find_dupes(files)).await.map_err(|e| e.to_string())
}

#[derive(Serialize)]
struct Trashed {
    removed: usize,
    freed: u64,
    skipped: usize,
    failed: usize,
    error: Option<String>,
}

#[tauri::command]
async fn trash_paths(state: State<'_, AppState>, paths: Vec<String>) -> Result<Trashed, String> {
    let all: Vec<PathBuf> = paths.iter().map(PathBuf::from).collect();
    // vérifié sur le chemin reçu ET sur sa forme canonique (noms courts 8.3, liens, `..`)
    let canon = |p: &PathBuf| std::fs::canonicalize(p).unwrap_or_else(|_| p.clone());
    let safe: Vec<PathBuf> = all.iter().filter(|p| !engine::is_protected(p) && !engine::is_protected(&canon(p))).cloned().collect();
    let skipped = all.len() - safe.len();
    let todo = safe.clone();
    let (gone, error) = spawn_blocking(move || {
        let mut error = None;
        // lot d'abord ; s'il échoue (un fichier verrouillé suffit), on reprend élément par élément
        if !todo.is_empty() && trash::delete_all(&todo).is_err() {
            for p in &todo {
                if std::fs::symlink_metadata(p).is_ok() {
                    if let Err(e) = trash::delete(p) {
                        error.get_or_insert(format!("{} : {e}", p.display()));
                    }
                }
            }
        }
        (todo.into_iter().filter(|p| std::fs::symlink_metadata(p).is_err()).collect::<Vec<_>>(), error)
    })
    .await
    .map_err(|e| e.to_string())?;
    let mut g = state.0.lock().map_err(|e| e.to_string())?;
    let mut freed = 0;
    if let Some(root) = g.as_mut() {
        for p in &gone {
            freed += engine::remove(root, p).map_or(0, |(s, _)| s);
        }
    }
    Ok(Trashed { removed: gone.len(), freed, skipped, failed: safe.len() - gone.len(), error })
}

#[cfg(windows)]
#[link(name = "shell32")]
extern "system" {
    fn IsUserAnAdmin() -> i32;
}

#[tauri::command]
fn is_admin() -> bool {
    #[cfg(windows)]
    unsafe {
        IsUserAnAdmin() != 0
    }
    #[cfg(not(windows))]
    false
}

/// Rouvre DCleaner élevé (UAC) sur `path`, puis quitte cette instance. Annulation de l'UAC : on reste ici.
#[tauri::command]
fn relaunch_admin(app: AppHandle, path: Option<String>) -> Result<(), String> {
    #[cfg(not(windows))]
    return Err("Disponible uniquement sous Windows".into());
    #[cfg(windows)]
    {
        use std::os::windows::process::CommandExt;
        let q = |s: &str| s.replace('\'', "''");
        let exe = std::env::current_exe().map_err(|e| e.to_string())?;
        let args = path.map(|p| format!(" -ArgumentList '\"{}\"'", q(&p))).unwrap_or_default();
        let cmd = format!("Start-Process -Verb RunAs -FilePath '{}'{args}", q(&exe.to_string_lossy()));
        let ok = std::process::Command::new("powershell")
            .args(["-NoProfile", "-WindowStyle", "Hidden", "-Command", &cmd])
            .creation_flags(0x0800_0000) // CREATE_NO_WINDOW
            .status()
            .map_err(|e| e.to_string())?
            .success();
        if !ok {
            return Err("Élévation refusée ou annulée.".into());
        }
        app.exit(0);
        Ok(())
    }
}

#[tauri::command]
fn reveal(path: String) {
    #[cfg(windows)]
    {
        use std::os::windows::process::CommandExt;
        let _ = std::process::Command::new("explorer").raw_arg(format!("/select,\"{path}\"")).spawn();
    }
    #[cfg(not(windows))]
    {
        let _ = std::process::Command::new("xdg-open").arg(path).spawn();
    }
}

fn main() {
    let _ = rayon::ThreadPoolBuilder::new().stack_size(16 << 20).build_global();
    tauri::Builder::default()
        .plugin(tauri_plugin_dialog::init())
        .manage(AppState(Mutex::new(None)))
        .invoke_handler(tauri::generate_handler![
            initial_path, drives, scan, children, info, treemap, caches, dupes, trash_paths, reveal, is_admin, relaunch_admin
        ])
        .run(tauri::generate_context!())
        .expect("erreur au lancement de DCleaner");
}
