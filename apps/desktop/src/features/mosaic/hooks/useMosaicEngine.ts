"use client";

import { useCallback, useState } from "react";
import { loadTiles as ipcLoadTiles, render as ipcRender } from "@/infrastructure/tauri/mosaic";
import type {
  AlphaName,
  MosaicResult,
  ParamsDTO,
  Progress,
  RenderKind,
  TilesLoaded,
} from "@/features/mosaic/types";

export interface UseMosaicEngine {
  loadTiles: (files: File[], grid: number, alpha: AlphaName) => Promise<TilesLoaded>;
  render: (kind: RenderKind, target: File, params: ParamsDTO) => Promise<MosaicResult>;
  progress: Progress | null;
  busy: boolean;
}

export function useMosaicEngine(): UseMosaicEngine {
  const [progress, setProgress] = useState<Progress | null>(null);
  const [busy, setBusy] = useState(false);

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
    (files: File[], grid: number, alpha: AlphaName) =>
      track(ipcLoadTiles(files, grid, alpha, setProgress)),
    [track],
  );

  const render = useCallback(
    (kind: RenderKind, target: File, params: ParamsDTO) =>
      track(ipcRender(kind, target, params, setProgress)),
    [track],
  );

  return { loadTiles, render, progress, busy };
}
