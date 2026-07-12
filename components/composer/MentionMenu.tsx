"use client";

import { useEffect, useRef } from "react";

export interface MentionItem {
  label: string;
  type: "file" | "doc";
}

export interface MentionMenuProps {
  x: number;
  y: number;
  query: string;
  items: MentionItem[];
  activeIndex: number;
  onSelect: (item: MentionItem) => void;
  onClose: () => void;
}

export default function MentionMenu({
  x, y, query, items, activeIndex, onSelect, onClose,
}: MentionMenuProps) {
  const containerRef = useRef<HTMLDivElement>(null);

  const filtered = query
    ? items.filter((item) => item.label.toLowerCase().includes(query.toLowerCase()))
    : items;

  useEffect(() => {
    const el = containerRef.current?.querySelector(`[data-index="${activeIndex}"]`) as HTMLElement | null;
    el?.scrollIntoView({ block: "nearest" });
  }, [activeIndex]);

  if (!filtered.length) return null;

  return (
    <div ref={containerRef}
      className="fixed z-[60] max-h-48 w-64 overflow-y-auto rounded-lg border border-[#444] bg-[#1e1e1e] py-1 shadow-2xl"
      style={{ left: x, top: y }}>
      {filtered.map((item, i) => (
        <button key={item.label} data-index={i} onClick={() => onSelect(item)}
          className={`w-full px-3 py-1.5 text-left text-sm transition-colors ${i === activeIndex ? "bg-blue-700 text-white" : "text-[#d4d4d4] hover:bg-[#333]"}`}>
          <span className="mr-2 text-[#888]">{item.type === "doc" ? "📚" : "@"}</span>
          <span>{item.label}</span>
          {item.type === "doc" && <span className="ml-2 text-[10px] text-emerald-400">Docs</span>}
        </button>
      ))}
    </div>
  );
}