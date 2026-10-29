import { afterEach, beforeEach, expect, it, vi } from "vite-plus/test";
import { PhysicalPosition } from "@tauri-apps/api/dpi";
import type { Window } from "@tauri-apps/api/window";
import { persist_floating_window_position } from "./floating-window-state";

const mocks = vi.hoisted(() => ({ invoke: vi.fn(), warn: vi.fn() }));
vi.mock("@tauri-apps/api/core", () => ({ invoke: mocks.invoke }));
vi.mock("#lib/logger.js", () => ({ logger: { warn: mocks.warn } }));

beforeEach(() => {
  vi.useFakeTimers();
  vi.clearAllMocks();
  mocks.invoke.mockResolvedValue(undefined);
});
afterEach(() => vi.useRealTimers());

it("saves after movement settles, reports failures and cleans up", async () => {
  let moved: Parameters<Window["onMoved"]>[0] = () => {};
  const unlisten = vi.fn();
  const stop = await persist_floating_window_position({
    onMoved: vi.fn(async (handler: typeof moved) => {
      moved = handler;
      return unlisten;
    }),
  });
  const event = {
    event: "tauri://move",
    id: 1,
    payload: new PhysicalPosition(50, 70),
  };
  moved(event);
  await vi.advanceTimersByTimeAsync(200);
  moved(event);
  await vi.advanceTimersByTimeAsync(249);
  expect(mocks.invoke).not.toHaveBeenCalled();
  await vi.advanceTimersByTimeAsync(1);
  expect(mocks.invoke).toHaveBeenCalledExactlyOnceWith(
    "plugin:window-state|save_window_state",
    { flags: 0 },
  );
  mocks.invoke.mockRejectedValueOnce(new Error("unavailable"));
  moved(event);
  await vi.advanceTimersByTimeAsync(250);
  expect(mocks.warn).toHaveBeenCalledOnce();
  moved(event);
  stop();
  await vi.advanceTimersByTimeAsync(250);
  expect(mocks.invoke).toHaveBeenCalledTimes(2);
  expect(unlisten).toHaveBeenCalledOnce();
});
