export type DueStatus = "overdue" | "today" | "soon" | "normal";

export function due_status(due_time: Date, now: Date): DueStatus {
    const has_time = due_time.getHours() !== 0 || due_time.getMinutes() !== 0;
    if (has_time) {
        const remaining = due_time.getTime() - now.getTime();
        const one_day = 24 * 60 * 60 * 1000;

        if (remaining < 0) return "overdue";
        if (remaining <= one_day) return "soon";
        return "normal";
    }

    const today_start = new Date(
        now.getFullYear(),
        now.getMonth(),
        now.getDate(),
    ).getTime();
    const tomorrow_start = new Date(
        now.getFullYear(),
        now.getMonth(),
        now.getDate() + 1,
    ).getTime();
    const due_timestamp = due_time.getTime();

    if (due_timestamp < today_start) return "overdue";
    if (due_timestamp < tomorrow_start) return "today";
    return "normal";
}
