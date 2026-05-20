/**
 * Tauri command client for the mosaic engine. Mirrors what `apps/web` does
 * inside a Web Worker, but here the heavy work lives in Rust (`src-tauri`)
 * and large RGBA buffers cross the IPC boundary as raw bytes via Tauri 2's
 * `InvokeBody::Raw`.
 *
 * Wire format:
 *   - `mosaic_reset`: no payload.
 *   - `mosaic_add_tile`: raw RGBA8 body. Headers carry `x-tile-id` / `-w` / `-h`.
 *   - `mosaic_gather`: small JSON args (`grid`, `alpha`, `skipped`).
 *   - `mosaic_render`: raw RGBA8 target body. Headers carry `x-kind`,
 *     `x-target-w`, `x-target-h`, `x-params` (params as JSON string).
 *     Response body is `[width_u32_le, height_u32_le, rgba...]`.
 */
import { invoke } from "@tauri-apps/api/core";
import { decodeFull, decodeThumbnail } from "@/infrastructure/image/decode";
import type {
  AlphaName,
  MosaicResult,
  ParamsDTO,
  Progress,
  RenderKind,
  TilesLoaded,
} from "@/features/mosaic/types";

/** Longest tile edge kept after thumbnailing (bounds IPC payload size). */
const TILE_MAX_EDGE = 96;
/** How often to emit decode progress. */
const PROGRESS_EVERY = 16;

function alphaToU32(a: AlphaName): number {
  return a === "Weighted" ? 1 : 0;
}

/** View the Uint8Array's backing storage as a standalone ArrayBuffer when possible. */
function toArrayBuffer(view: Uint8Array): ArrayBuffer {
  if (
    view.buffer instanceof ArrayBuffer &&
    view.byteOffset === 0 &&
    view.byteLength === view.buffer.byteLength
  ) {
    return view.buffer;
  }

  const copy = new Uint8Array(view.byteLength);
  copy.set(view);
  return copy.buffer;
}

export async function loadTiles(
  files: File[],
  grid: number,
  alpha: AlphaName,
  onProgress?: (p: Progress) => void,
): Promise<TilesLoaded> {
  await invoke("mosaic_reset");

  const images = files.filter((f) => f.type.startsWith("image/"));
  let skipped = files.length - images.length;
  let added = 0;

  for (let i = 0; i < images.length; i++) {
    try {
      const tile = await decodeThumbnail(images[i], TILE_MAX_EDGE);
      await invoke("mosaic_add_tile", toArrayBuffer(tile.data), {
        headers: {
          "x-tile-id": String(added),
          "x-tile-w": String(tile.w),
          "x-tile-h": String(tile.h),
        },
      });
      added++;
    } catch {
      skipped++;
    }
    if (onProgress && (i % PROGRESS_EVERY === 0 || i === images.length - 1)) {
      onProgress({ phase: "decode", done: i + 1, total: images.length });
    }
  }

  onProgress?.({ phase: "gather", done: 0, total: 1 });
  return invoke<TilesLoaded>("mosaic_gather", {
    grid,
    alpha: alphaToU32(alpha),
    skipped,
  });
}

export async function render(
  kind: RenderKind,
  target: File,
  params: ParamsDTO,
  onProgress?: (p: Progress) => void,
): Promise<MosaicResult> {
  onProgress?.({ phase: "render", done: 0, total: 1 });
  const decoded = await decodeFull(target);

  const ab = await invoke<ArrayBuffer>("mosaic_render", toArrayBuffer(decoded.data), {
    headers: {
      "x-kind": kind,
      "x-target-w": String(decoded.w),
      "x-target-h": String(decoded.h),
      "x-params": JSON.stringify(params),
    },
  });

  const dv = new DataView(ab);
  const w = dv.getUint32(0, true);
  const h = dv.getUint32(4, true);
  return { w, h, buf: ab.slice(8) };
}
