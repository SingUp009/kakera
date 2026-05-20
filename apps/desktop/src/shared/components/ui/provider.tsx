"use client";

import { ChakraProvider, defaultSystem } from "@chakra-ui/react";
import { useEffect, useState } from "react";
import { ColorModeProvider, type ColorModeProviderProps } from "./color-mode";

/**
 * Client-only provider. We render `null` on the server and on the very first
 * client render, then mount the Chakra + next-themes tree after `useEffect`
 * fires. This avoids a hydration mismatch caused by emotion's in-tree SSR
 * `<style>` insertion competing with next-themes' init `<script>`, without
 * adding `@emotion/cache` + `useServerInsertedHTML` plumbing. Acceptable
 * because the entire app is client-only (Tauri webview).
 */
export function Provider(props: ColorModeProviderProps) {
  const [mounted, setMounted] = useState(false);
  useEffect(() => setMounted(true), []);

  if (!mounted) return null;

  return (
    <ChakraProvider value={defaultSystem}>
      <ColorModeProvider {...props} />
    </ChakraProvider>
  );
}
