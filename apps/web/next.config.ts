import type { NextConfig } from "next";

const nextConfig: NextConfig = {
  reactCompiler: true,
  // Fully client-side app (WASM in a worker) -> static site for Cloudflare Pages.
  output: "export",
  // Note: no `*.wasm` rule here. wasm-pack `--target web` fetches the .wasm
  // at runtime via `new URL('kakera_wasm_bg.wasm', import.meta.url)`, and
  // Turbopack treats that as a static asset. Adding a `type: 'wasm'` rule
  // would make Turbopack try to bundle the file as a wasm *module* and emit
  // a broken loader.
};

export default nextConfig;
