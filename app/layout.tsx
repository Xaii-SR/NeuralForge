import type { Metadata } from "next";
import "./globals.css";

export const metadata: Metadata = {
  title: "NeuralForge",
  description: "Local-first AI-native desktop IDE",
};

const THEME_INIT_SCRIPT = `
(function () {
  try {
    var stored = window.localStorage.getItem("neuralforge-theme");
    var theme = stored || "dark";
    if (theme === "dark") document.documentElement.classList.add("dark");
  } catch (e) {}
})();
`;

export default function RootLayout({
  children,
}: {
  children: React.ReactNode;
}) {
  return (
    <html lang="en" suppressHydrationWarning>
      <head>
        <script dangerouslySetInnerHTML={{ __html: THEME_INIT_SCRIPT }} />
      </head>
      <body>{children}</body>
    </html>
  );
}
