import { invoke } from "@tauri-apps/api/core";

type LogLevel = "debug" | "info" | "warn" | "error";
type LogContext = Record<string, unknown>;

const originalConsoleError = console.error.bind(console);

function safeValue(value: unknown, depth = 0): unknown {
  if (typeof value === "bigint") return value.toString();
  if (value === null || typeof value !== "object") return value;
  if (depth >= 3) return "[object omitted]";
  if (value instanceof Error) {
    const details: Record<string, unknown> = {
      name: value.name,
      message: value.message,
      stack: value.stack,
    };
    const coded_error = value as Error & { code?: unknown; cause?: unknown };
    if ("code" in value) details.code = safeValue(coded_error.code, depth + 1);
    if (coded_error.cause !== undefined) details.cause = safeValue(coded_error.cause, depth + 1);
    return details;
  }
  if (Array.isArray(value)) return value.slice(0, 20).map((item) => safeValue(item, depth + 1));

  return Object.fromEntries(
    Object.entries(value as Record<string, unknown>)
      .slice(0, 50)
      .map(([key, nestedValue]) => [key, safeValue(nestedValue, depth + 1)]),
  );
}

function safeContext(context?: LogContext): LogContext | undefined {
  return context ? safeValue(context) as LogContext : undefined;
}

function serialize(error?: unknown, context?: LogContext): string {
  try {
    const payload: Record<string, unknown> = {};
    if (error !== undefined) payload.error = safeValue(error);
    const sanitized = safeContext(context);
    if (sanitized) payload.context = sanitized;
    if (Object.keys(payload).length === 0) return "";
    return JSON.stringify(payload);
  } catch {
    return JSON.stringify({ error: true });
  }
}

async function write(
  level: LogLevel,
  event: string,
  error?: unknown,
  context?: LogContext,
): Promise<void> {
  const [scope] = event.split(".", 1);
  const metadata = serialize(error, context);
  const message = metadata ? `${event} ${metadata}` : event;
  try {
    await invoke("log_frontend", {
      level,
      target: `frontend.${scope || "app"}`,
      message,
    });
  } catch (loggingError) {
    originalConsoleError("Could not persist frontend log", loggingError);
  }
}

export const logger = {
  debug: (event: string, context?: LogContext) => void write("debug", event, undefined, context),
  info: (event: string, context?: LogContext) => void write("info", event, undefined, context),
  warn: (event: string, error?: unknown, context?: LogContext) =>
    void write("warn", event, error, context),
  error: (event: string, error?: unknown, context?: LogContext) =>
    void write("error", event, error, context),
};

export function installGlobalErrorLogging(): () => void {
  const onError = (event: ErrorEvent) =>
    logger.error("window.error", event.error ?? event.message, {
      filename: event.filename,
      line: event.lineno,
      column: event.colno,
    });
  const onUnhandledRejection = (event: PromiseRejectionEvent) =>
    logger.error("window.unhandled_rejection", event.reason);

  window.addEventListener("error", onError);
  window.addEventListener("unhandledrejection", onUnhandledRejection);
  return () => {
    window.removeEventListener("error", onError);
    window.removeEventListener("unhandledrejection", onUnhandledRejection);
  };
}
