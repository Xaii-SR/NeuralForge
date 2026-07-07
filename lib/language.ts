const EXTENSION_TO_LANGUAGE: Record<string, string> = {
  ts: "typescript",
  tsx: "typescript",
  js: "javascript",
  jsx: "javascript",
  json: "json",
  rs: "rust",
  py: "python",
  md: "markdown",
  css: "css",
  html: "html",
  toml: "toml",
  yaml: "yaml",
  yml: "yaml",
  sh: "shell",
};

export function languageFromPath(path: string): string {
  const ext = path.split(".").pop()?.toLowerCase() ?? "";
  return EXTENSION_TO_LANGUAGE[ext] ?? "plaintext";
}
