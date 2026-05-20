/**
 * Image decode helpers. Produces tightly-packed, non-premultiplied sRGB RGBA8 —
 * exactly what `kakera_core::RgbaImage::new` expects. Do NOT premultiply alpha;
 * `AlphaPolicy::Weighted` is handled inside the core.
 */

export interface DecodedImage {
  w: number;
  h: number;
  /** `w * h * 4` bytes, row-major RGBA8. */
  data: Uint8Array;
}

function toRgba(bitmap: ImageBitmap, w: number, h: number): DecodedImage {
  const canvas = new OffscreenCanvas(w, h);
  const ctx = canvas.getContext("2d", { willReadFrequently: true });
  if (!ctx) {
    throw new Error("2D canvas context unavailable");
  }
  ctx.drawImage(bitmap, 0, 0, w, h);
  const img = ctx.getImageData(0, 0, w, h);
  // Copy out of the clamped array's buffer into a plain Uint8Array.
  return { w, h, data: new Uint8Array(img.data.buffer.slice(0)) };
}

/** Decode at native resolution (used for the target image). */
export async function decodeFull(file: File): Promise<DecodedImage> {
  const bitmap = await createImageBitmap(file);
  try {
    return toRgba(bitmap, bitmap.width, bitmap.height);
  } finally {
    bitmap.close();
  }
}

/**
 * Decode a tile downscaled so its longest edge is `<= maxEdge`. Source detail
 * beyond the rendered cell size is discarded by the core anyway, so this keeps
 * the IPC payload to the Rust side bounded.
 */
export async function decodeThumbnail(file: File, maxEdge: number): Promise<DecodedImage> {
  const probe = await createImageBitmap(file);
  const longest = Math.max(probe.width, probe.height);
  const scale = longest > maxEdge ? maxEdge / longest : 1;
  const w = Math.max(1, Math.round(probe.width * scale));
  const h = Math.max(1, Math.round(probe.height * scale));
  probe.close();

  const bitmap = await createImageBitmap(file, {
    resizeWidth: w,
    resizeHeight: h,
    resizeQuality: "high",
  });
  try {
    return toRgba(bitmap, w, h);
  } finally {
    bitmap.close();
  }
}
