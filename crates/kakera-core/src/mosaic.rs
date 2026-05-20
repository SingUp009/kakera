use crate::color::{average_grid, AlphaPolicy, Rgb};
use crate::error::{KakeraError, Result};
use crate::feature::TileFeature;
use crate::image::{RgbaImage, RgbaView};
use crate::index::{nearest_with_dist, TileId, TileIndex, TileProvider};
use crate::parallel::map_collect;
use std::collections::HashMap;

#[derive(Clone, Debug, PartialEq)]
#[cfg_attr(feature = "serde", derive(serde::Serialize, serde::Deserialize))]
pub struct MosaicParams {
    pub cell_width: u32,
    pub cell_height: u32,
    /// N for the N x N sub-grid feature. Must equal the index's grid.
    pub grid: u32,
    pub alpha: AlphaPolicy,
    /// Guarantee every indexed tile is placed at least once: after the
    /// greedy nearest-match pass, each unused tile displaces the
    /// least-regret over-used cell. No-op when the grid has fewer cells
    /// than tiles (pigeonhole-infeasible).
    pub ensure_all_tiles: bool,
    /// Hard cap on how many times any single tile may be placed.
    /// `Some(0)` or a cap too small to fill the grid is an error.
    pub max_tile_usage: Option<u32>,
    /// Best-effort: avoid the same tile in horizontally/vertically
    /// adjacent cells.
    pub avoid_adjacent_duplicates: bool,
    /// Per-pixel multiply of the placed tile by the target color.
    /// `0.0` = off (tile unchanged, current behaviour); `1.0` = full
    /// multiply (`tile * target / 255`); values in between blend the
    /// multiplier toward white. Must be finite in `0.0..=1.0`.
    pub color_adjust: f32,
    /// Integer upscale of the rendered output. Tile-selection
    /// granularity is unchanged (still `cell_width/height` over the
    /// target); each cell is just painted at `cell * output_scale`
    /// pixels, so the result is larger than the source. `1` = no
    /// upscale (current behaviour). Must be >= 1.
    pub output_scale: u32,
}

impl Default for MosaicParams {
    fn default() -> Self {
        Self {
            cell_width: 16,
            cell_height: 16,
            grid: 3,
            alpha: AlphaPolicy::Ignore,
            ensure_all_tiles: false,
            max_tile_usage: None,
            avoid_adjacent_duplicates: false,
            color_adjust: 0.0,
            output_scale: 1,
        }
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct MosaicGrid {
    pub cols: u32,
    pub rows: u32,
    /// Target-sampling cell size (drives tile selection / matching).
    pub cell_width: u32,
    pub cell_height: u32,
    /// Output upscale factor (>= 1).
    pub scale: u32,
    pub out_width: u32,
    pub out_height: u32,
}

impl MosaicGrid {
    /// Rendered (upscaled) cell pixel size.
    fn out_cell_width(&self) -> u32 {
        self.cell_width * self.scale
    }
    fn out_cell_height(&self) -> u32 {
        self.cell_height * self.scale
    }
}

impl MosaicGrid {
    /// Ceil grid: output is >= target so every target pixel is covered.
    pub fn compute(target_w: u32, target_h: u32, params: &MosaicParams) -> Result<MosaicGrid> {
        if params.grid < 1 {
            return Err(KakeraError::InvalidGrid { grid: params.grid });
        }
        if params.cell_width < 1 || params.cell_height < 1 {
            return Err(KakeraError::InvalidCellSize {
                cell_width: params.cell_width,
                cell_height: params.cell_height,
            });
        }
        if target_w < params.cell_width || target_h < params.cell_height {
            return Err(KakeraError::TargetTooSmall {
                width: target_w,
                height: target_h,
                cell_width: params.cell_width,
                cell_height: params.cell_height,
            });
        }
        if params.output_scale < 1 {
            return Err(KakeraError::InvalidOutputScale {
                scale: params.output_scale,
            });
        }
        let cols = target_w.div_ceil(params.cell_width);
        let rows = target_h.div_ceil(params.cell_height);
        Ok(MosaicGrid {
            cols,
            rows,
            cell_width: params.cell_width,
            cell_height: params.cell_height,
            scale: params.output_scale,
            out_width: cols * params.cell_width * params.output_scale,
            out_height: rows * params.cell_height * params.output_scale,
        })
    }
}

/// Per-cell target region, clipped to the target image bounds.
fn cell_region(grid: &MosaicGrid, target_w: u32, target_h: u32, col: u32, row: u32) -> (u32, u32, u32, u32) {
    let rx = col * grid.cell_width;
    let ry = row * grid.cell_height;
    let rw = grid.cell_width.min(target_w - rx);
    let rh = grid.cell_height.min(target_h - ry);
    (rx, ry, rw, rh)
}

fn cell_feature(
    target: &RgbaView<'_>,
    grid: &MosaicGrid,
    n: u32,
    alpha: AlphaPolicy,
    col: u32,
    row: u32,
) -> TileFeature {
    let (rx, ry, rw, rh) = cell_region(grid, target.width(), target.height(), col, row);
    let cells = average_grid(target, rx, ry, rw, rh, n, n, alpha);
    TileFeature { grid: n, cells }
}

/// Row-major (col, row) coordinates for every cell.
fn cell_coords(grid: &MosaicGrid) -> Vec<(u32, u32)> {
    (0..grid.rows)
        .flat_map(|row| (0..grid.cols).map(move |col| (col, row)))
        .collect()
}

/// Extract every cell's feature in `coords` order (parallelizable).
fn cell_features(
    target: &RgbaView<'_>,
    grid: &MosaicGrid,
    params: &MosaicParams,
    coords: &[(u32, u32)],
) -> Vec<TileFeature> {
    map_collect(coords, |&(col, row)| {
        cell_feature(target, grid, params.grid, params.alpha, col, row)
    })
}

/// Lowest-distance tile satisfying `pred`, deterministic (index order;
/// ties keep the earlier tile, matching `nearest_with_dist`).
fn best_tile_where(
    index: &TileIndex,
    feat: &TileFeature,
    mut pred: impl FnMut(TileId) -> bool,
) -> Option<(TileId, f32)> {
    let mut best: Option<(TileId, f32)> = None;
    for t in &index.tiles {
        if !pred(t.id) {
            continue;
        }
        let d = feat.distance_sq(&t.feature);
        match best {
            Some((_, bd)) if d >= bd => {}
            _ => best = Some((t.id, d)),
        }
    }
    best
}

fn usage_counts(picks: &[TileId], cap_hint: usize) -> HashMap<TileId, u32> {
    let mut usage: HashMap<TileId, u32> = HashMap::with_capacity(cap_hint);
    for &p in picks {
        *usage.entry(p).or_insert(0) += 1;
    }
    usage
}

/// Greedy nearest per cell, then ordered constraint layers:
/// max-usage cap (hard, may error) -> coverage (hard when feasible) ->
/// adjacent-duplicate avoidance (best-effort). Order is significant.
fn assign_picks(
    index: &TileIndex,
    feats: &[TileFeature],
    cols: u32,
    params: &MosaicParams,
) -> Result<Vec<TileId>> {
    let cell_count = feats.len();
    let tile_count = index.tiles.len();

    let cap = match params.max_tile_usage {
        Some(0) => return Err(KakeraError::InvalidMaxTileUsage { max: 0 }),
        Some(n) => {
            if (n as u64) * (tile_count as u64) < cell_count as u64 {
                return Err(KakeraError::MaxTileUsageTooSmall {
                    max: n,
                    tiles: tile_count,
                    cells: cell_count,
                });
            }
            Some(n)
        }
        None => None,
    };

    let matched = map_collect(feats, |f| nearest_with_dist(index, f))
        .into_iter()
        .collect::<Result<Vec<(TileId, f32)>>>()?;
    let mut picks: Vec<TileId> = matched.iter().map(|&(id, _)| id).collect();
    let mut best_d: Vec<f32> = matched.iter().map(|&(_, d)| d).collect();

    if let Some(n) = cap {
        apply_max_usage(index, feats, &mut picks, &mut best_d, n);
    }
    if params.ensure_all_tiles {
        ensure_coverage(index, feats, &mut picks, &mut best_d);
    }
    if params.avoid_adjacent_duplicates {
        avoid_adjacent(
            index,
            feats,
            &mut picks,
            &mut best_d,
            cols,
            cap,
            params.ensure_all_tiles,
        );
    }
    Ok(picks)
}

/// Enforce `max_tile_usage`: for every over-cap tile, keep its `cap`
/// best-fitting cells and reassign the rest to the nearest tile that
/// still has spare capacity. Feasibility is guaranteed by the caller's
/// `cap * tiles >= cells` check.
fn apply_max_usage(
    index: &TileIndex,
    feats: &[TileFeature],
    picks: &mut [TileId],
    best_d: &mut [f32],
    cap: u32,
) {
    let mut usage = usage_counts(picks, index.tiles.len());

    for t in &index.tiles {
        if usage.get(&t.id).copied().unwrap_or(0) <= cap {
            continue;
        }
        // Cells currently on this tile, best fit first; the worst
        // (sorted tail) lose the tile.
        let mut cells: Vec<(usize, f32)> = picks
            .iter()
            .enumerate()
            .filter(|&(_, &p)| p == t.id)
            .map(|(c, _)| (c, best_d[c]))
            .collect();
        cells.sort_by(|a, b| {
            a.1.partial_cmp(&b.1)
                .unwrap_or(std::cmp::Ordering::Equal)
                .then(a.0.cmp(&b.0))
        });
        for &(c, _) in cells.iter().skip(cap as usize) {
            if let Some((nid, nd)) =
                best_tile_where(index, &feats[c], |id| {
                    usage.get(&id).copied().unwrap_or(0) < cap
                })
            {
                if let Some(u) = usage.get_mut(&t.id) {
                    *u -= 1;
                }
                *usage.entry(nid).or_insert(0) += 1;
                picks[c] = nid;
                best_d[c] = nd;
            }
        }
    }
}

/// Best-effort: change cells equal to an already-decided left/up
/// neighbor to a different nearby tile, without breaking the cap or
/// (under `ensure_all`) orphaning a tile.
fn avoid_adjacent(
    index: &TileIndex,
    feats: &[TileFeature],
    picks: &mut [TileId],
    best_d: &mut [f32],
    cols: u32,
    cap: Option<u32>,
    ensure_all: bool,
) {
    if index.tiles.len() < 2 || cols == 0 {
        return;
    }
    let cols = cols as usize;
    let n = picks.len();
    let mut usage = usage_counts(picks, index.tiles.len());

    for c in 0..n {
        let col = c % cols;
        let row = c / cols;
        let cur = picks[c];
        let left = if col > 0 { Some(picks[c - 1]) } else { None };
        let up = if row > 0 { Some(picks[c - cols]) } else { None };
        if left != Some(cur) && up != Some(cur) {
            continue;
        }
        if ensure_all && usage.get(&cur).copied().unwrap_or(0) <= 1 {
            continue; // would orphan a covered tile
        }
        let right = if col + 1 < cols {
            Some(picks[c + 1])
        } else {
            None
        };
        let down = if c + cols < n {
            Some(picks[c + cols])
        } else {
            None
        };
        let cand = best_tile_where(index, &feats[c], |id| {
            if id == cur
                || Some(id) == left
                || Some(id) == up
                || Some(id) == right
                || Some(id) == down
            {
                return false;
            }
            match cap {
                Some(m) => usage.get(&id).copied().unwrap_or(0) < m,
                None => true,
            }
        });
        if let Some((nid, nd)) = cand {
            if let Some(u) = usage.get_mut(&cur) {
                *u -= 1;
            }
            *usage.entry(nid).or_insert(0) += 1;
            picks[c] = nid;
            best_d[c] = nd;
        }
    }
}

/// For each unused tile, take over the non-locked cell where swapping in
/// this tile costs the least extra distance, displacing only a tile that
/// is currently used more than once (so no other tile becomes unused).
/// Feasible whenever cells >= tiles; otherwise a no-op (pigeonhole).
fn ensure_coverage(
    index: &TileIndex,
    feats: &[TileFeature],
    picks: &mut [TileId],
    best_d: &mut [f32],
) {
    let cell_count = feats.len();
    let tile_count = index.tiles.len();
    if tile_count < 2 || cell_count < tile_count {
        return;
    }

    let mut usage: HashMap<TileId, u32> = HashMap::with_capacity(tile_count);
    for &p in picks.iter() {
        *usage.entry(p).or_insert(0) += 1;
    }
    let mut locked = vec![false; cell_count];

    for t in &index.tiles {
        if usage.get(&t.id).copied().unwrap_or(0) > 0 {
            continue;
        }
        // Least-regret host cell among displaceable (over-used) cells.
        let mut best: Option<(usize, f32)> = None;
        for (c, f) in feats.iter().enumerate() {
            if locked[c] || usage.get(&picks[c]).copied().unwrap_or(0) <= 1 {
                continue;
            }
            let regret = f.distance_sq(&t.feature) - best_d[c];
            if best.map_or(true, |(_, br)| regret < br) {
                best = Some((c, regret));
            }
        }
        if let Some((c, _)) = best {
            let old = picks[c];
            if let Some(u) = usage.get_mut(&old) {
                *u -= 1;
            }
            *usage.entry(t.id).or_insert(0) += 1;
            picks[c] = t.id;
            best_d[c] = feats[c].distance_sq(&t.feature);
            locked[c] = true;
        }
    }
}

fn validate(target: &RgbaView<'_>, index: &TileIndex, params: &MosaicParams) -> Result<MosaicGrid> {
    if index.is_empty() {
        return Err(KakeraError::EmptyIndex);
    }
    if params.grid != index.grid {
        return Err(KakeraError::GridMismatch {
            params: params.grid,
            index: index.grid,
        });
    }
    if !params.color_adjust.is_finite() || !(0.0..=1.0).contains(&params.color_adjust) {
        return Err(KakeraError::InvalidColorAdjust {
            value: params.color_adjust,
        });
    }
    MosaicGrid::compute(target.width(), target.height(), params)
}

/// Per-channel multiply of `base` by `target`, with the multiplier
/// blended from white (no change) toward `target` by `strength`.
/// `result = base * ((1 - s) + s * target / 255)`.
fn multiply_toward(base: Rgb, target: Rgb, s: f32) -> Rgb {
    let f = |b: f32, t: f32| b * ((1.0 - s) + s * t / 255.0);
    Rgb {
        r: f(base.r, target.r),
        g: f(base.g, target.g),
        b: f(base.b, target.b),
    }
}

/// Multiply each painted RGB pixel of the cell at `(ox, oy)` by the
/// corresponding target pixel (nearest mapping from the cell's clipped
/// target region). Alpha is left untouched.
#[allow(clippy::too_many_arguments)]
fn color_adjust_cell(
    out: &mut RgbaImage,
    ox: u32,
    oy: u32,
    cw: u32,
    ch: u32,
    target: &RgbaView<'_>,
    rx: u32,
    ry: u32,
    rw: u32,
    rh: u32,
    strength: f32,
) {
    let ow = out.width();
    let buf = out.as_mut_slice();
    for y in 0..ch {
        let ty = (ry + ((y as u64 * rh as u64) / ch as u64) as u32).min(ry + rh - 1);
        for x in 0..cw {
            let tx = (rx + ((x as u64 * rw as u64) / cw as u64) as u32).min(rx + rw - 1);
            let (tr, tg, tb, _) = target.pixel(tx, ty);
            let idx = (((oy + y) as usize * ow as usize) + (ox + x) as usize) * 4;
            let base = Rgb {
                r: buf[idx] as f32,
                g: buf[idx + 1] as f32,
                b: buf[idx + 2] as f32,
            };
            let m = multiply_toward(
                base,
                Rgb {
                    r: tr as f32,
                    g: tg as f32,
                    b: tb as f32,
                },
                strength,
            );
            buf[idx] = clamp_u8(m.r);
            buf[idx + 1] = clamp_u8(m.g);
            buf[idx + 2] = clamp_u8(m.b);
        }
    }
}

/// Full mosaic: each cell painted with the matched source tile's pixels,
/// box-resampled to the cell size.
pub fn build(
    target: &RgbaView<'_>,
    index: &TileIndex,
    provider: &dyn TileProvider,
    params: &MosaicParams,
) -> Result<RgbaImage> {
    let grid = validate(target, index, params)?;
    let mut out = RgbaImage::zeroed(grid.out_width, grid.out_height)?;

    // Match every cell first (parallelizable), then fetch + paint serially
    // so the TileProvider need not be Sync.
    let coords = cell_coords(&grid);
    let feats = cell_features(target, &grid, params, &coords);
    let picks = assign_picks(index, &feats, grid.cols, params)?;

    let (ocw, och) = (grid.out_cell_width(), grid.out_cell_height());
    for (&(col, row), &id) in coords.iter().zip(&picks) {
        let tile = provider.tile(id)?;
        let (ox, oy) = (col * ocw, row * och);
        paint_cell(&mut out, ox, oy, ocw, och, &tile.view(), params.alpha);
        if params.color_adjust > 0.0 {
            let (rx, ry, rw, rh) =
                cell_region(&grid, target.width(), target.height(), col, row);
            color_adjust_cell(
                &mut out,
                ox,
                oy,
                ocw,
                och,
                target,
                rx,
                ry,
                rw,
                rh,
                params.color_adjust,
            );
        }
    }
    Ok(out)
}

/// Cheap preview: each cell flat-filled with the matched tile's average
/// color. No TileProvider needed (no source pixels read).
pub fn preview(
    target: &RgbaView<'_>,
    index: &TileIndex,
    params: &MosaicParams,
) -> Result<RgbaImage> {
    let grid = validate(target, index, params)?;
    let mut out = RgbaImage::zeroed(grid.out_width, grid.out_height)?;

    let coords = cell_coords(&grid);
    let feats = cell_features(target, &grid, params, &coords);
    let picks = assign_picks(index, &feats, grid.cols, params)?;

    let (ocw, och) = (grid.out_cell_width(), grid.out_cell_height());
    for (i, (&(col, row), &id)) in coords.iter().zip(&picks).enumerate() {
        let mut avg = mean_color(index_feature(index, id));
        if params.color_adjust > 0.0 {
            // Multiply by the cell's mean target color (preview has no
            // per-pixel tile), keeping preview ~ build at coarse scale.
            avg = multiply_toward(avg, mean_color(&feats[i]), params.color_adjust);
        }
        fill_cell(&mut out, col * ocw, row * och, ocw, och, avg);
    }
    Ok(out)
}

fn index_feature<'a>(index: &'a TileIndex, id: TileId) -> &'a TileFeature {
    // `id` came from `nearest(index, ..)` so it is always present.
    &index
        .tiles
        .iter()
        .find(|t| t.id == id)
        .expect("nearest returned an id not in the index")
        .feature
}

fn mean_color(feat: &TileFeature) -> Rgb {
    if feat.cells.is_empty() {
        return Rgb::default();
    }
    let n = feat.cells.len() as f32;
    let mut acc = Rgb::default();
    for c in &feat.cells {
        acc.r += c.r;
        acc.g += c.g;
        acc.b += c.b;
    }
    Rgb {
        r: acc.r / n,
        g: acc.g / n,
        b: acc.b / n,
    }
}

fn clamp_u8(v: f32) -> u8 {
    v.round().clamp(0.0, 255.0) as u8
}

fn fill_cell(out: &mut RgbaImage, ox: u32, oy: u32, cw: u32, ch: u32, color: Rgb) {
    let ow = out.width();
    let buf = out.as_mut_slice();
    let (r, g, b) = (clamp_u8(color.r), clamp_u8(color.g), clamp_u8(color.b));
    for y in 0..ch {
        for x in 0..cw {
            let idx = (((oy + y) as usize * ow as usize) + (ox + x) as usize) * 4;
            buf[idx] = r;
            buf[idx + 1] = g;
            buf[idx + 2] = b;
            buf[idx + 3] = 255;
        }
    }
}

/// Box-resample `src` into the `cw x ch` cell at `(ox, oy)`. Inverse mapping:
/// each output pixel averages the source pixels it covers (area-average on
/// downscale, nearest-neighbor replication on upscale — no gaps).
fn paint_cell(
    out: &mut RgbaImage,
    ox: u32,
    oy: u32,
    cw: u32,
    ch: u32,
    src: &RgbaView<'_>,
    alpha: AlphaPolicy,
) {
    let (sw, sh) = (src.width(), src.height());
    let ow = out.width();
    let buf = out.as_mut_slice();

    for y in 0..ch {
        let sy0 = (y as u64 * sh as u64 / ch as u64) as u32;
        let mut sy1 = ((y as u64 + 1) * sh as u64 / ch as u64) as u32;
        if sy1 <= sy0 {
            sy1 = sy0 + 1;
        }
        for x in 0..cw {
            let sx0 = (x as u64 * sw as u64 / cw as u64) as u32;
            let mut sx1 = ((x as u64 + 1) * sw as u64 / cw as u64) as u32;
            if sx1 <= sx0 {
                sx1 = sx0 + 1;
            }

            let mut acc_r = 0f64;
            let mut acc_g = 0f64;
            let mut acc_b = 0f64;
            let mut acc_a = 0f64;
            let mut weight = 0f64;
            let mut count = 0f64;
            for syy in sy0..sy1 {
                for sxx in sx0..sx1 {
                    let (r, g, b, a) = src.pixel(sxx, syy);
                    let w = match alpha {
                        AlphaPolicy::Ignore => 1.0,
                        AlphaPolicy::Weighted => a as f64 / 255.0,
                    };
                    acc_r += r as f64 * w;
                    acc_g += g as f64 * w;
                    acc_b += b as f64 * w;
                    acc_a += a as f64;
                    weight += w;
                    count += 1.0;
                }
            }

            let (r, g, b, a) = if weight <= 0.0 {
                (0, 0, 0, 0)
            } else {
                match alpha {
                    AlphaPolicy::Ignore => (
                        clamp_u8((acc_r / weight) as f32),
                        clamp_u8((acc_g / weight) as f32),
                        clamp_u8((acc_b / weight) as f32),
                        255,
                    ),
                    AlphaPolicy::Weighted => (
                        clamp_u8((acc_r / weight) as f32),
                        clamp_u8((acc_g / weight) as f32),
                        clamp_u8((acc_b / weight) as f32),
                        clamp_u8((acc_a / count) as f32),
                    ),
                }
            };

            let idx = (((oy + y) as usize * ow as usize) + (ox + x) as usize) * 4;
            buf[idx] = r;
            buf[idx + 1] = g;
            buf[idx + 2] = b;
            buf[idx + 3] = a;
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::image::RgbaImage;
    use crate::index::{IndexedTile, TileIndex};
    use std::collections::HashMap;

    fn solid(w: u32, h: u32, rgba: [u8; 4]) -> RgbaImage {
        let data: Vec<u8> = std::iter::repeat(rgba)
            .take((w * h) as usize)
            .flatten()
            .collect();
        RgbaImage::new(w, h, data).unwrap()
    }

    struct MockProvider(HashMap<TileId, RgbaImage>);
    impl TileProvider for MockProvider {
        fn tile(&self, id: TileId) -> Result<RgbaImage> {
            self.0
                .get(&id)
                .cloned()
                .ok_or(KakeraError::MissingTile(id))
        }
    }

    fn feat1(rgb: [f32; 3]) -> TileFeature {
        TileFeature {
            grid: 1,
            cells: vec![Rgb {
                r: rgb[0],
                g: rgb[1],
                b: rgb[2],
            }],
        }
    }

    fn four_color_index() -> TileIndex {
        TileIndex {
            grid: 1,
            alpha: AlphaPolicy::Ignore,
            tiles: vec![
                IndexedTile {
                    id: 0,
                    feature: feat1([255.0, 0.0, 0.0]),
                },
                IndexedTile {
                    id: 1,
                    feature: feat1([0.0, 255.0, 0.0]),
                },
                IndexedTile {
                    id: 2,
                    feature: feat1([0.0, 0.0, 255.0]),
                },
                IndexedTile {
                    id: 3,
                    feature: feat1([255.0, 255.0, 255.0]),
                },
            ],
        }
    }

    #[test]
    fn grid_compute_ceil_and_out_size() {
        let p = MosaicParams {
            cell_width: 16,
            cell_height: 16,
            grid: 1,
            ..MosaicParams::default()
        };
        let g = MosaicGrid::compute(100, 100, &p).unwrap();
        assert_eq!((g.cols, g.rows), (7, 7));
        assert_eq!((g.out_width, g.out_height), (112, 112));
    }

    #[test]
    fn grid_compute_rejects_bad_params() {
        let zero_grid = MosaicParams {
            grid: 0,
            ..MosaicParams::default()
        };
        assert!(matches!(
            MosaicGrid::compute(100, 100, &zero_grid).unwrap_err(),
            KakeraError::InvalidGrid { grid: 0 }
        ));

        let zero_cell = MosaicParams {
            cell_width: 0,
            ..MosaicParams::default()
        };
        assert!(matches!(
            MosaicGrid::compute(100, 100, &zero_cell).unwrap_err(),
            KakeraError::InvalidCellSize { .. }
        ));

        let p = MosaicParams::default();
        assert!(matches!(
            MosaicGrid::compute(8, 8, &p).unwrap_err(),
            KakeraError::TargetTooSmall { .. }
        ));
    }

    #[test]
    fn build_empty_index_errors() {
        let target = solid(4, 4, [10, 10, 10, 255]);
        let idx = TileIndex {
            grid: 1,
            ..TileIndex::default()
        };
        let prov = MockProvider(HashMap::new());
        let p = MosaicParams {
            cell_width: 2,
            cell_height: 2,
            grid: 1,
            ..MosaicParams::default()
        };
        assert!(matches!(
            build(&target.view(), &idx, &prov, &p).unwrap_err(),
            KakeraError::EmptyIndex
        ));
    }

    #[test]
    fn build_grid_mismatch_errors() {
        let target = solid(4, 4, [10, 10, 10, 255]);
        let idx = four_color_index(); // grid 1
        let prov = MockProvider(HashMap::new());
        let p = MosaicParams {
            cell_width: 2,
            cell_height: 2,
            grid: 3,
            ..MosaicParams::default()
        };
        assert!(matches!(
            build(&target.view(), &idx, &prov, &p).unwrap_err(),
            KakeraError::GridMismatch {
                params: 3,
                index: 1
            }
        ));
    }

    #[test]
    fn build_end_to_end_tiny_mosaic() {
        // 4x4 target, four 2x2 quadrants: red, green, blue, white.
        let mut data = vec![0u8; 4 * 4 * 4];
        let put = |d: &mut Vec<u8>, x: u32, y: u32, c: [u8; 4]| {
            let i = ((y * 4 + x) * 4) as usize;
            d[i..i + 4].copy_from_slice(&c);
        };
        for y in 0..4 {
            for x in 0..4 {
                let c = match (x / 2, y / 2) {
                    (0, 0) => [255, 0, 0, 255],
                    (1, 0) => [0, 255, 0, 255],
                    (0, 1) => [0, 0, 255, 255],
                    _ => [255, 255, 255, 255],
                };
                put(&mut data, x, y, c);
            }
        }
        let target = RgbaImage::new(4, 4, data).unwrap();

        let mut tiles = HashMap::new();
        tiles.insert(0, solid(8, 8, [255, 0, 0, 255]));
        tiles.insert(1, solid(8, 8, [0, 255, 0, 255]));
        tiles.insert(2, solid(8, 8, [0, 0, 255, 255]));
        tiles.insert(3, solid(8, 8, [255, 255, 255, 255]));
        let prov = MockProvider(tiles);

        let p = MosaicParams {
            cell_width: 2,
            cell_height: 2,
            grid: 1,
            ..MosaicParams::default()
        };
        let out = build(&target.view(), &four_color_index(), &prov, &p).unwrap();
        assert_eq!((out.width(), out.height()), (4, 4));
        let v = out.view();
        assert_eq!(v.pixel(0, 0), (255, 0, 0, 255)); // top-left red
        assert_eq!(v.pixel(3, 0), (0, 255, 0, 255)); // top-right green
        assert_eq!(v.pixel(0, 3), (0, 0, 255, 255)); // bottom-left blue
        assert_eq!(v.pixel(3, 3), (255, 255, 255, 255)); // bottom-right white
    }

    #[test]
    fn preview_fills_average_color() {
        let target = solid(4, 4, [250, 4, 4, 255]); // close to red tile id 0
        let p = MosaicParams {
            cell_width: 2,
            cell_height: 2,
            grid: 1,
            ..MosaicParams::default()
        };
        let out = preview(&target.view(), &four_color_index(), &p).unwrap();
        let v = out.view();
        assert_eq!(v.pixel(0, 0), (255, 0, 0, 255));
        assert_eq!(v.pixel(3, 3), (255, 0, 0, 255));
    }

    #[test]
    fn build_missing_tile_propagates() {
        let target = solid(4, 4, [255, 0, 0, 255]);
        let prov = MockProvider(HashMap::new()); // no tiles at all
        let p = MosaicParams {
            cell_width: 2,
            cell_height: 2,
            grid: 1,
            ..MosaicParams::default()
        };
        assert!(matches!(
            build(&target.view(), &four_color_index(), &prov, &p).unwrap_err(),
            KakeraError::MissingTile(_)
        ));
    }

    fn params_with(
        ensure_all: bool,
        max: Option<u32>,
        adjacent: bool,
    ) -> MosaicParams {
        MosaicParams {
            cell_width: 2,
            cell_height: 2,
            grid: 1,
            ensure_all_tiles: ensure_all,
            max_tile_usage: max,
            avoid_adjacent_duplicates: adjacent,
            ..MosaicParams::default()
        }
    }

    fn count(picks: &[TileId], id: TileId) -> usize {
        picks.iter().filter(|&&p| p == id).count()
    }

    #[test]
    fn assign_picks_greedy_leaves_tiles_unused() {
        let feats = vec![feat1([250.0, 5.0, 5.0]); 8]; // all closest to red (id 0)
        let picks =
            assign_picks(&four_color_index(), &feats, 8, &params_with(false, None, false))
                .unwrap();
        assert!(picks.iter().all(|&id| id == 0));
    }

    #[test]
    fn assign_picks_coverage_uses_every_tile() {
        let feats = vec![feat1([250.0, 5.0, 5.0]); 8];
        let picks =
            assign_picks(&four_color_index(), &feats, 8, &params_with(true, None, false))
                .unwrap();
        assert_eq!(picks.len(), 8);
        let used: std::collections::BTreeSet<_> = picks.iter().copied().collect();
        assert_eq!(used, [0, 1, 2, 3].into_iter().collect());
        // The dominant (best-matching) tile keeps the majority of cells.
        assert_eq!(count(&picks, 0), 5);
    }

    #[test]
    fn coverage_noop_when_more_tiles_than_cells() {
        let feats = vec![feat1([250.0, 5.0, 5.0]); 2]; // 2 cells, 4 tiles
        let picks =
            assign_picks(&four_color_index(), &feats, 2, &params_with(true, None, false))
                .unwrap();
        assert_eq!(picks, vec![0, 0]); // pigeonhole-infeasible -> greedy stands
    }

    #[test]
    fn max_usage_caps_occurrences() {
        let feats = vec![feat1([250.0, 5.0, 5.0]); 8]; // greedily all red
        let picks = assign_picks(
            &four_color_index(),
            &feats,
            8,
            &params_with(false, Some(2), false),
        )
        .unwrap();
        assert_eq!(picks.len(), 8);
        for id in 0..4 {
            assert!(count(&picks, id) <= 2, "tile {id} exceeds cap");
        }
        // 8 cells / cap 2 forces all four tiles into use.
        let used: std::collections::BTreeSet<_> = picks.iter().copied().collect();
        assert_eq!(used, [0, 1, 2, 3].into_iter().collect());
    }

    #[test]
    fn max_usage_zero_errors() {
        let feats = vec![feat1([1.0, 1.0, 1.0]); 4];
        let err = assign_picks(
            &four_color_index(),
            &feats,
            4,
            &params_with(false, Some(0), false),
        )
        .unwrap_err();
        assert!(matches!(err, KakeraError::InvalidMaxTileUsage { max: 0 }));
    }

    #[test]
    fn max_usage_too_small_errors() {
        let feats = vec![feat1([1.0, 1.0, 1.0]); 9]; // 4 tiles * cap 2 = 8 < 9
        let err = assign_picks(
            &four_color_index(),
            &feats,
            3,
            &params_with(false, Some(2), false),
        )
        .unwrap_err();
        assert!(matches!(
            err,
            KakeraError::MaxTileUsageTooSmall {
                max: 2,
                tiles: 4,
                cells: 9
            }
        ));
    }

    #[test]
    fn avoid_adjacent_breaks_neighbor_duplicates() {
        // 2x2 grid (cols=2), all cells greedily red -> adjacency must split.
        let feats = vec![feat1([250.0, 5.0, 5.0]); 4];
        let picks = assign_picks(
            &four_color_index(),
            &feats,
            2,
            &params_with(false, None, true),
        )
        .unwrap();
        let cols = 2usize;
        for c in 0..picks.len() {
            if c % cols > 0 {
                assert_ne!(picks[c], picks[c - 1], "horizontal dup at {c}");
            }
            if c / cols > 0 {
                assert_ne!(picks[c], picks[c - cols], "vertical dup at {c}");
            }
        }
    }

    #[test]
    fn all_constraints_together() {
        // 16 cells (cols=4), greedily all red; coverage + cap + adjacency.
        let feats = vec![feat1([250.0, 5.0, 5.0]); 16];
        let picks = assign_picks(
            &four_color_index(),
            &feats,
            4,
            &params_with(true, Some(6), true),
        )
        .unwrap();
        assert_eq!(picks.len(), 16);
        let used: std::collections::BTreeSet<_> = picks.iter().copied().collect();
        assert_eq!(used, [0, 1, 2, 3].into_iter().collect()); // coverage
        for id in 0..4 {
            assert!(count(&picks, id) <= 6, "tile {id} exceeds cap"); // hard cap
        }
    }

    #[test]
    fn build_with_ensure_all_places_every_tile() {
        let target = solid(8, 8, [250, 5, 5, 255]); // 4x4 = 16 cells, all near red
        let mut tiles = HashMap::new();
        tiles.insert(0, solid(4, 4, [255, 0, 0, 255]));
        tiles.insert(1, solid(4, 4, [0, 255, 0, 255]));
        tiles.insert(2, solid(4, 4, [0, 0, 255, 255]));
        tiles.insert(3, solid(4, 4, [255, 255, 255, 255]));
        let prov = MockProvider(tiles);

        let p = MosaicParams {
            cell_width: 2,
            cell_height: 2,
            grid: 1,
            ensure_all_tiles: true,
            ..MosaicParams::default()
        };
        let out = build(&target.view(), &four_color_index(), &prov, &p).unwrap();
        let v = out.view();
        let mut colors = std::collections::BTreeSet::new();
        for cy in 0..4 {
            for cx in 0..4 {
                let (r, g, b, _) = v.pixel(cx * 2, cy * 2);
                colors.insert((r, g, b));
            }
        }
        for c in [(255, 0, 0), (0, 255, 0), (0, 0, 255), (255, 255, 255)] {
            assert!(colors.contains(&c), "missing tile color {c:?}");
        }
    }

    fn white_index() -> TileIndex {
        TileIndex {
            grid: 1,
            alpha: AlphaPolicy::Ignore,
            tiles: vec![IndexedTile {
                id: 0,
                feature: feat1([255.0, 255.0, 255.0]),
            }],
        }
    }

    #[test]
    fn color_adjust_off_keeps_tile_unchanged() {
        let target = solid(4, 4, [128, 128, 128, 255]);
        let mut tiles = HashMap::new();
        tiles.insert(0, solid(4, 4, [255, 255, 255, 255]));
        let prov = MockProvider(tiles);
        let p = MosaicParams {
            cell_width: 2,
            cell_height: 2,
            grid: 1,
            color_adjust: 0.0,
            ..MosaicParams::default()
        };
        let out = build(&target.view(), &white_index(), &prov, &p).unwrap();
        assert_eq!(out.view().pixel(0, 0), (255, 255, 255, 255));
    }

    #[test]
    fn color_adjust_full_multiply_tints_to_target() {
        let target = solid(4, 4, [128, 128, 128, 255]);
        let mut tiles = HashMap::new();
        tiles.insert(0, solid(4, 4, [255, 255, 255, 255]));
        let prov = MockProvider(tiles);
        let p = MosaicParams {
            cell_width: 2,
            cell_height: 2,
            grid: 1,
            color_adjust: 1.0,
            ..MosaicParams::default()
        };
        let out = build(&target.view(), &white_index(), &prov, &p).unwrap();
        // white * (128/255) -> ~128, alpha preserved
        assert_eq!(out.view().pixel(0, 0), (128, 128, 128, 255));
    }

    #[test]
    fn color_adjust_preview_uses_cell_mean() {
        let target = solid(4, 4, [128, 128, 128, 255]);
        let p = MosaicParams {
            cell_width: 2,
            cell_height: 2,
            grid: 1,
            color_adjust: 1.0,
            ..MosaicParams::default()
        };
        let out = preview(&target.view(), &white_index(), &p).unwrap();
        assert_eq!(out.view().pixel(0, 0), (128, 128, 128, 255));
    }

    #[test]
    fn color_adjust_out_of_range_errors() {
        let target = solid(4, 4, [10, 10, 10, 255]);
        for bad in [1.5f32, -0.1, f32::NAN] {
            let p = MosaicParams {
                cell_width: 2,
                cell_height: 2,
                grid: 1,
                color_adjust: bad,
                ..MosaicParams::default()
            };
            assert!(matches!(
                preview(&target.view(), &white_index(), &p).unwrap_err(),
                KakeraError::InvalidColorAdjust { .. }
            ));
        }
    }

    #[test]
    fn output_scale_grid_dims() {
        let p = MosaicParams {
            cell_width: 16,
            cell_height: 16,
            grid: 1,
            output_scale: 3,
            ..MosaicParams::default()
        };
        let g = MosaicGrid::compute(100, 100, &p).unwrap();
        assert_eq!((g.cols, g.rows), (7, 7)); // sampling granularity unchanged
        assert_eq!((g.out_width, g.out_height), (7 * 16 * 3, 7 * 16 * 3));
    }

    #[test]
    fn output_scale_upscales_render() {
        let target = solid(8, 8, [10, 20, 30, 255]); // 4x4 cells @ cell 2
        let mut tiles = HashMap::new();
        tiles.insert(0, solid(4, 4, [200, 100, 50, 255]));
        let prov = MockProvider(tiles);
        let p = MosaicParams {
            cell_width: 2,
            cell_height: 2,
            grid: 1,
            output_scale: 4,
            ..MosaicParams::default()
        };
        let out =
            build(&target.view(), &white_index_solid(200.0, 100.0, 50.0), &prov, &p).unwrap();
        // 4 cols * cell 2 * scale 4 = 32
        assert_eq!((out.width(), out.height()), (32, 32));
        let v = out.view();
        assert_eq!(v.pixel(0, 0), (200, 100, 50, 255));
        assert_eq!(v.pixel(31, 31), (200, 100, 50, 255));
    }

    #[test]
    fn output_scale_preview_upscales() {
        let target = solid(8, 8, [10, 20, 30, 255]);
        let p = MosaicParams {
            cell_width: 2,
            cell_height: 2,
            grid: 1,
            output_scale: 4,
            ..MosaicParams::default()
        };
        let out = preview(&target.view(), &white_index_solid(7.0, 7.0, 7.0), &p).unwrap();
        assert_eq!((out.width(), out.height()), (32, 32));
    }

    #[test]
    fn output_scale_zero_errors() {
        let target = solid(4, 4, [10, 10, 10, 255]);
        let p = MosaicParams {
            cell_width: 2,
            cell_height: 2,
            grid: 1,
            output_scale: 0,
            ..MosaicParams::default()
        };
        assert!(matches!(
            preview(&target.view(), &white_index(), &p).unwrap_err(),
            KakeraError::InvalidOutputScale { scale: 0 }
        ));
    }

    fn white_index_solid(r: f32, g: f32, b: f32) -> TileIndex {
        TileIndex {
            grid: 1,
            alpha: AlphaPolicy::Ignore,
            tiles: vec![IndexedTile {
                id: 0,
                feature: feat1([r, g, b]),
            }],
        }
    }
}
