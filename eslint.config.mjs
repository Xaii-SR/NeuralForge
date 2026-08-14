import nextConfig from "eslint-config-next";

const config = [
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

export default config;
