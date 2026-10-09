import { readFileSync } from "node:fs";
import { parse } from "svelte/compiler";
import { transpile } from "typescript";
import { afterEach, expect, it, vi } from "vite-plus/test";

const source = readFileSync(new URL("./+page.svelte", import.meta.url), "utf8");
const mount = parse(source, { modern: true }).instance!.content.body.find(
  (node) =>
    node.type === "ExpressionStatement" &&
    node.expression.type === "CallExpression" &&
    node.expression.callee.type === "Identifier" &&
    node.expression.callee.name === "onMount",
)!;
// The parser returns offsets that its ESTree types omit.
const { start, end } = mount as typeof mount & { start: number; end: number };
const lifecycle = transpile(source.slice(start, end));

function deferred() {
  let resolve!: (value?: unknown) => void;
  const promise = new Promise((done) => {
    resolve = done;
  });
  return { promise, resolve };
}

function mountPage(
  startup: Promise<unknown>,
  initialization: Promise<unknown> = Promise.resolve(),
) {
  vi.useFakeTimers();
  vi.stubGlobal(
    "window",
    Object.assign(new EventTarget(), {
      setInterval,
      clearInterval,
      setTimeout,
      clearTimeout,
    }),
  );
  vi.stubGlobal("document", new EventTarget());
  vi.stubGlobal("requestAnimationFrame", vi.fn());
  let cleanup!: () => void;
  const scope = {
    onMount: (callback: () => () => void) => {
      cleanup = callback();
    },
    invoke: vi.fn(() => startup),
    board: { init: vi.fn(() => initialization), boards: [] },
    load_app_name: vi.fn(),
    refresh_device_requests: vi.fn(),
    check_for_updates: vi.fn(),
    listen: vi.fn(async () => vi.fn()),
    UPDATE_STARTUP_DELAY_MS: 10_000,
    startup_check_finished: false,
    startup_error: null,
    managed_share_state: { storage_error: null },
    restore_managed_share_state: vi.fn(),
    restore_all_managed_shares: vi.fn(),
  };
  // Exercise the actual component lifecycle without a desktop or user data.
  new Function(...Object.keys(scope), lifecycle)(...Object.values(scope));
  return { cleanup, ...scope };
}

afterEach(() => {
  vi.useRealTimers();
  vi.unstubAllGlobals();
});

it("does not start polling when startup finishes after unmount", async () => {
  const startup = deferred();
  const page = mountPage(startup.promise);
  page.cleanup();
  startup.resolve(null);
  await Promise.resolve();
  expect(page.board.init).not.toHaveBeenCalled();
  expect(page.refresh_device_requests).not.toHaveBeenCalled();
  expect(vi.getTimerCount()).toBe(0);
});

it("does not restore shares when initialization finishes after unmount", async () => {
  const initialization = deferred();
  const page = mountPage(Promise.resolve(null), initialization.promise);
  await Promise.resolve();
  expect(page.board.init).toHaveBeenCalledOnce();
  page.cleanup();
  initialization.resolve();
  await Promise.resolve();
  expect(page.restore_managed_share_state).not.toHaveBeenCalled();
  expect(page.restore_all_managed_shares).not.toHaveBeenCalled();
  expect(vi.getTimerCount()).toBe(0);
});
