/**
 * Smartly truncates terminal output to protect the LLM's context window.
 * Keeps the UI display complete (untruncated) while the AI feedback loop
 * receives a compressed version.
 *
 * @param rawOutput - The full terminal output string.
 * @param maxLines - Maximum number of lines to send to the AI (default 200).
 * @returns Truncated string with preserved head/tail and a removal marker.
 */
export function truncateTerminalOutput(rawOutput: string, maxLines: number = 200): string {
  const lines = rawOutput.split("\n");
  if (lines.length <= maxLines) {
    return rawOutput;
  }

  const HEAD_LINES = 50;
  const TAIL_LINES = maxLines - HEAD_LINES;
  const removedCount = lines.length - HEAD_LINES - TAIL_LINES;

  const head = lines.slice(0, HEAD_LINES).join("\n");
  const tail = lines.slice(lines.length - TAIL_LINES).join("\n");

  return `${head}\n... [Truncated ${removedCount} lines] ...\n${tail}`;
}