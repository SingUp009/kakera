import { describe, expect, it } from "vitest";
import { computeOutputSize, outputSizeError } from "./outputSize";

describe("computeOutputSize", () => {
  it("matches MosaicGrid::compute ceil + out-size (100x100, cell 16, scale 1)", () => {
    // kakera-core test `grid_compute_ceil_and_out_size`: 100/16 -> 7 cols/rows,
    // out = 7*16 = 112.
    const s = computeOutputSize(100, 100, {
      cell_width: 16,
      cell_height: 16,
      output_scale: 1,
    });
    expect(s).not.toBeNull();
    expect([s!.cols, s!.rows]).toEqual([7, 7]);
    expect([s!.width, s!.height]).toEqual([112, 112]);
    expect(s!.bytes).toBe(112 * 112 * 4);
  });

  it("applies output_scale", () => {
    const s = computeOutputSize(64, 32, {
      cell_width: 16,
      cell_height: 16,
      output_scale: 4,
    });
    expect([s!.width, s!.height]).toEqual([64 * 4, 32 * 4]);
  });

  it("returns null when target is smaller than one cell (TargetTooSmall)", () => {
    expect(
      computeOutputSize(8, 8, { cell_width: 16, cell_height: 16, output_scale: 1 }),
    ).toBeNull();
  });

  it("flags oversized output", () => {
    const s = computeOutputSize(20000, 20000, {
      cell_width: 16,
      cell_height: 16,
      output_scale: 1,
    });
    expect(outputSizeError(s!)).not.toBeNull();
  });
});
