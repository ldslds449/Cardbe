import { describe, expect, it } from "vitest";

import { due_status } from "./due-status";

describe("due_status", () => {
    const now = new Date(2026, 8, 19, 16, 0);

    it("uses the calendar date when no time is set", () => {
        expect(due_status(new Date(2026, 8, 19), now)).toBe("today");
        expect(due_status(new Date(2026, 8, 18, 23, 59), now)).toBe("overdue");
        expect(due_status(new Date(2026, 8, 20), now)).toBe("normal");
    });

    it("keeps the existing time-based behavior when a time is set", () => {
        expect(due_status(new Date(2026, 8, 19, 9), now)).toBe("overdue");
        expect(due_status(new Date(2026, 8, 19, 23), now)).toBe("soon");
        expect(due_status(new Date(2026, 8, 21, 16), now)).toBe("normal");
    });
});
