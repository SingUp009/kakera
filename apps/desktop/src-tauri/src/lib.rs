//! Tauri adapter for `kakera-core`.
//!
//! Mirrors `crates/kakera-wasm`'s in-memory [`MosaicEngine`] design but for the
//! desktop process: tiles + index live in a Tauri-managed [`AppState`], and
//! large RGBA buffers cross the IPC boundary as raw bytes (with metadata in
//! request headers) so they skip the JSON-array encode/decode the default
//! serde-based path would impose.

use kakera_core::{
    build, gather_one, preview, AlphaPolicy, KakeraError, MosaicParams, RgbaImage, RgbaView,
    TileId, TileIndex, TileProvider,
};
use serde::Serialize;
use std::collections::HashMap;
use std::sync::Mutex;
use tauri::{ipc, State};

#[derive(Default)]
struct MosaicEngine {
    tiles: HashMap<TileId, RgbaImage>,
    index: Option<TileIndex>,
}

#[derive(Default)]
struct AppState {
    engine: Mutex<MosaicEngine>,
}

/// Serves decoded tiles straight from the engine's map; no I/O.
struct MapProvider<'a>(&'a HashMap<TileId, RgbaImage>);

impl TileProvider for MapProvider<'_> {
    fn tile(&self, id: TileId) -> kakera_core::Result<RgbaImage> {
        self.0.get(&id).cloned().ok_or(KakeraError::MissingTile(id))
    }
}

#[derive(Serialize)]
struct TilesLoaded {
    count: u32,
    skipped: u32,
}

#[tauri::command]
fn mosaic_reset(state: State<'_, AppState>) {
    let mut eng = state.engine.lock().expect("engine mutex poisoned");
    eng.tiles.clear();
    eng.index = None;
}

#[tauri::command]
fn mosaic_add_tile(
    request: ipc::Request<'_>,
    state: State<'_, AppState>,
) -> Result<(), String> {
    let id = header_u32(&request, "x-tile-id")?;
    let w = header_u32(&request, "x-tile-w")?;
    let h = header_u32(&request, "x-tile-h")?;
    let bytes = match request.body() {
        ipc::InvokeBody::Raw(b) => b.clone(),
        ipc::InvokeBody::Json(_) => {
            return Err("mosaic_add_tile expects a raw byte body".into());
        }
    };
    let img = RgbaImage::new(w, h, bytes).map_err(kakera_err)?;
    state
        .engine
        .lock()
        .expect("engine mutex poisoned")
        .tiles
        .insert(id, img);
    Ok(())
}

#[tauri::command]
fn mosaic_gather(
    grid: u32,
    alpha: u32,
    skipped: u32,
    state: State<'_, AppState>,
) -> Result<TilesLoaded, String> {
    let alpha = u32_to_alpha(alpha)?;
    let mut eng = state.engine.lock().expect("engine mutex poisoned");
    let mut tiles = Vec::with_capacity(eng.tiles.len());
    for (id, img) in &eng.tiles {
        tiles.push(gather_one(*id, &img.view(), grid, alpha).map_err(kakera_err)?);
    }
    let count = tiles.len() as u32;
    eng.index = Some(TileIndex { grid, alpha, tiles });
    Ok(TilesLoaded { count, skipped })
}

#[tauri::command]
fn mosaic_render(
    request: ipc::Request<'_>,
    state: State<'_, AppState>,
) -> Result<ipc::Response, String> {
    let kind = header_str(&request, "x-kind")?.to_owned();
    let tw = header_u32(&request, "x-target-w")?;
    let th = header_u32(&request, "x-target-h")?;
    let params_json = header_str(&request, "x-params")?;
    let params: MosaicParams =
        serde_json::from_str(params_json).map_err(|e| format!("invalid params JSON: {e}"))?;

    let bytes = match request.body() {
        ipc::InvokeBody::Raw(b) => b,
        ipc::InvokeBody::Json(_) => {
            return Err("mosaic_render expects a raw byte body".into());
        }
    };
    let target = RgbaView::new(tw, th, bytes).map_err(kakera_err)?;

    let eng = state.engine.lock().expect("engine mutex poisoned");
    let index = eng
        .index
        .as_ref()
        .ok_or_else(|| "gather must be called before render".to_string())?;

    let out = match kind.as_str() {
        "build" => build(&target, index, &MapProvider(&eng.tiles), &params),
        "preview" => preview(&target, index, &params),
        other => return Err(format!("unknown render kind: {other}")),
    }
    .map_err(kakera_err)?;

    // Pack [width_u32_le, height_u32_le, ...rgba] so the JS side can recover
    // the dimensions without a separate metadata channel.
    let w = out.width();
    let h = out.height();
    let pixels = out.into_bytes();
    let mut payload = Vec::with_capacity(8 + pixels.len());
    payload.extend_from_slice(&w.to_le_bytes());
    payload.extend_from_slice(&h.to_le_bytes());
    payload.extend_from_slice(&pixels);
    Ok(ipc::Response::new(payload))
}

fn u32_to_alpha(v: u32) -> Result<AlphaPolicy, String> {
    match v {
        0 => Ok(AlphaPolicy::Ignore),
        1 => Ok(AlphaPolicy::Weighted),
        other => Err(format!("alpha must be 0 (Ignore) or 1 (Weighted), got {other}")),
    }
}

fn header_u32(req: &ipc::Request<'_>, key: &str) -> Result<u32, String> {
    header_str(req, key)?
        .parse::<u32>()
        .map_err(|e| format!("header {key} is not a valid u32: {e}"))
}

fn header_str<'r>(req: &'r ipc::Request<'_>, key: &str) -> Result<&'r str, String> {
    req.headers()
        .get(key)
        .ok_or_else(|| format!("missing header: {key}"))?
        .to_str()
        .map_err(|e| format!("header {key} is not valid UTF-8: {e}"))
}

fn kakera_err(e: KakeraError) -> String {
    e.to_string()
}

#[cfg_attr(mobile, tauri::mobile_entry_point)]
pub fn run() {
    tauri::Builder::default()
        .manage(AppState::default())
        .setup(|app| {
            if cfg!(debug_assertions) {
                app.handle().plugin(
                    tauri_plugin_log::Builder::default()
                        .level(log::LevelFilter::Info)
                        .build(),
                )?;
            }
            Ok(())
        })
        .invoke_handler(tauri::generate_handler![
            mosaic_reset,
            mosaic_add_tile,
            mosaic_gather,
            mosaic_render
        ])
        .run(tauri::generate_context!())
        .expect("error while running tauri application");
}
