"use client";

import { Box, Button, Stack, Text } from "@chakra-ui/react";
import { useEffect, useRef } from "react";

export function TargetImageInput({
  fileName,
  dims,
  previewUrl,
  disabled,
  onSelect,
}: {
  fileName: string | null;
  dims: { w: number; h: number } | null;
  previewUrl: string | null;
  disabled?: boolean;
  onSelect: (file: File) => void;
}) {
  const ref = useRef<HTMLInputElement>(null);
  return (
    <Stack gap="2">
      <Text fontWeight="semibold">1. ターゲット画像</Text>
      <input
        ref={ref}
        type="file"
        accept="image/*"
        hidden
        onChange={(e) => {
          const f = e.target.files?.[0];
          if (f) onSelect(f);
        }}
      />
      <Button
        variant="outline"
        size="sm"
        disabled={disabled}
        onClick={() => ref.current?.click()}
        alignSelf="flex-start"
      >
        画像を選択
      </Button>
      {previewUrl ? (
        // eslint-disable-next-line @next/next/no-img-element
        <img
          src={previewUrl}
          alt="target preview"
          style={{ maxWidth: 200, maxHeight: 140, borderRadius: 6, objectFit: "contain" }}
        />
      ) : null}
      {fileName ? (
        <Text fontSize="xs" color="fg.muted">
          {fileName}
          {dims ? ` — ${dims.w}×${dims.h}px` : ""}
        </Text>
      ) : null}
    </Stack>
  );
}

export function TilesFolderInput({
  count,
  skipped,
  disabled,
  onSelect,
}: {
  count: number | null;
  skipped: number | null;
  disabled?: boolean;
  onSelect: (files: File[]) => void;
}) {
  const ref = useRef<HTMLInputElement>(null);

  // `webkitdirectory` is not in the React typings; set it imperatively.
  useEffect(() => {
    const el = ref.current;
    if (el) {
      el.setAttribute("webkitdirectory", "");
      el.setAttribute("directory", "");
    }
  }, []);

  return (
    <Stack gap="2">
      <Text fontWeight="semibold">2. タイル画像フォルダ</Text>
      <input
        ref={ref}
        type="file"
        multiple
        hidden
        onChange={(e) => {
          const files = e.target.files ? Array.from(e.target.files) : [];
          if (files.length) onSelect(files);
        }}
      />
      <Button
        variant="outline"
        size="sm"
        disabled={disabled}
        onClick={() => ref.current?.click()}
        alignSelf="flex-start"
      >
        フォルダを選択
      </Button>
      {count != null ? (
        <Text fontSize="xs" color="fg.muted">
          {count} 枚のタイルを読み込み
          {skipped ? `（${skipped} 件スキップ）` : ""}
        </Text>
      ) : (
        <Box fontSize="xs" color="fg.muted">
          画像ファイルを含むフォルダを選択してください
        </Box>
      )}
    </Stack>
  );
}
