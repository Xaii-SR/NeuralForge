"use client";

import { useState } from "react";

export interface ContextSource {
  file_path: string;
  start_line: number;
  end_line: number;
  text: string;
  score: number;
}

export interface ContextAccordionProps {
  sources: ContextSource[];
}

export default function ContextAccordion({ sources }: ContextAccordionProps) {
  const [isExpanded, setIsExpanded] = useState(false);

  if (!sources || sources.length === 0) return null;

  return (
    <div className="mt-2 overflow-hidden rounded border border-neutral-200 bg-neutral-50 dark:border-neutral-800 dark:bg-[#1a1a1a]">
      <button
        onClick={() => setIsExpanded(!isExpanded)}
        className="flex w-full items-center justify-between px-3 py-1.5 text-[11px] text-neutral-500 transition-colors hover:bg-neutral-100 dark:hover:bg-neutral-800/60"
      >
        <span>
          <span className="text-blue-600 dark:text-blue-400">🔍</span> Used {sources.length} file{sources.length > 1 ? "s" : ""} as context
        </span>
        <span className="text-[10px]">{isExpanded ? "▲" : "▼"}</span>
      </button>

      {isExpanded && (
        <div className="divide-y divide-neutral-200 border-t border-neutral-200 dark:divide-neutral-800 dark:border-neutral-800">
          {sources.map((src, i) => (
            <div key={i} className="px-3 py-2 text-[11px]">
              <div className="mb-1 flex items-center justify-between">
                <span className="max-w-[70%] truncate font-medium text-blue-600 dark:text-blue-400">{src.file_path}</span>
                <span className="ml-2 whitespace-nowrap text-[10px] text-neutral-400 dark:text-neutral-500">
                  L{src.start_line}–{src.end_line}
                </span>
              </div>
              <div className="mb-1 flex items-center gap-2">
                <div className="h-1 flex-1 overflow-hidden rounded-full bg-neutral-200 dark:bg-neutral-700">
                  <div
                    className="h-full rounded-full bg-blue-600"
                    style={{ width: `${Math.round(src.score * 100)}%` }}
                  />
                </div>
                <span className="w-8 text-right text-[10px] text-neutral-400 dark:text-neutral-500">{Math.round(src.score * 100)}%</span>
              </div>
              <p className="line-clamp-2 text-[10px] leading-relaxed text-neutral-400 dark:text-neutral-500">{src.text}</p>
            </div>
          ))}
        </div>
      )}
    </div>
  );
}