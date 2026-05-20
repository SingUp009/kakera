//! WASM adapter for `kakera-core`.
//!
//! The browser decodes images (target + tiles) to tightly-packed RGBA8 and
//! feeds them in. [`MosaicEngine`] holds every decoded tile in WASM linear
//! memory and serves them through an in-memory [`TileProvider`], so the heavy
//! `build`/`preview` loop never crosses the JS boundary per cell.
//!
//! Memory note: tiles are never freed until [`MosaicEngine::reset`] or the
//! whole module is dropped (worker terminate). Callers must thumbnail tiles
//! before [`MosaicEngine::add_tile`] to bound footprint.

use kakera_core::{
    build, gather_one, preview, AlphaPolicy, KakeraError, MosaicParams, RgbaImage, RgbaView,
    TileId, TileIndex, TileProvider,
};
use std::collections::HashMap;
use wasm_bindgen::prelude::*;

#[wasm_bindgen(start)]
pub fn _start() {
    #[cfg(feature = "console_error_panic_hook")]
    console_error_panic_hook::set_once();
}

/// Rendered mosaic handed back to JS. `take_bytes` consumes self so the RGBA
/// buffer is moved out once (no lingering WASM-side copy).
#[wasm_bindgen]
pub struct MosaicOutput {
    width: u32,
    height: u32,
    data: Vec<u8>,
}

#[wasm_bindgen]
impl MosaicOutput {
    #[wasm_bindgen(getter)]
    pub fn width(&self) -> u32 {
        self.width
    }

    #[wasm_bindgen(getter)]
    pub fn height(&self) -> u32 {
        self.height
    }

    pub fn take_bytes(self) -> Vec<u8> {
        self.data
    }
}

/// Serves decoded tiles straight from the engine's map; no I/O, no FFI.
struct MapProvider<'a>(&'a HashMap<TileId, RgbaImage>);

impl TileProvider for MapProvider<'_> {
    fn tile(&self, id: TileId) -> kakera_core::Result<RgbaImage> {
        self.0.get(&id).cloned().ok_or(KakeraError::MissingTile(id))
    }
}

#[wasm_bindgen]
pub struct MosaicEngine {
    tiles: HashMap<TileId, RgbaImage>,
    index: Option<TileIndex>,
}

impl Default for MosaicEngine {
    fn default() -> Self {
        Self::new()
    }
}

#[wasm_bindgen]
impl MosaicEngine {
    #[wasm_bindgen(constructor)]
    pub fn new() -> MosaicEngine {
        MosaicEngine {
            tiles: HashMap::new(),
            index: None,
        }
    }

    /// Store one decoded tile. `rgba` must be exactly `w * h * 4` bytes.
    pub fn add_tile(&mut self, id: u32, w: u32, h: u32, rgba: &[u8]) -> Result<(), JsValue> {
        let img = RgbaImage::new(w, h, rgba.to_vec()).map_err(kakera_err)?;
        self.tiles.insert(id, img);
        Ok(())
    }

    pub fn tile_count(&self) -> usize {
        self.tiles.len()
    }

    /// Drop all tiles and the index. Note: WASM linear memory is not returned
    /// to the OS; terminate the worker to truly reclaim it.
    pub fn reset(&mut self) {
        self.tiles.clear();
        self.index = None;
    }

    /// Build the reusable color-feature index over all loaded tiles.
    /// `alpha`: 0 = Ignore, 1 = Weighted.
    pub fn gather(&mut self, grid: u32, alpha: u32) -> Result<(), JsValue> {
        let alpha = u32_to_alpha(alpha).map_err(js_err)?;
        let mut tiles = Vec::with_capacity(self.tiles.len());
        for (id, img) in &self.tiles {
            tiles.push(gather_one(*id, &img.view(), grid, alpha).map_err(kakera_err)?);
        }
        self.index = Some(TileIndex { grid, alpha, tiles });
        Ok(())
    }

    /// Full mosaic: each cell painted with matched tile pixels.
    pub fn build(
        &self,
        tw: u32,
        th: u32,
        target_rgba: &[u8],
        params_json: &str,
    ) -> Result<MosaicOutput, JsValue> {
        let index = self.require_index()?;
        let params = parse_params(params_json).map_err(js_err)?;
        let target = RgbaView::new(tw, th, target_rgba).map_err(kakera_err)?;
        let out =
            build(&target, index, &MapProvider(&self.tiles), &params).map_err(kakera_err)?;
        Ok(MosaicOutput {
            width: out.width(),
            height: out.height(),
            data: out.into_bytes(),
        })
    }

    /// Cheap preview: cells flat-filled with the matched tile's average color.
    pub fn preview(
        &self,
        tw: u32,
        th: u32,
        target_rgba: &[u8],
        params_json: &str,
    ) -> Result<MosaicOutput, JsValue> {
        let index = self.require_index()?;
        let params = parse_params(params_json).map_err(js_err)?;
        let target = RgbaView::new(tw, th, target_rgba).map_err(kakera_err)?;
        let out = preview(&target, index, &params).map_err(kakera_err)?;
        Ok(MosaicOutput {
            width: out.width(),
            height: out.height(),
            data: out.into_bytes(),
        })
    }
}

impl MosaicEngine {
    fn require_index(&self) -> Result<&TileIndex, JsValue> {
        self.index
            .as_ref()
            .ok_or_else(|| js_err("gather() must be called before build/preview".to_string()))
    }
}

/// Map alpha discriminant from JS. Kept separate from the params-JSON `alpha`
/// (which serde encodes as the variant string) — the worker bridges the two.
fn u32_to_alpha(v: u32) -> Result<AlphaPolicy, String> {
    match v {
        0 => Ok(AlphaPolicy::Ignore),
        1 => Ok(AlphaPolicy::Weighted),
        other => Err(format!("alpha must be 0 (Ignore) or 1 (Weighted), got {other}")),
    }
}

fn parse_params(s: &str) -> Result<MosaicParams, String> {
    serde_json::from_str(s).map_err(|e| format!("invalid params JSON: {e}"))
}

fn kakera_err(e: KakeraError) -> JsValue {
    JsValue::from_str(&e.to_string())
}

fn js_err(message: String) -> JsValue {
    JsValue::from_str(&message)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn alpha_maps_known_discriminants() {
        assert_eq!(u32_to_alpha(0).unwrap(), AlphaPolicy::Ignore);
        assert_eq!(u32_to_alpha(1).unwrap(), AlphaPolicy::Weighted);
    }

    #[test]
    fn alpha_rejects_unknown() {
        assert!(u32_to_alpha(2).is_err());
    }

    #[test]
    fn params_round_trip_from_json() {
        let json = r#"{
            "cell_width": 16,
            "cell_height": 16,
            "grid": 3,
            "alpha": "Ignore",
            "ensure_all_tiles": true,
            "max_tile_usage": null,
            "avoid_adjacent_duplicates": false,
            "color_adjust": 0.0,
            "output_scale": 1
        }"#;
        let p = parse_params(json).unwrap();
        assert_eq!(p.cell_width, 16);
        assert_eq!(p.grid, 3);
        assert_eq!(p.alpha, AlphaPolicy::Ignore);
        assert!(p.ensure_all_tiles);
        assert_eq!(p.max_tile_usage, None);
        assert_eq!(p.output_scale, 1);
    }

    #[test]
    fn params_alpha_weighted_and_max_use() {
        let json = r#"{
            "cell_width": 8,
            "cell_height": 12,
            "grid": 2,
            "alpha": "Weighted",
            "ensure_all_tiles": false,
            "max_tile_usage": 5,
            "avoid_adjacent_duplicates": true,
            "color_adjust": 0.5,
            "output_scale": 2
        }"#;
        let p = parse_params(json).unwrap();
        assert_eq!(p.alpha, AlphaPolicy::Weighted);
        assert_eq!(p.max_tile_usage, Some(5));
        assert_eq!(p.color_adjust, 0.5);
    }

    #[test]
    fn params_rejects_garbage() {
        assert!(parse_params("not json").is_err());
    }
}
