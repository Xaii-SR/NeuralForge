"use client";

export interface ContextPillProps {
  filePath: string;
  onRemove: (path: string) => void;
}

export default function ContextPill({ filePath, onRemove }: ContextPillProps) {
  const basename = filePath.split("/").pop() || filePath;

  return (
    <span className="inline-flex items-center gap-1 rounded-full bg-[#2a2d2e] pl-2 pr-1 py-0.5 text-[11px] text-[#d4d4d4] border border-[#444]">
      <span className="text-[10px] text-blue-400">@</span>
      <span className="max-w-[120px] truncate">{basename}</span>
      <button
        onClick={() => onRemove(filePath)}
        className="ml-0.5 inline-flex h-3.5 w-3.5 items-center justify-center rounded-full text-[9px] text-[#888] transition-colors hover:bg-[#444] hover:text-white"
      >
        ✕
      </button>
    </span>
  );
}