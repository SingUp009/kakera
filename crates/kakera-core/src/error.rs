use thiserror::Error;

#[derive(Debug, Error)]
#[non_exhaustive]
pub enum KakeraError {
    #[error("buffer length {actual} does not match {width}x{height}x4 = {expected}")]
    BufferSizeMismatch {
        width: u32,
        height: u32,
        expected: usize,
        actual: usize,
    },

    #[error("zero dimension: width={width}, height={height}")]
    ZeroDimension { width: u32, height: u32 },

    #[error("invalid grid size {grid}: must be >= 1")]
    InvalidGrid { grid: u32 },

    #[error(
        "invalid cell size: cell_width={cell_width}, cell_height={cell_height} (both must be >= 1)"
    )]
    InvalidCellSize { cell_width: u32, cell_height: u32 },

    #[error(
        "target image ({width}x{height}) is smaller than one cell ({cell_width}x{cell_height})"
    )]
    TargetTooSmall {
        width: u32,
        height: u32,
        cell_width: u32,
        cell_height: u32,
    },

    #[error("params grid {params} != index grid {index}")]
    GridMismatch { params: u32, index: u32 },

    #[error("tile index is empty; nothing to match against")]
    EmptyIndex,

    #[error("tile provider has no pixels for tile id {0}")]
    MissingTile(u32),

    #[error("tile provider failed for tile id {id}: {message}")]
    TileProvider { id: u32, message: String },

    #[error("max_tile_usage must be >= 1, got {max}")]
    InvalidMaxTileUsage { max: u32 },

    #[error("max_tile_usage {max} too small: {tiles} tiles cannot fill {cells} cells")]
    MaxTileUsageTooSmall {
        max: u32,
        tiles: usize,
        cells: usize,
    },

    #[error("color_adjust must be a finite value in 0.0..=1.0, got {value}")]
    InvalidColorAdjust { value: f32 },

    #[error("output_scale must be >= 1, got {scale}")]
    InvalidOutputScale { scale: u32 },

    #[error("image {width}x{height} exceeds supported size")]
    ImageTooLarge { width: u128, height: u128 },
}

pub type Result<T> = core::result::Result<T, KakeraError>;
