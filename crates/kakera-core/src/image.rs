use crate::error::{KakeraError, Result};

/// Owned, tightly-packed RGBA8 image (no row padding).
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct RgbaImage {
    width: u32,
    height: u32,
    data: Vec<u8>,
}

impl RgbaImage {
    /// `data` must be exactly `width * height * 4` bytes, row-major RGBA8.
    pub fn new(width: u32, height: u32, data: Vec<u8>) -> Result<Self> {
        let expected = expected_len(width, height)?;
        if data.len() != expected {
            return Err(KakeraError::BufferSizeMismatch {
                width,
                height,
                expected,
                actual: data.len(),
            });
        }
        Ok(Self {
            width,
            height,
            data,
        })
    }

    /// Fully transparent canvas (all zero bytes).
    pub fn zeroed(width: u32, height: u32) -> Result<Self> {
        let expected = expected_len(width, height)?;
        let mut data = Vec::<u8>::new();
        data.try_reserve_exact(expected)
            .map_err(|_| image_too_large(width as u128, height as u128))?;
        data.resize(expected, 0);
        Ok(Self {
            width,
            height,
            data,
        })
    }

    pub fn width(&self) -> u32 {
        self.width
    }

    pub fn height(&self) -> u32 {
        self.height
    }

    pub fn as_bytes(&self) -> &[u8] {
        &self.data
    }

    pub fn into_bytes(self) -> Vec<u8> {
        self.data
    }

    pub fn view(&self) -> RgbaView<'_> {
        RgbaView {
            width: self.width,
            height: self.height,
            data: &self.data,
        }
    }

    pub(crate) fn as_mut_slice(&mut self) -> &mut [u8] {
        &mut self.data
    }
}

/// Borrowed view over a tightly-packed RGBA8 buffer.
#[derive(Clone, Copy, Debug)]
pub struct RgbaView<'a> {
    width: u32,
    height: u32,
    data: &'a [u8],
}

impl<'a> RgbaView<'a> {
    pub fn new(width: u32, height: u32, data: &'a [u8]) -> Result<Self> {
        let expected = expected_len(width, height)?;
        if data.len() != expected {
            return Err(KakeraError::BufferSizeMismatch {
                width,
                height,
                expected,
                actual: data.len(),
            });
        }
        Ok(Self {
            width,
            height,
            data,
        })
    }

    pub fn width(&self) -> u32 {
        self.width
    }

    pub fn height(&self) -> u32 {
        self.height
    }

    /// RGBA at (x, y). Caller must keep x < width, y < height.
    pub fn pixel(&self, x: u32, y: u32) -> (u8, u8, u8, u8) {
        debug_assert!(x < self.width && y < self.height);
        let idx = ((y as usize * self.width as usize) + x as usize) * 4;
        (
            self.data[idx],
            self.data[idx + 1],
            self.data[idx + 2],
            self.data[idx + 3],
        )
    }
}

fn expected_len(width: u32, height: u32) -> Result<usize> {
    if width == 0 || height == 0 {
        return Err(KakeraError::ZeroDimension { width, height });
    }
    let bytes = rgba_byte_len(width as u128, height as u128)
        .ok_or_else(|| image_too_large(width as u128, height as u128))?;
    usize::try_from(bytes).map_err(|_| image_too_large(width as u128, height as u128))
}

pub(crate) fn rgba_byte_len(width: u128, height: u128) -> Option<u128> {
    width.checked_mul(height)?.checked_mul(4)
}

pub(crate) fn image_too_large(width: u128, height: u128) -> KakeraError {
    KakeraError::ImageTooLarge { width, height }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn new_rejects_wrong_length() {
        let err = RgbaImage::new(2, 2, vec![0u8; 10]).unwrap_err();
        match err {
            KakeraError::BufferSizeMismatch {
                expected, actual, ..
            } => {
                assert_eq!(expected, 16);
                assert_eq!(actual, 10);
            }
            other => panic!("unexpected: {other:?}"),
        }
    }

    #[test]
    fn new_rejects_zero_dimension() {
        let err = RgbaImage::new(0, 4, vec![]).unwrap_err();
        assert!(matches!(err, KakeraError::ZeroDimension { .. }));
    }

    #[test]
    fn into_bytes_round_trip() {
        let bytes: Vec<u8> = (0..16).collect();
        let img = RgbaImage::new(2, 2, bytes.clone()).unwrap();
        assert_eq!(img.into_bytes(), bytes);
    }

    #[test]
    fn pixel_reads_correct_rgba() {
        // 2x2: pixel (1,1) is bytes 12..16
        let bytes: Vec<u8> = (0..16).collect();
        let img = RgbaImage::new(2, 2, bytes).unwrap();
        let v = img.view();
        assert_eq!(v.pixel(0, 0), (0, 1, 2, 3));
        assert_eq!(v.pixel(1, 1), (12, 13, 14, 15));
    }

    #[test]
    fn zeroed_is_transparent() {
        let img = RgbaImage::zeroed(3, 2).unwrap();
        assert_eq!(img.as_bytes().len(), 3 * 2 * 4);
        assert!(img.as_bytes().iter().all(|&b| b == 0));
    }

    #[test]
    fn new_rejects_dimensions_whose_len_exceeds_usize() {
        let err = RgbaImage::new(u32::MAX, u32::MAX, Vec::new()).unwrap_err();
        assert!(matches!(
            err,
            KakeraError::ImageTooLarge {
                width: 4_294_967_295,
                height: 4_294_967_295,
                ..
            }
        ));
    }
}
