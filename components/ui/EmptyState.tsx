export interface EmptyStateProps {
  icon?: string;
  title: string;
  hint?: string;
}

export default function EmptyState({ icon, title, hint }: EmptyStateProps) {
  return (
    <div className="flex h-full flex-col items-center justify-center gap-2 px-6 text-center">
      {icon && <div className="text-2xl opacity-40">{icon}</div>}
      <div className="text-sm font-medium text-neutral-500 dark:text-neutral-400">{title}</div>
      {hint && <div className="max-w-xs text-xs text-neutral-400 dark:text-neutral-600">{hint}</div>}
    </div>
  );
}
