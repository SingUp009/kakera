"use client";

import { Box, Grid, Input } from "@chakra-ui/react";
import type { AlphaName, ParamsDTO } from "@/features/mosaic/types";
import { Field } from "./Field";

const selectStyle: React.CSSProperties = {
  width: "100%",
  padding: "6px 8px",
  borderRadius: 6,
  border: "1px solid var(--chakra-colors-border, #5554)",
  background: "transparent",
  color: "inherit",
};

export function ParamsForm({
  value,
  onChange,
  disabled,
}: {
  value: ParamsDTO;
  onChange: (p: ParamsDTO) => void;
  disabled?: boolean;
}) {
  const set = <K extends keyof ParamsDTO>(key: K, v: ParamsDTO[K]) =>
    onChange({ ...value, [key]: v });

  const num = (s: string, fallback: number) => {
    const n = Number(s);
    return Number.isFinite(n) ? n : fallback;
  };

  return (
    <Grid templateColumns={{ base: "1fr", sm: "1fr 1fr" }} gap="4">
      <Field label="グリッド (N×N 特徴)" hint="1〜8。索引と一致させる必要があります">
        <Input
          type="number"
          min={1}
          max={8}
          value={value.grid}
          disabled={disabled}
          onChange={(e) => set("grid", num(e.target.value, value.grid))}
        />
      </Field>

      <Field label="アルファ">
        <select
          style={selectStyle}
          value={value.alpha}
          disabled={disabled}
          onChange={(e) => set("alpha", e.target.value as AlphaName)}
        >
          <option value="Ignore">Ignore（無視）</option>
          <option value="Weighted">Weighted（α重み付け）</option>
        </select>
      </Field>

      <Field label="セル幅 (px)">
        <Input
          type="number"
          min={1}
          value={value.cell_width}
          disabled={disabled}
          onChange={(e) => set("cell_width", num(e.target.value, value.cell_width))}
        />
      </Field>

      <Field label="セル高 (px)">
        <Input
          type="number"
          min={1}
          value={value.cell_height}
          disabled={disabled}
          onChange={(e) => set("cell_height", num(e.target.value, value.cell_height))}
        />
      </Field>

      <Field label="出力拡大率" hint="1〜（整数）。粒度は変えず出力を拡大">
        <Input
          type="number"
          min={1}
          value={value.output_scale}
          disabled={disabled}
          onChange={(e) => set("output_scale", num(e.target.value, value.output_scale))}
        />
      </Field>

      <Field label={`色補正: ${value.color_adjust.toFixed(2)}`} hint="0=オフ, 1=完全に乗算">
        <input
          type="range"
          min={0}
          max={1}
          step={0.05}
          value={value.color_adjust}
          disabled={disabled}
          style={{ width: "100%" }}
          onChange={(e) => set("color_adjust", num(e.target.value, value.color_adjust))}
        />
      </Field>

      <Box>
        <label style={{ display: "flex", gap: 8, alignItems: "center", fontSize: 14 }}>
          <input
            type="checkbox"
            checked={value.ensure_all_tiles}
            disabled={disabled}
            onChange={(e) => set("ensure_all_tiles", e.target.checked)}
          />
          全タイルを最低1回使う
        </label>
        <label
          style={{ display: "flex", gap: 8, alignItems: "center", fontSize: 14, marginTop: 8 }}
        >
          <input
            type="checkbox"
            checked={value.avoid_adjacent_duplicates}
            disabled={disabled}
            onChange={(e) => set("avoid_adjacent_duplicates", e.target.checked)}
          />
          隣接セルの重複を避ける
        </label>
      </Box>

      <Box>
        <label style={{ display: "flex", gap: 8, alignItems: "center", fontSize: 14 }}>
          <input
            type="checkbox"
            checked={value.max_tile_usage != null}
            disabled={disabled}
            onChange={(e) => set("max_tile_usage", e.target.checked ? 10 : null)}
          />
          1タイルの使用回数上限
        </label>
        {value.max_tile_usage != null ? (
          <Input
            mt="2"
            type="number"
            min={1}
            value={value.max_tile_usage}
            disabled={disabled}
            onChange={(e) => set("max_tile_usage", Math.max(1, num(e.target.value, 1)))}
          />
        ) : null}
      </Box>
    </Grid>
  );
}
