// Why a screen has nothing to show, told apart by what the request said: no right to it (denied, never
// shown as empty), no network (offline), or anything else (failed). The retry stays with the caller.

import { ApiError } from "@ferrum/client";
import { StatePanel } from "@ferrum/kit";

export function LoadFailed({ error, onRetry, title }: { error: unknown; onRetry?: () => void; title?: string }) {
  if (error instanceof ApiError && (error.status === 401 || error.status === 403)) return <StatePanel kind="denied" />;
  if (typeof navigator !== "undefined" && !navigator.onLine) return <StatePanel kind="offline" onRetry={onRetry} />;
  return <StatePanel kind="failed" title={title} error={error} onRetry={onRetry} />;
}
