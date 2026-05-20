use crate::image::RgbaView;

/// Linear RGB triple in `0.0..=255.0`.
#[derive(Clone, Copy, Debug, PartialEq, Default)]
#[cfg_attr(feature = "serde", derive(serde::Serialize, serde::Deserialize))]
pub struct Rgb {
    pub r: f32,
    pub g: f32,
    pub b: f32,
}

impl Rgb {
    pub fn distance_sq(&self, other: &Rgb) -> f32 {
        let dr = self.r - other.r;
        let dg = self.g - other.g;
        let db = self.b - other.b;
        dr * dr + dg * dg + db * db
    }
}

/// How alpha participates in color averaging.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Default)]
#[cfg_attr(feature = "serde", derive(serde::Serialize, serde::Deserialize))]
pub enum AlphaPolicy {
    /// Alpha excluded; every pixel contributes equally.
    #[default]
    Ignore,
    /// RGB weighted by `a / 255`.
    Weighted,
}

/// Box / area-average downscale of a region into a `grid_w x grid_h`
/// row-major grid of average colors.
///
/// `region_*` must lie within `view`. A source pixel at local `(lx, ly)`
/// maps to bucket `col = lx * grid_w / region_w`, `row = ly * grid_h / region_h`.
#[allow(clippy::too_many_arguments)]
pub(crate) fn average_grid(
    view: &RgbaView<'_>,
    region_x: u32,
    region_y: u32,
    region_w: u32,
    region_h: u32,
    grid_w: u32,
    grid_h: u32,
    alpha: AlphaPolicy,
) -> Vec<Rgb> {
    debug_assert!(region_w >= 1 && region_h >= 1 && grid_w >= 1 && grid_h >= 1);
    debug_assert!(region_x + region_w <= view.width());
    debug_assert!(region_y + region_h <= view.height());

    let cells = (grid_w as usize) * (grid_h as usize);
    let mut sum_r = vec![0f64; cells];
    let mut sum_g = vec![0f64; cells];
    let mut sum_b = vec![0f64; cells];
    let mut weight = vec![0f64; cells];

    for ly in 0..region_h {
        let row = ((ly as u64 * grid_h as u64) / region_h as u64) as usize;
        for lx in 0..region_w {
            let col = ((lx as u64 * grid_w as u64) / region_w as u64) as usize;
            let bucket = row * grid_w as usize + col;
            let (r, g, b, a) = view.pixel(region_x + lx, region_y + ly);
            let w = match alpha {
                AlphaPolicy::Ignore => 1.0,
                AlphaPolicy::Weighted => a as f64 / 255.0,
            };
            sum_r[bucket] += r as f64 * w;
            sum_g[bucket] += g as f64 * w;
            sum_b[bucket] += b as f64 * w;
            weight[bucket] += w;
        }
    }

    (0..cells)
        .map(|i| {
            let w = weight[i];
            if w <= 0.0 {
                Rgb::default()
            } else {
                Rgb {
                    r: (sum_r[i] / w) as f32,
                    g: (sum_g[i] / w) as f32,
                    b: (sum_b[i] / w) as f32,
                }
            }
        })
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::image::RgbaImage;

    fn solid(w: u32, h: u32, rgba: [u8; 4]) -> RgbaImage {
        let data: Vec<u8> = std::iter::repeat_n(rgba, (w * h) as usize)
            .flatten()
            .collect();
        RgbaImage::new(w, h, data).unwrap()
    }

    #[test]
    fn distance_sq_basics() {
        let a = Rgb {
            r: 0.0,
            g: 0.0,
            b: 0.0,
        };
        let b = Rgb {
            r: 3.0,
            g: 4.0,
            b: 0.0,
        };
        assert_eq!(a.distance_sq(&a), 0.0);
        assert_eq!(a.distance_sq(&b), 25.0);
        assert_eq!(a.distance_sq(&b), b.distance_sq(&a));
    }

    #[test]
    fn solid_region_all_cells_same() {
        let img = solid(8, 8, [255, 0, 0, 255]);
        let cells = average_grid(&img.view(), 0, 0, 8, 8, 3, 3, AlphaPolicy::Ignore);
        assert_eq!(cells.len(), 9);
        for c in cells {
            assert_eq!(
                c,
                Rgb {
                    r: 255.0,
                    g: 0.0,
                    b: 0.0
                }
            );
        }
    }

    #[test]
    fn left_white_right_black_split_at_1x2() {
        // 2x1 image: left white, right black -> 1x2 grid keeps them separate.
        let data = vec![
            255, 255, 255, 255, // (0,0) white
            0, 0, 0, 255, // (1,0) black
        ];
        let img = RgbaImage::new(2, 1, data).unwrap();
        let cells = average_grid(&img.view(), 0, 0, 2, 1, 2, 1, AlphaPolicy::Ignore);
        assert_eq!(
            cells[0],
            Rgb {
                r: 255.0,
                g: 255.0,
                b: 255.0
            }
        );
        assert_eq!(
            cells[1],
            Rgb {
                r: 0.0,
                g: 0.0,
                b: 0.0
            }
        );
    }

    #[test]
    fn two_by_two_to_single_cell_is_mean() {
        let data = vec![
            0, 0, 0, 255, // 0
            100, 100, 100, 255, // 100
            100, 100, 100, 255, // 100
            200, 200, 200, 255, // 200
        ];
        let img = RgbaImage::new(2, 2, data).unwrap();
        let cells = average_grid(&img.view(), 0, 0, 2, 2, 1, 1, AlphaPolicy::Ignore);
        assert_eq!(
            cells[0],
            Rgb {
                r: 100.0,
                g: 100.0,
                b: 100.0
            }
        );
    }

    #[test]
    fn weighted_alpha_skips_transparent() {
        // Opaque red + fully transparent green -> weighted avg is pure red.
        let data = vec![
            255, 0, 0, 255, // opaque red
            0, 255, 0, 0, // transparent green
        ];
        let img = RgbaImage::new(2, 1, data).unwrap();
        let cells = average_grid(&img.view(), 0, 0, 2, 1, 1, 1, AlphaPolicy::Weighted);
        assert_eq!(
            cells[0],
            Rgb {
                r: 255.0,
                g: 0.0,
                b: 0.0
            }
        );
    }

    #[test]
    fn weighted_fully_transparent_is_zero() {
        let img = solid(4, 4, [123, 200, 50, 0]);
        let cells = average_grid(&img.view(), 0, 0, 4, 4, 1, 1, AlphaPolicy::Weighted);
        assert_eq!(cells[0], Rgb::default());
    }
}
