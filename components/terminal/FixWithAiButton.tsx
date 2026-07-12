"use client";

export interface FixWithAiButtonProps {
  onClick: () => void;
}

export default function FixWithAiButton({ onClick }: FixWithAiButtonProps) {
  return (
    <button
      onClick={onClick}
      className="absolute right-2 top-2 z-10 animate-pulse rounded bg-red-700 px-3 py-1 text-xs font-semibold text-white shadow-lg transition-colors hover:bg-red-600"
    >
      ✦ Fix with AI
    </button>
  );
}