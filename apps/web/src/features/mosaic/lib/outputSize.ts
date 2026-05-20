import type { ParamsDTO } from "@/features/mosaic/types";

export interface OutputSize {
  cols: number;
  rows: number;
  width: number;
  height: number;
  /** RGBA8 byte size of the output buffer. */
  bytes: number;
}

export interface OutputSizeLimitIssue {
  message: string;
  webAction: string;
  desktopSuggestion: string;
}

/** Browsers reject canvases beyond ~these limits; keep a conservative cap. */
export const MAX_CANVAS_EDGE = 16384;
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

/** Human-facing details when the output is too large to render, or null if OK. */
export function outputSizeLimitIssue(size: OutputSize): OutputSizeLimitIssue | null {
  if (size.width > MAX_CANVAS_EDGE || size.height > MAX_CANVAS_EDGE) {
    return {
      message: `出力サイズ ${size.width}×${size.height}px がブラウザの上限 (${MAX_CANVAS_EDGE}px) を超えています。`,
      webAction: "Web版ではセルサイズか拡大率を下げると生成できます。",
      desktopSuggestion: "大きいまま作りたい場合は、インストール版を使う選択肢もあります。",
    };
  }
  if (size.bytes > MAX_OUTPUT_BYTES) {
    const mb = Math.round(size.bytes / (1024 * 1024));
    return {
      message: `出力バッファが約 ${mb}MB と大きすぎます。`,
      webAction: "Web版ではセルサイズか拡大率を下げると生成できます。",
      desktopSuggestion: "大きいまま作りたい場合は、インストール版を使う選択肢もあります。",
    };
  }
  return null;
}

/** Human-facing reason the output is too large to render, or null if OK. */
export function outputSizeError(size: OutputSize): string | null {
  return outputSizeLimitIssue(size)?.message ?? null;
}
