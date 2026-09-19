import { describe, expect, it } from "vitest";

import { parse_import_summary } from "./import-data";

describe("parse_import_summary", () => {
    it("summarizes supported import collections", () => {
        expect(
            parse_import_summary(
                JSON.stringify({
                    columns: [
                        { tasks: [{ id: 1 }, { id: 2 }] },
                        { tasks: [{ id: 3 }] },
                    ],
                    archives: [{ task: { id: 4 } }],
                    templates: [{ id: 1 }, { id: 2 }],
                }),
            ),
        ).toEqual({ columns: 2, tasks: 3, archives: 1, templates: 2 });
    });

    it("accepts imports without templates", () => {
        expect(parse_import_summary('{"columns":[],"archives":[]}')).toEqual({
            columns: 0,
            tasks: 0,
            archives: 0,
            templates: 0,
        });
    });

    it("rejects imports without the required collections", () => {
        expect(() => parse_import_summary("[]")).toThrow(
            "The import file must contain columns and archives arrays",
        );
        expect(() => parse_import_summary('{"columns":[]}')).toThrow(
            "The import file must contain columns and archives arrays",
        );
    });
});
