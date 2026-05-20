import type { Metadata } from "next";
import { Provider } from "@/shared/components/ui/provider";

export const metadata: Metadata = {
  title: "kakera — photomosaic",
  description: "ターゲット画像と画像フォルダからフォトモザイクを生成",
};

export default function RootLayout({
  children,
}: Readonly<{
  children: React.ReactNode;
}>) {
  return (
    <html lang="ja" suppressHydrationWarning>
      <body>
        <Provider>{children}</Provider>
      </body>
    </html>
  );
}
