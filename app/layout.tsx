import type { Metadata } from "next";
import "./globals.css";

export const metadata: Metadata = {
  title: "NeuralForge",
  description: "Local-first AI-native desktop IDE",
};

export default function RootLayout({
  children,
}: {
  children: React.ReactNode;
}) {
  return (
    <html lang="en">
      <body>{children}</body>
    </html>
  );
}
