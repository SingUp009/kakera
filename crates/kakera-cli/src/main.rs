use anyhow::{anyhow, bail, Context, Result};
use clap::{Args, Parser, Subcommand, ValueEnum};
use kakera_core::{
    build, gather_one, preview, AlphaPolicy, KakeraError, MosaicParams, RgbaImage, TileId,
    TileIndex, TileProvider,
};
use std::collections::BTreeMap;
use std::path::{Path, PathBuf};

const IMAGE_EXTS: &[&str] = &["png", "jpg", "jpeg", "bmp", "gif", "webp", "tiff", "tif"];

#[derive(Parser)]
#[command(name = "kakera-cli", about = "photomosaic generator", version)]
struct Cli {
    #[command(subcommand)]
    cmd: Cmd,
}

#[derive(Subcommand)]
enum Cmd {
    /// Build a reusable color-feature index from a folder of images.
    Gather(GatherArgs),
    /// Assemble the mosaic, painting each cell with source tile pixels.
    Build(BuildArgs),
    /// Cheap preview: cells flat-filled with the matched tile's avg color.
    Preview(PreviewArgs),
}

#[derive(Clone, Copy, ValueEnum)]
enum AlphaArg {
    /// Alpha excluded from color averaging (default).
    Ignore,
    /// RGB weighted by alpha / 255.
    Weighted,
}

impl From<AlphaArg> for AlphaPolicy {
    fn from(a: AlphaArg) -> Self {
        match a {
            AlphaArg::Ignore => AlphaPolicy::Ignore,
            AlphaArg::Weighted => AlphaPolicy::Weighted,
        }
    }
}

#[derive(Args)]
struct GatherArgs {
    /// Folder of source/tile images.
    tiles_dir: PathBuf,
    /// Output index JSON path (a `<path>.tiles.json` sidecar is also written).
    index_out: String,
    /// N for the N x N sub-grid color feature.
    #[arg(long, default_value_t = 3)]
    grid: u32,
    /// How alpha participates in color averaging.
    #[arg(long, value_enum, default_value_t = AlphaArg::Ignore)]
    alpha: AlphaArg,
}

/// Tunables shared by `build` and `preview`. Grid and alpha are read
/// back from the index so they always match `gather`.
#[derive(Args)]
struct MosaicOpts {
    /// Output cell width in pixels.
    #[arg(long, default_value_t = 16)]
    cell_width: u32,
    /// Output cell height in pixels.
    #[arg(long, default_value_t = 16)]
    cell_height: u32,
    /// Disable the default "place every tile at least once" pass
    /// (plain greedy nearest-match; some tiles may go unused).
    #[arg(long = "no-all", default_value_t = false)]
    no_all: bool,
    /// Cap how many times any single tile may be used.
    #[arg(long = "max-use", value_name = "N")]
    max_use: Option<u32>,
    /// Avoid the same tile in adjacent cells (best-effort).
    #[arg(long = "avoid-adjacent-dup", default_value_t = false)]
    avoid_adjacent_dup: bool,
    /// Multiply each tile by the target color. 0.0 = off (default),
    /// 1.0 = full multiply; values between blend toward white.
    #[arg(long = "color-adjust", value_name = "0.0..=1.0", default_value_t = 0.0)]
    color_adjust: f32,
    /// Integer upscale of the output (granularity unchanged; the
    /// mosaic is rendered larger than the source). 1 = no upscale.
    #[arg(long = "scale", value_name = "N", default_value_t = 1)]
    scale: u32,
}

impl MosaicOpts {
    fn into_params(self, index: &TileIndex) -> MosaicParams {
        MosaicParams {
            cell_width: self.cell_width,
            cell_height: self.cell_height,
            grid: index.grid,
            alpha: index.alpha,
            ensure_all_tiles: !self.no_all,
            max_tile_usage: self.max_use,
            avoid_adjacent_duplicates: self.avoid_adjacent_dup,
            color_adjust: self.color_adjust,
            output_scale: self.scale,
        }
    }
}

#[derive(Args)]
struct BuildArgs {
    /// Target image to recreate.
    target: PathBuf,
    /// Index JSON produced by `gather`.
    index: String,
    /// Folder containing the tile images referenced by the index.
    tiles_dir: PathBuf,
    /// Output mosaic image path.
    out: PathBuf,
    #[command(flatten)]
    opts: MosaicOpts,
}

#[derive(Args)]
struct PreviewArgs {
    /// Target image to recreate.
    target: PathBuf,
    /// Index JSON produced by `gather`.
    index: String,
    /// Output preview image path.
    out: PathBuf,
    #[command(flatten)]
    opts: MosaicOpts,
}

fn main() -> Result<()> {
    match Cli::parse().cmd {
        Cmd::Gather(a) => cmd_gather(a),
        Cmd::Build(a) => cmd_build(a),
        Cmd::Preview(a) => cmd_preview(a),
    }
}

/// Decode an image file to tightly-packed RGBA8.
fn load_rgba(path: &Path) -> Result<RgbaImage> {
    let img = image::open(path)
        .with_context(|| format!("decoding {}", path.display()))?
        .to_rgba8();
    let (w, h) = img.dimensions();
    RgbaImage::new(w, h, img.into_raw()).map_err(|e| anyhow!("{} -> {e}", path.display()))
}

fn save_rgba(out: RgbaImage, path: &Path) -> Result<()> {
    let buf = image::RgbaImage::from_raw(out.width(), out.height(), out.into_bytes())
        .ok_or_else(|| anyhow!("internal: output buffer size mismatch"))?;
    buf.save(path)
        .with_context(|| format!("writing {}", path.display()))
}

fn sidecar_path(index_path: &str) -> PathBuf {
    PathBuf::from(format!("{index_path}.tiles.json"))
}

/// Sorted (id, file_name) pairs for every image directly under `dir`.
fn list_tiles(dir: &Path) -> Result<Vec<(TileId, String)>> {
    let mut names: Vec<String> = std::fs::read_dir(dir)
        .with_context(|| format!("reading directory {}", dir.display()))?
        .filter_map(|e| e.ok())
        .filter(|e| e.path().is_file())
        .filter_map(|e| {
            let p = e.path();
            let ext = p.extension()?.to_str()?.to_ascii_lowercase();
            if IMAGE_EXTS.contains(&ext.as_str()) {
                e.file_name().into_string().ok()
            } else {
                None
            }
        })
        .collect();
    names.sort();
    Ok(names
        .into_iter()
        .enumerate()
        .map(|(i, n)| (i as TileId, n))
        .collect())
}

fn cmd_gather(a: GatherArgs) -> Result<()> {
    let alpha: AlphaPolicy = a.alpha.into();
    let listing = list_tiles(&a.tiles_dir)?;
    if listing.is_empty() {
        bail!("no images found in {}", a.tiles_dir.display());
    }

    let mut tiles = Vec::with_capacity(listing.len());
    let mut sidecar: BTreeMap<TileId, String> = BTreeMap::new();
    for (id, name) in &listing {
        let img = load_rgba(&a.tiles_dir.join(name))?;
        let indexed = gather_one(*id, &img.view(), a.grid, alpha)?;
        tiles.push(indexed);
        sidecar.insert(*id, name.clone());
        // `img` dropped here: only one decoded source held at a time.
    }

    let index = TileIndex {
        grid: a.grid,
        alpha,
        tiles,
    };
    std::fs::write(&a.index_out, serde_json::to_vec_pretty(&index)?)
        .with_context(|| format!("writing {}", a.index_out))?;
    std::fs::write(
        sidecar_path(&a.index_out),
        serde_json::to_vec_pretty(&sidecar)?,
    )
    .context("writing tile sidecar")?;

    println!(
        "gathered {} tiles (grid {}, alpha {:?}) -> {}",
        index.tiles.len(),
        a.grid,
        index.alpha,
        a.index_out
    );
    Ok(())
}

fn read_index(index_path: &str) -> Result<TileIndex> {
    let bytes = std::fs::read(index_path).with_context(|| format!("reading {index_path}"))?;
    serde_json::from_slice(&bytes).with_context(|| format!("parsing {index_path}"))
}

fn read_sidecar(index_path: &str) -> Result<BTreeMap<TileId, String>> {
    let p = sidecar_path(index_path);
    let bytes = std::fs::read(&p).with_context(|| format!("reading {}", p.display()))?;
    serde_json::from_slice(&bytes).with_context(|| format!("parsing {}", p.display()))
}

struct DirTileProvider {
    map: BTreeMap<TileId, PathBuf>,
}

impl TileProvider for DirTileProvider {
    fn tile(&self, id: TileId) -> kakera_core::Result<RgbaImage> {
        let path = self.map.get(&id).ok_or(KakeraError::MissingTile(id))?;
        let img = image::open(path)
            .map_err(|e| KakeraError::TileProvider {
                id,
                message: e.to_string(),
            })?
            .to_rgba8();
        let (w, h) = img.dimensions();
        RgbaImage::new(w, h, img.into_raw())
    }
}

fn cmd_build(a: BuildArgs) -> Result<()> {
    let index = read_index(&a.index)?;
    let sidecar = read_sidecar(&a.index)?;
    let provider = DirTileProvider {
        map: sidecar
            .into_iter()
            .map(|(id, name)| (id, a.tiles_dir.join(name)))
            .collect(),
    };

    let target = load_rgba(&a.target)?;
    let params = a.opts.into_params(&index);

    let out = build(&target.view(), &index, &provider, &params)?;
    save_rgba(out, &a.out)?;
    println!("built mosaic -> {}", a.out.display());
    Ok(())
}

fn cmd_preview(a: PreviewArgs) -> Result<()> {
    let index = read_index(&a.index)?;
    let target = load_rgba(&a.target)?;
    let params = a.opts.into_params(&index);

    let out = preview(&target.view(), &index, &params)?;
    save_rgba(out, &a.out)?;
    println!("built preview -> {}", a.out.display());
    Ok(())
}
