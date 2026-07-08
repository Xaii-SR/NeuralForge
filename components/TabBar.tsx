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
    <div className="flex h-9 shrink-0 overflow-x-auto border-b border-neutral-200 bg-neutral-50 dark:border-neutral-800 dark:bg-neutral-900">
      {tabs.map((tab) => {
        const name = tab.path.split(/[/\\]/).pop() ?? tab.path;
        const isActive = tab.path === activePath;
        return (
          <div
            key={tab.path}
            onClick={() => onSelect(tab.path)}
            className={`group flex cursor-pointer items-center gap-2 border-r border-neutral-200 px-3 text-xs transition-colors dark:border-neutral-800 ${
              isActive
                ? "bg-white text-neutral-900 dark:bg-neutral-800 dark:text-neutral-100"
                : "text-neutral-500 hover:bg-neutral-100 hover:text-neutral-800 dark:text-neutral-400 dark:hover:bg-neutral-800/60 dark:hover:text-neutral-200"
            }`}
          >
            <span className="flex items-center gap-1">
              {name}
              {tab.isDirty && <span className="text-blue-500 dark:text-blue-400">●</span>}
            </span>
            <button
              aria-label={`Close ${name}`}
              onClick={(e) => {
                e.stopPropagation();
                onClose(tab.path);
              }}
              className="rounded px-1 text-neutral-400 opacity-0 transition-opacity hover:bg-neutral-200 hover:text-neutral-700 group-hover:opacity-100 dark:text-neutral-500 dark:hover:bg-neutral-700 dark:hover:text-neutral-200"
            >
              ×
            </button>
          </div>
        );
      })}
    </div>
  );
}
