"use client";

import { useEffect, useRef } from "react";

export interface MentionItem {
  label: string;
  type: "file" | "doc" | "web" | "git";
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
      className="fixed z-[60] max-h-48 w-64 overflow-y-auto rounded-lg border border-neutral-200 bg-white py-1 shadow-2xl dark:border-neutral-700 dark:bg-neutral-900"
      style={{ left: x, top: y }}>
      {filtered.map((item, i) => (
        <button key={item.label} data-index={i} onClick={() => onSelect(item)}
          className={`w-full px-3 py-1.5 text-left text-sm transition-colors ${i === activeIndex ? "bg-blue-600 text-white" : "text-neutral-700 hover:bg-neutral-100 dark:text-neutral-200 dark:hover:bg-neutral-800"}`}>
          <span className={`mr-2 ${i === activeIndex ? "text-blue-200" : "text-neutral-400 dark:text-neutral-500"}`}>{item.type === "doc" ? "📚" : item.type === "web" ? "🌐" : item.type === "git" ? "🌿" : "@"}</span>
          <span>{item.label}</span>
          {item.type === "doc" && <span className={`ml-2 text-[10px] ${i === activeIndex ? "text-emerald-200" : "text-emerald-600 dark:text-emerald-400"}`}>Docs</span>}
        </button>
      ))}
    </div>
  );
}