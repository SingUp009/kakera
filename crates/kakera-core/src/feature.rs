use crate::color::{average_grid, AlphaPolicy, Rgb};
use crate::error::{KakeraError, Result};
use crate::image::RgbaView;

/// N x N sub-grid of average colors describing a tile or a target cell.
#[derive(Clone, Debug, PartialEq, Default)]
#[cfg_attr(feature = "serde", derive(serde::Serialize, serde::Deserialize))]
pub struct TileFeature {
    pub grid: u32,
    /// Row-major, length == `grid * grid`.
    pub cells: Vec<Rgb>,
}

impl TileFeature {
    /// Reduce the whole `view` to a `grid x grid` color descriptor.
    pub fn extract(view: &RgbaView<'_>, grid: u32, alpha: AlphaPolicy) -> Result<TileFeature> {
        if grid < 1 {
            return Err(KakeraError::InvalidGrid { grid });
        }
        let cells = average_grid(
            view,
            0,
            0,
            view.width(),
            view.height(),
            grid,
            grid,
            alpha,
        );
        Ok(TileFeature { grid, cells })
    }

    /// Sum of per-cell squared RGB distance. `INFINITY` if grids differ.
    pub fn distance_sq(&self, other: &TileFeature) -> f32 {
        if self.grid != other.grid || self.cells.len() != other.cells.len() {
            return f32::INFINITY;
        }
        self.cells
            .iter()
            .zip(&other.cells)
            .map(|(a, b)| a.distance_sq(b))
            .sum()
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::image::RgbaImage;

    fn solid(w: u32, h: u32, rgba: [u8; 4]) -> RgbaImage {
        let data: Vec<u8> = std::iter::repeat(rgba)
            .take((w * h) as usize)
            .flatten()
            .collect();
        RgbaImage::new(w, h, data).unwrap()
    }

    #[test]
    fn extract_solid_tile_uniform_cells() {
        let img = solid(12, 12, [10, 20, 30, 255]);
        let f = TileFeature::extract(&img.view(), 3, AlphaPolicy::Ignore).unwrap();
        assert_eq!(f.grid, 3);
        assert_eq!(f.cells.len(), 9);
        for c in &f.cells {
            assert_eq!(*c, Rgb { r: 10.0, g: 20.0, b: 30.0 });
        }
    }

    #[test]
    fn extract_rejects_zero_grid() {
        let img = solid(4, 4, [0, 0, 0, 255]);
        let err = TileFeature::extract(&img.view(), 0, AlphaPolicy::Ignore).unwrap_err();
        assert!(matches!(err, KakeraError::InvalidGrid { grid: 0 }));
    }

    #[test]
    fn distance_sq_identical_is_zero_and_symmetric() {
        let img = solid(8, 8, [200, 100, 50, 255]);
        let a = TileFeature::extract(&img.view(), 2, AlphaPolicy::Ignore).unwrap();
        let b = a.clone();
        assert_eq!(a.distance_sq(&b), 0.0);
        assert_eq!(a.distance_sq(&b), b.distance_sq(&a));
    }

    #[test]
    fn distance_sq_known_value() {
        let a = TileFeature {
            grid: 1,
            cells: vec![Rgb { r: 0.0, g: 0.0, b: 0.0 }],
        };
        let b = TileFeature {
            grid: 1,
            cells: vec![Rgb { r: 3.0, g: 4.0, b: 0.0 }],
        };
        assert_eq!(a.distance_sq(&b), 25.0);
    }

    #[test]
    fn distance_sq_grid_mismatch_is_infinity() {
        let a = TileFeature {
            grid: 1,
            cells: vec![Rgb::default()],
        };
        let b = TileFeature {
            grid: 2,
            cells: vec![Rgb::default(); 4],
        };
        assert_eq!(a.distance_sq(&b), f32::INFINITY);
    }
}
