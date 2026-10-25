import { m } from "#lib/paraglide/messages.js";
import { formatDate, formatDateTime } from "#lib/i18n/index.js";
import { due_status, type DueStatus } from "./due-status";
import { is_date_only } from "../../utils/task-date-range";

export function due_label(
  due_time: Date,
  now: Date,
  at_midnight = false,
): { text: string; status: DueStatus } {
  const include_year = due_time.getFullYear() !== now.getFullYear();
  const options: Intl.DateTimeFormatOptions = {
    month: "short",
    day: "numeric",
    year: include_year ? "numeric" : undefined,
  };
  const date = !is_date_only(due_time, at_midnight)
    ? formatDateTime(due_time, {
        ...options,
        hour: "2-digit",
        minute: "2-digit",
      })
    : formatDate(due_time, options);
  const status = due_status(due_time, now, at_midnight);
  return {
    text:
      status === "today"
        ? m.task_due_today({ date })
        : status === "soon"
          ? m.task_due_soon({ date })
          : date,
    status,
  };
}

export function due_label_class(status: DueStatus): string {
  return status === "overdue"
    ? "text-destructive"
    : status === "today" || status === "soon"
      ? "font-medium text-warning"
      : "text-muted-foreground";
}
