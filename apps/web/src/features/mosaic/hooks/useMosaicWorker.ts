"use client";

import { useCallback, useEffect, useRef, useState } from "react";
import type {
  AlphaName,
  MosaicResult,
  ParamsDTO,
  Progress,
  RenderKind,
  TilesLoaded,
  WorkerRequest,
  WorkerResponse,
} from "@/features/mosaic/types";

type Pending = { resolve: (v: unknown) => void; reject: (e: Error) => void };

function createWorker(): Worker {
  // Relative specifier (not the `@/` alias) so Turbopack's
  // `new Worker(new URL(...))` detection picks it up.
  return new Worker(new URL("../../../infrastructure/wasm/worker.ts", import.meta.url), {
    type: "module",
  });
}

export interface UseMosaicWorker {
  loadTiles: (files: File[], grid: number, alpha: AlphaName) => Promise<TilesLoaded>;
  render: (kind: RenderKind, target: File, params: ParamsDTO) => Promise<MosaicResult>;
  progress: Progress | null;
  busy: boolean;
}

export function useMosaicWorker(): UseMosaicWorker {
  const workerRef = useRef<Worker | null>(null);
  const pending = useRef(new Map<number, Pending>());
  const idRef = useRef(0);
  const [progress, setProgress] = useState<Progress | null>(null);
  const [busy, setBusy] = useState(false);

  const failAll = useCallback((message: string) => {
    pending.current.forEach((p) => p.reject(new Error(message)));
    pending.current.clear();
  }, []);

  const terminate = useCallback(() => {
    workerRef.current?.terminate();
    workerRef.current = null;
    failAll("worker terminated");
  }, [failAll]);

  const ensureWorker = useCallback(() => {
    if (!workerRef.current) {
      const w = createWorker();
      w.onmessage = (ev: MessageEvent<WorkerResponse>) => {
        const msg = ev.data;
        if (msg.kind === "progress") {
          setProgress({ phase: msg.phase, done: msg.done, total: msg.total });
          return;
        }
        const entry = pending.current.get(msg.id);
        if (!entry) return;
        pending.current.delete(msg.id);
        if (msg.kind === "error") entry.reject(new Error(msg.message));
        else entry.resolve(msg);
      };
      w.onerror = (ev) => failAll(ev.message || "worker error");
      workerRef.current = w;
    }
    return workerRef.current;
  }, [failAll]);

  const send = useCallback(
    <T>(make: (id: number) => WorkerRequest, transfer: Transferable[] = []): Promise<T> => {
      const w = ensureWorker();
      const id = ++idRef.current;
      return new Promise<T>((resolve, reject) => {
        pending.current.set(id, {
          resolve: resolve as (v: unknown) => void,
          reject,
        });
        w.postMessage(make(id), transfer);
      });
    },
    [ensureWorker],
  );

  const track = useCallback(async <T>(p: Promise<T>): Promise<T> => {
    setBusy(true);
    try {
      return await p;
    } finally {
      setBusy(false);
      setProgress(null);
    }
  }, []);

  const loadTiles = useCallback(
    (files: File[], grid: number, alpha: AlphaName) => {
      // Fresh worker each load so old tiles' WASM memory is reclaimed.
      terminate();
      return track(
        send<TilesLoaded>((id) => ({ id, kind: "loadTiles", files, grid, alpha })),
      );
    },
    [send, terminate, track],
  );

  const render = useCallback(
    (kind: RenderKind, target: File, params: ParamsDTO) =>
      track(send<MosaicResult>((id) => ({ id, kind, target, params }))),
    [send, track],
  );

  useEffect(() => () => terminate(), [terminate]);

  return { loadTiles, render, progress, busy };
}
