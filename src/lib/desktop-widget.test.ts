import { afterEach, beforeEach, expect, it, vi } from "vite-plus/test";
import {
  forget_desktop_widget,
  open_desktop_widget,
  read_desktop_widgets,
  restore_desktop_widgets,
  remove_desktop_widget,
  read_widgets,
  open_widget,
  remove_widget,
  compact_widget,
} from "./desktop-widget.svelte";

const mocks = vi.hoisted(() => ({ invoke: vi.fn(), warn: vi.fn() }));
vi.mock("@tauri-apps/api/core", () => ({ invoke: mocks.invoke }));
vi.mock("#lib/logger.js", () => ({ logger: { warn: mocks.warn } }));

let values: Map<string, string>;
beforeEach(() => {
  values = new Map();
  mocks.invoke
    .mockReset()
    .mockImplementation(async (command: string) =>
      command === "desktop_widget_supported" ? true : undefined,
    );
  mocks.warn.mockReset();
  vi.stubGlobal("window", {
    localStorage: {
      getItem: (key: string) => values.get(key) ?? null,
      setItem: (key: string, value: string) => values.set(key, value),
      removeItem: (key: string) => values.delete(key),
    },
  });
});
afterEach(() => vi.unstubAllGlobals());

it("removes only a widget after native destruction succeeds", async () => {
  const task = { kind: "task" as const, boardId: 7, taskId: 2 };
  const focus = { kind: "focus" as const, id: "focus" };
  await open_desktop_widget(task);
  await open_desktop_widget(focus);
  mocks.invoke.mockRejectedValueOnce({ code: "INTERNAL_ERROR" });
  await expect(remove_desktop_widget(task)).rejects.toEqual({
    code: "INTERNAL_ERROR",
  });
  expect(read_desktop_widgets()).toEqual([task, focus]);
  await remove_desktop_widget(task);
  expect(mocks.invoke).toHaveBeenLastCalledWith("remove_widget", {
    widget: task,
    placement: "desktop",
  });
  expect(read_desktop_widgets()).toEqual([focus]);
});

it("opens widgets directly, deduplicates them and keeps floating records separate", async () => {
  const task = { kind: "task" as const, boardId: 1, taskId: 2 };
  const focus = { kind: "focus" as const, id: "今天 / 🐱" };
  values.set("cardbe.floating-tasks", "existing floating windows");
  await open_desktop_widget(task);
  await open_desktop_widget(task);
  await open_desktop_widget({ ...task, boardId: 2 });
  await open_desktop_widget(focus);
  expect(mocks.invoke).toHaveBeenCalledWith("open_desktop_task", {
    boardId: 1,
    taskId: 2,
  });
  expect(mocks.invoke).toHaveBeenCalledWith("open_desktop_focus", {
    id: focus.id,
  });
  expect(read_desktop_widgets()).toHaveLength(3);
  forget_desktop_widget(task);
  expect(read_desktop_widgets()).toEqual([{ ...task, boardId: 2 }, focus]);
  expect(values.get("cardbe.floating-tasks")).toBe("existing floating windows");
});

it("remembers only successful creation and drops missing widgets on restore", async () => {
  const ref = { kind: "focus" as const, id: "test" };
  mocks.invoke.mockRejectedValueOnce({ code: "INTERNAL_ERROR" });
  await expect(open_desktop_widget(ref)).rejects.toEqual({
    code: "INTERNAL_ERROR",
  });
  expect(read_desktop_widgets()).toEqual([]);
  await open_desktop_widget(ref);
  mocks.invoke.mockImplementation(async (command: string) => {
    if (command === "desktop_widget_supported") {
      return true;
    }
    throw { code: "FOCUS_NOT_FOUND" };
  });
  await restore_desktop_widgets();
  expect(read_desktop_widgets()).toEqual([]);
});

it("validates saved references and preserves widgets after transient failures", async () => {
  values.set(
    "cardbe.desktop-widgets",
    JSON.stringify([
      null,
      { kind: "task", boardId: "1", taskId: 2 },
      { kind: "focus", id: "a" },
      { kind: "focus", id: "a", ignored: "old metadata" },
    ]),
  );
  expect(read_desktop_widgets()).toEqual([{ kind: "focus", id: "a" }]);
  mocks.invoke.mockImplementation(async (command: string) => {
    if (command === "desktop_widget_supported") {
      return true;
    }
    throw { code: "INTERNAL_ERROR" };
  });
  await restore_desktop_widgets();
  expect(read_desktop_widgets()).toEqual([{ kind: "focus", id: "a" }]);
  expect(mocks.warn).toHaveBeenCalledOnce();
});

it("does not open desktop widgets on unsupported platforms", async () => {
  await open_desktop_widget({ kind: "focus", id: "a" });
  mocks.invoke.mockClear().mockResolvedValue(false);
  await restore_desktop_widgets();
  expect(mocks.invoke).toHaveBeenCalledExactlyOnceWith(
    "desktop_widget_supported",
  );
  expect(read_desktop_widgets()).toHaveLength(1);
});

it("switches to compact content in narrow or short widgets", () => {
  expect(compact_widget(200, 400)).toBe(true);
  expect(compact_widget(360, 150)).toBe(true);
  expect(compact_widget(300, 260)).toBe(false);
});

it("lists and removes floating windows through the same widget references", async () => {
  const task = { kind: "task" as const, boardId: 3, taskId: 4 };
  const focus = { kind: "focus" as const, id: "today" };
  await open_widget(task, "floating");
  await open_widget(focus, "floating");
  expect(mocks.invoke).toHaveBeenCalledWith("open_floating_task", {
    boardId: 3,
    taskId: 4,
  });
  expect(read_widgets("floating")).toEqual([focus, task]);
  expect(read_widgets("desktop")).toEqual([]);
  await remove_widget(task, "floating");
  expect(mocks.invoke).toHaveBeenLastCalledWith("remove_widget", {
    widget: task,
    placement: "floating",
  });
  expect(read_widgets("floating")).toEqual([focus]);
});
