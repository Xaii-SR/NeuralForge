export interface ErrorBannerProps {
  message: string;
  onDismiss?: () => void;
  onRetry?: () => void;
}

export default function ErrorBanner({ message, onDismiss, onRetry }: ErrorBannerProps) {
  return (
    <div className="flex items-start gap-2 rounded border border-red-900/50 bg-red-950/40 px-3 py-2 text-xs text-red-300 dark:border-red-900/50 dark:bg-red-950/40 dark:text-red-300">
      <span aria-hidden className="mt-0.5">
        ⚠
      </span>
      <span className="min-w-0 flex-1 break-words">{message}</span>
      <div className="flex shrink-0 gap-2">
        {onRetry && (
          <button onClick={onRetry} className="text-red-200 underline decoration-dotted hover:text-white">
            Retry
          </button>
        )}
        {onDismiss && (
          <button onClick={onDismiss} className="text-red-400 hover:text-red-200" aria-label="Dismiss">
            ✕
          </button>
        )}
      </div>
    </div>
  );
}
