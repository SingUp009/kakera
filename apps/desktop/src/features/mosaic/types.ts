/** Shared types for the mosaic feature. */

/** Mirrors `kakera_core::AlphaPolicy`. Serialized as the variant string in
 *  the params JSON; passed as 0/1 to the Rust side. */
export type AlphaName = "Ignore" | "Weighted";

/** Mirrors `kakera_core::MosaicParams` (serde field names, verbatim). */
export interface ParamsDTO {
  grid: number;
  cell_width: number;
  cell_height: number;
  alpha: AlphaName;
  ensure_all_tiles: boolean;
  max_tile_usage?: number | null;
  avoid_adjacent_duplicates: boolean;
  color_adjust: number;
  output_scale: number;
}

/** CLI-mirrored defaults (kakera-cli `MosaicOpts` / `MosaicParams::default`). */
export const DEFAULT_PARAMS: ParamsDTO = {
  grid: 3,
  cell_width: 16,
  cell_height: 16,
  alpha: "Ignore",
  ensure_all_tiles: true,
  max_tile_usage: null,
  avoid_adjacent_duplicates: false,
  color_adjust: 0,
  output_scale: 1,
};

export type RenderKind = "build" | "preview";
export type ProgressPhase = "decode" | "gather" | "render";

export interface Progress {
  phase: ProgressPhase;
  done: number;
  total: number;
}

export interface MosaicResult {
  w: number;
  h: number;
  buf: ArrayBuffer;
}

export interface TilesLoaded {
  count: number;
  skipped: number;
}
