"use client";

export interface Tab {
  path: string;
  isDirty: boolean;
}

export interface TabBarProps {
  tabs: Tab[];
  activePath: string | null;
  onSelect: (path: string) => void;
  onClose: (path: string) => void;
}

export default function TabBar({ tabs, activePath, onSelect, onClose }: TabBarProps) {
  return (
    <div className="flex h-9 shrink-0 overflow-x-auto border-b border-neutral-800 bg-neutral-900">
      {tabs.map((tab) => {
        const name = tab.path.split(/[/\\]/).pop() ?? tab.path;
        const isActive = tab.path === activePath;
        return (
          <div
            key={tab.path}
            onClick={() => onSelect(tab.path)}
            className={`flex cursor-pointer items-center gap-2 border-r border-neutral-800 px-3 text-xs ${
              isActive
                ? "bg-neutral-800 text-neutral-100"
                : "text-neutral-400 hover:bg-neutral-850 hover:text-neutral-200"
            }`}
          >
            <span>
              {name}
              {tab.isDirty ? " ●" : ""}
            </span>
            <button
              aria-label={`Close ${name}`}
              onClick={(e) => {
                e.stopPropagation();
                onClose(tab.path);
              }}
              className="rounded px-1 text-neutral-500 hover:bg-neutral-700 hover:text-neutral-200"
            >
              ×
            </button>
          </div>
        );
      })}
    </div>
  );
}
