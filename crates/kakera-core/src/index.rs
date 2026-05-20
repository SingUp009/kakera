use crate::color::AlphaPolicy;
use crate::error::{KakeraError, Result};
use crate::feature::TileFeature;
use crate::image::{RgbaImage, RgbaView};
use crate::parallel::map_collect;

/// Adapter-assigned tile identity. Core never interprets it.
pub type TileId = u32;

#[derive(Clone, Debug, PartialEq)]
#[cfg_attr(feature = "serde", derive(serde::Serialize, serde::Deserialize))]
pub struct IndexedTile {
    pub id: TileId,
    pub feature: TileFeature,
}

/// Reusable color-feature index over a set of source tiles.
#[derive(Clone, Debug, Default, PartialEq)]
#[cfg_attr(feature = "serde", derive(serde::Serialize, serde::Deserialize))]
pub struct TileIndex {
    pub grid: u32,
    pub alpha: AlphaPolicy,
    pub tiles: Vec<IndexedTile>,
}

impl TileIndex {
    pub fn is_empty(&self) -> bool {
        self.tiles.is_empty()
    }

    pub fn len(&self) -> usize {
        self.tiles.len()
    }
}

/// Supplies decoded RGBA pixels for a tile id on demand. Implemented by
/// adapters (cli/wasm/tauri); core stays I/O-free and only asks for the
/// tiles it actually places.
pub trait TileProvider {
    fn tile(&self, id: TileId) -> Result<RgbaImage>;
}

/// Extract one tile's feature. Lets callers decode + drop sources one by one.
pub fn gather_one(
    id: TileId,
    view: &RgbaView<'_>,
    grid: u32,
    alpha: AlphaPolicy,
) -> Result<IndexedTile> {
    let feature = TileFeature::extract(view, grid, alpha)?;
    Ok(IndexedTile { id, feature })
}

/// Build an index from already-held source views.
pub fn gather(
    sources: &[(TileId, RgbaView<'_>)],
    grid: u32,
    alpha: AlphaPolicy,
) -> Result<TileIndex> {
    let tiles = map_collect(sources, |(id, view)| gather_one(*id, view, grid, alpha))
        .into_iter()
        .collect::<Result<Vec<_>>>()?;
    Ok(TileIndex {
        grid,
        alpha,
        tiles,
    })
}

/// Nearest tile id by squared feature distance. `Err(EmptyIndex)` if none.
pub fn nearest(index: &TileIndex, query: &TileFeature) -> Result<TileId> {
    nearest_with_dist(index, query).map(|(id, _)| id)
}

/// Nearest tile id together with its squared feature distance.
pub(crate) fn nearest_with_dist(
    index: &TileIndex,
    query: &TileFeature,
) -> Result<(TileId, f32)> {
    let mut best: Option<(TileId, f32)> = None;
    for t in &index.tiles {
        let d = query.distance_sq(&t.feature);
        match best {
            Some((_, bd)) if d >= bd => {}
            _ => best = Some((t.id, d)),
        }
    }
    best.ok_or(KakeraError::EmptyIndex)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::color::Rgb;

    fn solid(w: u32, h: u32, rgba: [u8; 4]) -> RgbaImage {
        let data: Vec<u8> = std::iter::repeat(rgba)
            .take((w * h) as usize)
            .flatten()
            .collect();
        RgbaImage::new(w, h, data).unwrap()
    }

    fn feat(rgb: [f32; 3]) -> TileFeature {
        TileFeature {
            grid: 1,
            cells: vec![Rgb {
                r: rgb[0],
                g: rgb[1],
                b: rgb[2],
            }],
        }
    }

    #[test]
    fn gather_builds_index() {
        let red = solid(4, 4, [255, 0, 0, 255]);
        let blue = solid(4, 4, [0, 0, 255, 255]);
        let sources = vec![(0u32, red.view()), (1u32, blue.view())];
        let index = gather(&sources, 1, AlphaPolicy::Ignore).unwrap();
        assert_eq!(index.len(), 2);
        assert_eq!(index.grid, 1);
        assert_eq!(index.tiles[0].id, 0);
    }

    #[test]
    fn gather_empty_is_ok_empty() {
        let index = gather(&[], 3, AlphaPolicy::Ignore).unwrap();
        assert!(index.is_empty());
        assert_eq!(index.grid, 3);
    }

    #[test]
    fn nearest_picks_closest_color() {
        let index = TileIndex {
            grid: 1,
            alpha: AlphaPolicy::Ignore,
            tiles: vec![
                IndexedTile {
                    id: 10,
                    feature: feat([255.0, 0.0, 0.0]),
                },
                IndexedTile {
                    id: 20,
                    feature: feat([0.0, 255.0, 0.0]),
                },
                IndexedTile {
                    id: 30,
                    feature: feat([0.0, 0.0, 255.0]),
                },
            ],
        };
        assert_eq!(nearest(&index, &feat([250.0, 5.0, 5.0])).unwrap(), 10);
        assert_eq!(nearest(&index, &feat([5.0, 250.0, 5.0])).unwrap(), 20);
        assert_eq!(nearest(&index, &feat([5.0, 5.0, 250.0])).unwrap(), 30);
    }

    #[test]
    fn nearest_prefers_strictly_closer() {
        let index = TileIndex {
            grid: 1,
            alpha: AlphaPolicy::Ignore,
            tiles: vec![
                IndexedTile {
                    id: 1,
                    feature: feat([255.0, 0.0, 0.0]),
                },
                IndexedTile {
                    id: 2,
                    feature: feat([240.0, 10.0, 10.0]),
                },
            ],
        };
        // Query nearer to id 2 than id 1.
        assert_eq!(nearest(&index, &feat([238.0, 12.0, 9.0])).unwrap(), 2);
    }

    #[test]
    fn nearest_empty_index_errors() {
        let index = TileIndex::default();
        let err = nearest(&index, &feat([0.0, 0.0, 0.0])).unwrap_err();
        assert!(matches!(err, KakeraError::EmptyIndex));
    }
}
