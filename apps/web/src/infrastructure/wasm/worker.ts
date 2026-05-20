/// <reference lib="webworker" />
/**
 * Mosaic Web Worker. Owns the WASM `MosaicEngine`, decodes images, and runs
 * the heavy `gather`/`build`/`preview` off the main thread.
 *
 * A fresh worker is spawned per tiles-load (see `useMosaicWorker`) so WASM
 * linear memory is reclaimed by terminating the old worker.
 */
import init, { MosaicEngine } from "@/wasm/pkg/kakera_wasm";
import { decodeFull, decodeThumbnail } from "@/infrastructure/image/decode";
import type { AlphaName, ParamsDTO, WorkerRequest, WorkerResponse } from "@/features/mosaic/types";

/** Longest tile edge kept after thumbnailing (bounds WASM memory). */
const TILE_MAX_EDGE = 96;
/** How often to emit decode progress. */
const PROGRESS_EVERY = 16;

const ctx = self as unknown as DedicatedWorkerGlobalScope;

let engine: MosaicEngine | null = null;
let gatheredGrid: number | null = null;

function post(msg: WorkerResponse, transfer: Transferable[] = []): void {
  ctx.postMessage(msg, transfer);
}

function alphaToU32(a: AlphaName): number {
  return a === "Weighted" ? 1 : 0;
}

async function ensureEngine(): Promise<MosaicEngine> {
  if (!engine) {
    await init();
    engine = new MosaicEngine();
  }
  return engine;
}

async function handleLoadTiles(req: Extract<WorkerRequest, { kind: "loadTiles" }>): Promise<void> {
  const eng = await ensureEngine();
  eng.reset();
  gatheredGrid = null;

  const images = req.files.filter((f) => f.type.startsWith("image/"));
  let skipped = req.files.length - images.length;
  let id = 0;

  for (let i = 0; i < images.length; i++) {
    try {
      const tile = await decodeThumbnail(images[i], TILE_MAX_EDGE);
      eng.add_tile(id, tile.w, tile.h, tile.data);
      id++;
    } catch {
      skipped++;
    }
    if (i % PROGRESS_EVERY === 0 || i === images.length - 1) {
      post({ id: req.id, kind: "progress", phase: "decode", done: i + 1, total: images.length });
    }
  }

  post({ id: req.id, kind: "progress", phase: "gather", done: 0, total: 1 });
  eng.gather(req.grid, alphaToU32(req.alpha));
  gatheredGrid = req.grid;
  post({ id: req.id, kind: "tilesLoaded", count: eng.tile_count(), skipped });
}

async function handleRender(
  req: Extract<WorkerRequest, { kind: "build" | "preview" }>,
): Promise<void> {
  const eng = await ensureEngine();
  if (gatheredGrid == null) {
    throw new Error("タイルが読み込まれていません（先にフォルダを選択してください）");
  }
  // Force params.grid to the gathered grid (mirrors CLI; avoids GridMismatch).
  const params: ParamsDTO = { ...req.params, grid: gatheredGrid };

  post({ id: req.id, kind: "progress", phase: "render", done: 0, total: 1 });
  const target = await decodeFull(req.target);
  const out =
    req.kind === "build"
      ? eng.build(target.w, target.h, target.data, JSON.stringify(params))
      : eng.preview(target.w, target.h, target.data, JSON.stringify(params));

  const w = out.width;
  const h = out.height;
  const bytes = out.take_bytes();
  const buf = bytes.buffer as ArrayBuffer;
  post({ id: req.id, kind: "result", w, h, buf }, [buf]);
}

ctx.onmessage = async (ev: MessageEvent<WorkerRequest>) => {
  const req = ev.data;
  try {
    switch (req.kind) {
      case "reset":
        engine?.reset();
        gatheredGrid = null;
        post({ id: req.id, kind: "tilesLoaded", count: 0, skipped: 0 });
        break;
      case "loadTiles":
        await handleLoadTiles(req);
        break;
      case "build":
      case "preview":
        await handleRender(req);
        break;
    }
  } catch (e) {
    post({
      id: req.id,
      kind: "error",
      message: e instanceof Error ? e.message : String(e),
    });
  }
};
