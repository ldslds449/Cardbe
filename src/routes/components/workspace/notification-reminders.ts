import { m } from "$lib/paraglide/messages.js";

// Matches the backend's allowed reminder offsets.
export const reminderMinutes = [0, 5, 15, 30, 60, 1440];

export function reminderLabel(minutes: number) {
  if (minutes === 0) {
    return m.notification_reminder_off();
  }
  if (minutes % 1440 === 0) {
    return m.notification_reminder_days({ count: minutes / 1440 });
  }
  if (minutes % 60 === 0) {
    return m.notification_reminder_hours({ count: minutes / 60 });
  }
  return m.notification_reminder_minutes({ count: minutes });
}
