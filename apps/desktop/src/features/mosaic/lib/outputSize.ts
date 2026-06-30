import type { ParamsDTO } from "@/features/mosaic/types";

export interface OutputSize {
  cols: number;
  rows: number;
  width: number;
  height: number;
  /** RGBA8 byte size of the output buffer. */
  bytes: number;
}

/** Refuse outputs whose RGBA buffer would exceed this (~0.5 GiB). */
export const MAX_OUTPUT_BYTES = 512 * 1024 * 1024;

/**
 * Mirrors `kakera_core::MosaicGrid::compute`:
 *   cols = ceil(tw / cell_w), rows = ceil(th / cell_h)
 *   out  = cols*cell_w*scale  x  rows*cell_h*scale
 * Returns null when the target is smaller than one cell (core `TargetTooSmall`).
 */
export function computeOutputSize(
  targetW: number,
  targetH: number,
  params: Pick<ParamsDTO, "cell_width" | "cell_height" | "output_scale">,
): OutputSize | null {
  const { cell_width: cw, cell_height: ch, output_scale: scale } = params;
  if (cw < 1 || ch < 1 || scale < 1) return null;
  if (targetW < cw || targetH < ch) return null;

  const cols = Math.ceil(targetW / cw);
  const rows = Math.ceil(targetH / ch);
  const width = cols * cw * scale;
  const height = rows * ch * scale;
  return { cols, rows, width, height, bytes: width * height * 4 };
}

/** Human-facing reason the output is too large to render, or null if OK. */
export function outputSizeError(size: OutputSize): string | null {
  if (size.bytes > MAX_OUTPUT_BYTES) {
    const mb = Math.round(size.bytes / (1024 * 1024));
    return `出力バッファが約 ${mb}MB と大きすぎます。セルサイズか拡大率を下げてください。`;
  }
  return null;
}
