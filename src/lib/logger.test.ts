import { beforeEach, describe, expect, it, vi } from "vitest";

const invoke = vi.hoisted(() => vi.fn().mockResolvedValue(undefined));
vi.mock("@tauri-apps/api/core", () => ({ invoke }));

import { logger } from "./logger";

describe("frontend logger", () => {
  beforeEach(() => invoke.mockClear());

  it("records a fixed event without including a caught error", async () => {
    logger.error("note.save.failed");

    await vi.waitFor(() => expect(invoke).toHaveBeenCalledOnce());
    expect(invoke).toHaveBeenCalledWith("log_frontend", {
      level: "error",
      target: "frontend.note",
      message: "note.save.failed",
    });
  });

  it("records context for local diagnostics", async () => {
    logger.warn("share.sync.failed", undefined, {
      boardId: 2,
      details: { title: "Private board", token: "secret-value" },
      line: 12,
      filename: "C:/Users/Alice/private-board.ts",
    });

    await vi.waitFor(() => expect(invoke).toHaveBeenCalledOnce());
    const message = invoke.mock.calls[0][1].message as string;
    expect(message).toContain('"boardId":2');
    expect(message).toContain('"line":12');
    expect(message).toContain("Private board");
    expect(message).toContain("secret-value");
    expect(message).toContain("Alice");
  });

  it("records error messages and stacks", async () => {
    const error = new Error('sqlite busy at C:/Users/Alice/private-board.db token=secret-value for board "Private Board"');
    error.stack = "at C:/Users/Alice/private-board.ts:12:4";
    logger.error("share.sync.failed", error);

    await vi.waitFor(() => expect(invoke).toHaveBeenCalledOnce());
    const message = invoke.mock.calls[0][1].message as string;
    expect(message).toContain("sqlite busy");
    expect(message).toContain("private-board.db");
    expect(message).toContain("secret-value");
    expect(message).toContain(":12:4");
    expect(message).toContain("Alice");
    expect(message).toContain("Private Board");
  });

  it("records error codes and wrapped causes", async () => {
    const cause = Object.assign(new Error("filesystem is busy"), { code: "E_BUSY" });
    logger.error("calendar.export.failed", new Error("Couldn't load PDF font", { cause }));

    await vi.waitFor(() => expect(invoke).toHaveBeenCalledOnce());
    const message = invoke.mock.calls[0][1].message as string;
    expect(message).toContain('"code":"E_BUSY"');
    expect(message).toContain("filesystem is busy");
  });

  it("records native error strings", async () => {
    logger.error("share.sync.failed", "connection refused https://share.example/invite/private-ticket");

    await vi.waitFor(() => expect(invoke).toHaveBeenCalledOnce());
    const message = invoke.mock.calls[0][1].message as string;
    expect(message).toContain("connection refused");
    expect(message).toContain("https://share.example/invite/private-ticket");
  });

  it("does not throw when error metadata cannot be inspected", async () => {
    const error = Object.defineProperty({}, "message", {
      enumerable: true,
      get: () => {
        throw new Error("private value");
      },
    });

    expect(() => logger.error("note.save.failed", error)).not.toThrow();
    await vi.waitFor(() => expect(invoke).toHaveBeenCalledOnce());
    expect(invoke.mock.calls[0][1].message).not.toContain("private value");
  });
});
