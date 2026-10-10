import { readFileSync } from "node:fs";
import { parse } from "svelte/compiler";
import { transpile } from "typescript";
import { expect, it, vi } from "vite-plus/test";
import { parseCommandError } from "$lib/command-errors";

const source = readFileSync(
  new URL("./plugin_permissions.svelte", import.meta.url),
  "utf8",
);
const functions = parse(source, { modern: true })
  .instance!.content.body.filter((node) => node.type === "FunctionDeclaration")
  .map((node) => {
    const { start, end } = node as typeof node & { start: number; end: number };
    return source.slice(start, end);
  })
  .join("\n");

function setup(invoke: ReturnType<typeof vi.fn>) {
  return new Function(
    "invoke",
    "parseCommandError",
    transpile(`
    let instances=[],safeMode=false,selected=null,open=false,busy=false,error=null,generation=0;
    const shown=new Set();
    ${functions}
    return { refresh,show,resolve,close:()=>{open=false;},state:()=>({instances,safeMode,selected,open,busy,error}) };
  `),
  )(invoke, parseCommandError);
}

function instance(trigger = "manual", run = "run-1") {
  return {
    id: "one",
    name: "Example",
    enabled: true,
    needs_review: false,
    allowed_domains: [],
    pending_domain: { domain: "api.example.com", run_id: run, trigger },
  };
}

it("prompts manual runs once and keeps background and dismissed requests reviewable", async () => {
  let next = instance("schedule");
  const invoke = vi.fn(async () => ({ safe_mode: false, instances: [next] }));
  const ui = setup(invoke);
  await ui.refresh();
  expect(ui.state().open).toBe(false);
  expect(ui.state().instances).toHaveLength(1);
  next = instance();
  await ui.refresh();
  expect(ui.state().open).toBe(true);
  ui.close();
  await ui.refresh();
  expect(ui.state().open).toBe(false);
  ui.show(ui.state().instances[0]);
  expect(ui.state().open).toBe(true);
  expect(source).toContain("plugin_pending_permissions");
});

it("sends only the instance and pending run identity, and waits for backend approval", async () => {
  const request = instance();
  let instances = [request];
  const invoke = vi.fn(async (command: string) => {
    if (command === "resolve_plugin_domain") {
      instances = [];
      return;
    }
    return { safe_mode: false, instances };
  });
  const ui = setup(invoke);
  await ui.refresh();
  await ui.resolve(true);
  expect(invoke).toHaveBeenCalledWith("resolve_plugin_domain", {
    instanceId: "one",
    runId: "run-1",
    allow: true,
  });
  expect(request.allowed_domains).toEqual([]);
  expect(ui.state().open).toBe(false);
  expect(ui.state().busy).toBe(false);
});

it("keeps failed decisions visible and translates structured errors", async () => {
  const invoke = vi.fn(async (command: string) => {
    if (command === "resolve_plugin_domain") {
      throw { code: "PERMISSION_DENIED" };
    }
    return { safe_mode: false, instances: [instance()] };
  });
  const ui = setup(invoke);
  await ui.refresh();
  await ui.resolve(false);
  expect(invoke).toHaveBeenCalledWith("resolve_plugin_domain", {
    instanceId: "one",
    runId: "run-1",
    allow: false,
  });
  expect(ui.state().error).toEqual({ code: "PERMISSION_DENIED" });
  expect(ui.state().open).toBe(true);
  expect(ui.state().busy).toBe(false);
});

it("ignores stale refreshes and removes requests revoked in another window", async () => {
  let finish: (value: unknown) => void = () => {};
  const invoke = vi
    .fn()
    .mockImplementationOnce(
      () =>
        new Promise((resolve) => {
          finish = resolve;
        }),
    )
    .mockResolvedValue({
      safe_mode: false,
      instances: [instance("manual", "new-run")],
    });
  const ui = setup(invoke);
  const older = ui.refresh();
  await ui.refresh();
  finish({ safe_mode: false, instances: [instance("manual", "old-run")] });
  await older;
  expect(ui.state().selected.pending_domain.run_id).toBe("new-run");
  invoke.mockResolvedValue({ safe_mode: false, instances: [] });
  await ui.refresh();
  expect(ui.state().open).toBe(false);
  expect(ui.state().selected).toBe(null);
});

it("does not prompt disabled or review-required instances or in safe mode", async () => {
  const invoke = vi.fn().mockResolvedValue({
    safe_mode: true,
    instances: [
      instance(),
      { ...instance(), id: "disabled", enabled: false },
      { ...instance(), id: "review", needs_review: true },
    ],
  });
  const ui = setup(invoke);
  await ui.refresh();
  expect(ui.state().open).toBe(false);
  expect(ui.state().instances).toHaveLength(1);
});

it("removes duplicate host entry and keeps permission review and revocation in settings", () => {
  const settings = readFileSync(
    new URL("./plugin_settings.svelte", import.meta.url),
    "utf8",
  );
  expect(settings).not.toContain('id="plugin-host-url"');
  expect(settings).toContain("onReviewDomain?.(instance.id)");
  expect(settings).toContain("domains.filter((value) => value !== domain)");
  const checkbox = settings.slice(
    settings.indexOf("{#each availableDomains as domain"),
  );
  const disabled = new Function(
    "pending",
    "domains",
    "domain",
    `return ${checkbox.match(/\sdisabled=\{([^}]+)\}/)![1]};`,
  );
  const hosts = Array.from({ length: 16 }, (_, i) => `host${i}.example.com`);
  expect(disabled(false, hosts, "new.example.com")).toBe(true);
  expect(disabled(false, hosts, hosts[0])).toBe(false);
  expect(disabled(false, hosts.slice(1), "new.example.com")).toBe(false);
  expect(disabled(true, hosts, hosts[0])).toBe(true);
});
