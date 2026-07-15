"use client";

export interface ResizeHandleProps {
  /** "vertical" divides columns (drag left/right); "horizontal" divides rows (drag up/down). */
  orientation: "vertical" | "horizontal";
  label: string;
  onPointerDown: (e: React.PointerEvent<HTMLDivElement>) => void;
  onDoubleClick: () => void;
  onNudge: (delta: number) => void;
}

/**
 * VS Code-style sash: an invisible 7px hit area straddling the panel border
 * that highlights on hover/drag. Double-click resets the panel to its default
 * size; arrow keys resize when focused.
 */
export default function ResizeHandle({ orientation, label, onPointerDown, onDoubleClick, onNudge }: ResizeHandleProps) {
  const isVertical = orientation === "vertical";

  const handleKeyDown = (e: React.KeyboardEvent) => {
    const step = e.shiftKey ? 48 : 16;
    if (isVertical && e.key === "ArrowLeft") { e.preventDefault(); onNudge(-step); }
    else if (isVertical && e.key === "ArrowRight") { e.preventDefault(); onNudge(step); }
    else if (!isVertical && e.key === "ArrowUp") { e.preventDefault(); onNudge(step); }
    else if (!isVertical && e.key === "ArrowDown") { e.preventDefault(); onNudge(-step); }
  };

  return (
    <div
      role="separator"
      aria-orientation={orientation}
      aria-label={label}
      tabIndex={0}
      onPointerDown={onPointerDown}
      onDoubleClick={onDoubleClick}
      onKeyDown={handleKeyDown}
      className={`group relative z-20 shrink-0 outline-none ${
        isVertical ? "-mx-[3px] w-[7px] cursor-col-resize" : "-my-[3px] h-[7px] w-full cursor-row-resize"
      }`}
    >
      <div
        className={`absolute bg-transparent transition-colors duration-150 group-hover:bg-blue-500/50 group-focus-visible:bg-blue-500/50 group-active:bg-blue-500 ${
          isVertical ? "inset-y-0 left-1/2 w-[3px] -translate-x-1/2" : "inset-x-0 top-1/2 h-[3px] -translate-y-1/2"
        }`}
      />
    </div>
  );
}
