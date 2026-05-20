"use client";

import { Box, Button, Stack, Text } from "@chakra-ui/react";
import { useEffect, useRef } from "react";
import type { MosaicResult } from "@/features/mosaic/types";

export function ResultCanvas({ result }: { result: MosaicResult | null }) {
  const canvasRef = useRef<HTMLCanvasElement>(null);

  useEffect(() => {
    const canvas = canvasRef.current;
    if (!canvas || !result) return;
    canvas.width = result.w;
    canvas.height = result.h;
    const ctx = canvas.getContext("2d");
    if (!ctx) return;
    const img = new ImageData(new Uint8ClampedArray(result.buf), result.w, result.h);
    ctx.putImageData(img, 0, 0);
  }, [result]);

  const download = () => {
    const canvas = canvasRef.current;
    if (!canvas) return;
    canvas.toBlob((blob) => {
      if (!blob) return;
      const url = URL.createObjectURL(blob);
      const a = document.createElement("a");
      a.href = url;
      a.download = "mosaic.png";
      a.click();
      URL.revokeObjectURL(url);
    }, "image/png");
  };

  if (!result) {
    return (
      <Box
        borderWidth="1px"
        borderStyle="dashed"
        rounded="md"
        p="10"
        textAlign="center"
        color="fg.muted"
      >
        結果はここに表示されます
      </Box>
    );
  }

  return (
    <Stack gap="3">
      <Text fontSize="sm" color="fg.muted">
        出力: {result.w}×{result.h}px
      </Text>
      <Box overflow="auto" borderWidth="1px" rounded="md" maxH="70vh">
        <canvas ref={canvasRef} style={{ display: "block", maxWidth: "100%", height: "auto" }} />
      </Box>
      <Button colorPalette="teal" alignSelf="flex-start" onClick={download}>
        PNG をダウンロード
      </Button>
    </Stack>
  );
}
