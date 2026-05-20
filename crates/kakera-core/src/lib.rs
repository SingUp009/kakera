//! kakera-core: I/O-free photomosaic engine.
//!
//! Pipeline: [`gather`] builds a reusable [`TileIndex`] of color features from
//! decoded source images; [`build`] assembles the mosaic by matching each
//! target cell to the nearest tile and painting its pixels; [`preview`] is a
//! cheap variant that flat-fills cells with the matched tile's average color.
//!
//! Interface is "RGBA8 buffer in -> RGBA8 buffer out". No filesystem, network,
//! or image decoding lives here — adapters (cli/wasm/tauri) own that and feed
//! tightly-packed RGBA8 buffers in, supplying tile pixels via [`TileProvider`].

mod color;
mod error;
mod feature;
mod image;
mod index;
mod mosaic;
mod parallel;

pub use color::{AlphaPolicy, Rgb};
pub use error::{KakeraError, Result};
pub use feature::TileFeature;
pub use image::{RgbaImage, RgbaView};
pub use index::{gather, gather_one, nearest, IndexedTile, TileId, TileIndex, TileProvider};
pub use mosaic::{build, preview, MosaicGrid, MosaicParams};

#[cfg(all(test, feature = "serde"))]
mod serde_tests {
    use super::*;

    #[test]
    fn tile_index_round_trip() {
        let index = TileIndex {
            grid: 2,
            alpha: AlphaPolicy::Weighted,
            tiles: vec![IndexedTile {
                id: 7,
                feature: TileFeature {
                    grid: 2,
                    cells: vec![
                        Rgb {
                            r: 1.0,
                            g: 2.0,
                            b: 3.0,
                        },
                        Rgb {
                            r: 4.0,
                            g: 5.0,
                            b: 6.0,
                        },
                        Rgb {
                            r: 7.0,
                            g: 8.0,
                            b: 9.0,
                        },
                        Rgb {
                            r: 10.0,
                            g: 11.0,
                            b: 12.0,
                        },
                    ],
                },
            }],
        };
        let json = serde_json::to_string(&index).unwrap();
        let back: TileIndex = serde_json::from_str(&json).unwrap();
        assert_eq!(index, back);
    }

    #[test]
    fn mosaic_params_round_trip() {
        let p = MosaicParams {
            cell_width: 24,
            cell_height: 12,
            grid: 4,
            alpha: AlphaPolicy::Ignore,
            ensure_all_tiles: true,
            max_tile_usage: Some(5),
            avoid_adjacent_duplicates: true,
            color_adjust: 0.5,
            output_scale: 4,
        };
        let json = serde_json::to_string(&p).unwrap();
        let back: MosaicParams = serde_json::from_str(&json).unwrap();
        assert_eq!(p, back);
    }
}
