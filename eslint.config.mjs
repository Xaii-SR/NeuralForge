import nextConfig from "eslint-config-next";

export default [
  ...nextConfig,
  {
    ignores: [
      "src-tauri/**",
      "node_modules/**",
      ".next/**",
      "out/**",
      "next-env.d.ts",
    ],
  },
];
