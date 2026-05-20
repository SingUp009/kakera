"use client";

import { Box, Text } from "@chakra-ui/react";
import type { ReactNode } from "react";

export function Field({
  label,
  hint,
  children,
}: {
  label: string;
  hint?: string;
  children: ReactNode;
}) {
  return (
    <Box>
      <Text fontSize="sm" fontWeight="medium" mb="1">
        {label}
      </Text>
      {children}
      {hint ? (
        <Text fontSize="xs" color="fg.muted" mt="1">
          {hint}
        </Text>
      ) : null}
    </Box>
  );
}
