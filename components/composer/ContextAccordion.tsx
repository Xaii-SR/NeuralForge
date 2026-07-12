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
    <div className="mt-2 rounded border border-[#333] bg-[#1a1a1a] overflow-hidden">
      <button
        onClick={() => setIsExpanded(!isExpanded)}
        className="flex w-full items-center justify-between px-3 py-1.5 text-[11px] text-[#888] transition-colors hover:bg-[#252525]"
      >
        <span>
          <span className="text-blue-400">🔍</span> Used {sources.length} file{sources.length > 1 ? "s" : ""} as context
        </span>
        <span className="text-[10px]">{isExpanded ? "▲" : "▼"}</span>
      </button>

      {isExpanded && (
        <div className="border-t border-[#333] divide-y divide-[#333]">
          {sources.map((src, i) => (
            <div key={i} className="px-3 py-2 text-[11px]">
              <div className="flex items-center justify-between mb-1">
                <span className="font-medium text-blue-400 truncate max-w-[70%]">{src.file_path}</span>
                <span className="text-[10px] text-[#666] ml-2 whitespace-nowrap">
                  L{src.start_line}–{src.end_line}
                </span>
              </div>
              <div className="flex items-center gap-2 mb-1">
                <div className="h-1 flex-1 rounded-full bg-[#333] overflow-hidden">
                  <div
                    className="h-full rounded-full bg-blue-600"
                    style={{ width: `${Math.round(src.score * 100)}%` }}
                  />
                </div>
                <span className="text-[10px] text-[#666] w-8 text-right">{Math.round(src.score * 100)}%</span>
              </div>
              <p className="text-[10px] text-[#666] leading-relaxed line-clamp-2">{src.text}</p>
            </div>
          ))}
        </div>
      )}
    </div>
  );
}