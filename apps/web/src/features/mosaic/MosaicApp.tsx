"use client";

import { Box, Button, Container, Heading, HStack, Stack, Text } from "@chakra-ui/react";
import { useCallback, useEffect, useRef, useState } from "react";
import { ColorModeButton } from "@/shared/components/ui/color-mode";
import { useMosaicWorker } from "@/features/mosaic/hooks/useMosaicWorker";
import { computeOutputSize, outputSizeError } from "@/features/mosaic/lib/outputSize";
import {
  DEFAULT_PARAMS,
  type MosaicResult,
  type ParamsDTO,
  type RenderKind,
} from "@/features/mosaic/types";
import { ParamsForm } from "./components/ParamsForm";
import { ResultCanvas } from "./components/ResultCanvas";
import { TargetImageInput, TilesFolderInput } from "./components/SourcePickers";

export function MosaicApp() {
  const { loadTiles, render, progress, busy } = useMosaicWorker();

  const [targetFile, setTargetFile] = useState<File | null>(null);
  const [targetDims, setTargetDims] = useState<{ w: number; h: number } | null>(null);
  const [targetUrl, setTargetUrl] = useState<string | null>(null);

  const [tileFiles, setTileFiles] = useState<File[]>([]);
  const folderToken = useRef(0);
  const [tilesInfo, setTilesInfo] = useState<{ count: number; skipped: number } | null>(null);

  const [params, setParams] = useState<ParamsDTO>(DEFAULT_PARAMS);
  const [result, setResult] = useState<MosaicResult | null>(null);
  const [error, setError] = useState<string | null>(null);

  // Signature of the index currently loaded in the worker.
  const loadedSig = useRef<string | null>(null);

  useEffect(() => {
    if (!targetUrl) return;
    return () => URL.revokeObjectURL(targetUrl);
  }, [targetUrl]);

  const onSelectTarget = useCallback(async (file: File) => {
    setTargetFile(file);
    setError(null);
    setTargetUrl((prev) => {
      if (prev) URL.revokeObjectURL(prev);
      return URL.createObjectURL(file);
    });
    try {
      const bmp = await createImageBitmap(file);
      setTargetDims({ w: bmp.width, h: bmp.height });
      bmp.close();
    } catch {
      setTargetDims(null);
    }
  }, []);

  const onSelectTiles = useCallback((files: File[]) => {
    setTileFiles(files);
    folderToken.current += 1;
    loadedSig.current = null;
    setTilesInfo(null);
    setError(null);
  }, []);

  const run = useCallback(
    async (kind: RenderKind) => {
      setError(null);
      if (!targetFile || !targetDims) {
        setError("ターゲット画像を選択してください");
        return;
      }
      if (tileFiles.length === 0) {
        setError("タイル画像フォルダを選択してください");
        return;
      }
      const size = computeOutputSize(targetDims.w, targetDims.h, params);
      if (!size) {
        setError(
          `ターゲット (${targetDims.w}×${targetDims.h}) が1セル (${params.cell_width}×${params.cell_height}) より小さいか、パラメータが不正です`,
        );
        return;
      }
      const sizeErr = outputSizeError(size);
      if (sizeErr) {
        setError(sizeErr);
        return;
      }

      try {
        const sig = `${params.grid}:${params.alpha}:${folderToken.current}`;
        if (loadedSig.current !== sig) {
          const info = await loadTiles(tileFiles, params.grid, params.alpha);
          setTilesInfo(info);
          loadedSig.current = sig;
          if (info.count === 0) {
            setError("有効なタイル画像が見つかりませんでした");
            return;
          }
        }
        const res = await render(kind, targetFile, params);
        setResult(res);
      } catch (e) {
        setError(e instanceof Error ? e.message : String(e));
      }
    },
    [targetFile, targetDims, tileFiles, params, loadTiles, render],
  );

  const progressLabel = progress
    ? progress.phase === "decode"
      ? `タイルをデコード中… ${progress.done}/${progress.total}`
      : progress.phase === "gather"
        ? "色特徴を索引化中…"
        : "モザイクを生成中…"
    : null;

  return (
    <Container maxW="6xl" py="8">
      <HStack justify="space-between" mb="6">
        <Stack gap="0">
          <Heading size="2xl">kakera</Heading>
          <Text color="fg.muted">画像フォルダでフォトモザイクを生成</Text>
        </Stack>
        <ColorModeButton />
      </HStack>

      <Stack
        direction={{ base: "column", lg: "row" }}
        gap="8"
        align="flex-start"
      >
        <Stack gap="6" flex="1" minW="0" w="full">
          <Box borderWidth="1px" rounded="lg" p="5">
            <Stack gap="5">
              <TargetImageInput
                fileName={targetFile?.name ?? null}
                dims={targetDims}
                previewUrl={targetUrl}
                disabled={busy}
                onSelect={onSelectTarget}
              />
              <TilesFolderInput
                count={tilesInfo?.count ?? null}
                skipped={tilesInfo?.skipped ?? null}
                disabled={busy}
                onSelect={onSelectTiles}
              />
            </Stack>
          </Box>

          <Box borderWidth="1px" rounded="lg" p="5">
            <Text fontWeight="semibold" mb="4">
              3. パラメータ
            </Text>
            <ParamsForm value={params} onChange={setParams} disabled={busy} />
          </Box>

          <HStack gap="3">
            <Button
              colorPalette="teal"
              variant="outline"
              loading={busy}
              onClick={() => run("preview")}
            >
              プレビュー
            </Button>
            <Button colorPalette="teal" loading={busy} onClick={() => run("build")}>
              モザイク生成
            </Button>
          </HStack>

          {progressLabel ? (
            <Text fontSize="sm" color="fg.muted">
              {progressLabel}
            </Text>
          ) : null}
          {error ? (
            <Box borderWidth="1px" borderColor="red.500" rounded="md" p="3">
              <Text color="red.500" fontSize="sm">
                {error}
              </Text>
            </Box>
          ) : null}
        </Stack>

        <Box flex="1" minW="0" w="full">
          <ResultCanvas result={result} />
        </Box>
      </Stack>
    </Container>
  );
}
