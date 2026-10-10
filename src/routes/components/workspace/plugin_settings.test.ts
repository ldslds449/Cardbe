import { readFileSync } from "node:fs";
import { parse } from "svelte/compiler";
import { transpile } from "typescript";
import { expect, it, vi } from "vite-plus/test";
import { PhysicalPosition } from "@tauri-apps/api/dpi";
import { parseCommandError } from "$lib/command-errors";
import { isNewerPluginVersion } from "$lib/plugins";

const source = readFileSync(
  new URL("./plugin_settings.svelte", import.meta.url),
  "utf8",
);

it("shows localized field help only when a description is available", () => {
  const help = source.slice(
    source.indexOf("{#snippet fieldHelp"),
    source.indexOf("{#snippet picker"),
  );
  const condition = help.match(/\{#if ([^}]+)\}/)![1];
  const visible = new Function(
    "field",
    "pluginText",
    `return Boolean(${condition});`,
  );
  const text = (value: unknown) => (typeof value === "string" ? value : "");
  for (const description of [undefined, null, "", "  "]) {
    expect(visible({ description }, text)).toBe(false);
  }
  expect(visible({ description: "Field meaning" }, text)).toBe(true);
  expect(help).toContain('type="button"');
  expect(help).toContain("m.plugin_field_help");
  expect(help).toContain("<Popover.Content");
  expect(help).toContain("{pluginText(field.description ?? null)}");
});

it("keeps the column picker mounted while refreshing existing options", () => {
  const columnField = source.slice(
    source.indexOf('{:else if field.type === "column"}'),
    source.indexOf('{:else if field.type === "select"}'),
  );
  const condition = columnField.match(/\{#if ([^}]+)\}/)![1];
  const showsPlaceholder = new Function(
    "columns",
    "columnsLoading",
    `return ${condition};`,
  );
  expect(showsPlaceholder([{ id: 7, name: "Todo" }], true)).toBe(false);
  expect(showsPlaceholder([], true)).toBe(true);
  expect(showsPlaceholder([], false)).toBe(true);
});

it("keeps new-column enabled during background refresh but guards initial loading", () => {
  const columnField = source.slice(
    source.indexOf('{:else if field.type === "column"}'),
    source.indexOf('{:else if field.type === "select"}'),
  );
  const condition = columnField.match(/disabled=\{([^}]+)\}/)![1];
  const isDisabled = new Function(
    "pending",
    "boardId",
    "columnsLoading",
    "columns",
    `return ${condition};`,
  );
  const columns = [{ id: 7, name: "Todo" }];
  expect(isDisabled(false, "1", true, columns)).toBe(false);
  expect(isDisabled(false, "1", false, columns)).toBe(false);
  expect(isDisabled(false, "1", true, [])).toBe(true);
  expect(isDisabled(false, "1", false, [])).toBe(false);
  expect(isDisabled(false, "", false, [])).toBe(true);
  expect(isDisabled(true, "1", false, columns)).toBe(true);
});

function setupHistory(invoke: ReturnType<typeof vi.fn>) {
  const functions = parse(source, { modern: true })
    .instance!.content.body.filter(
      (node) =>
        node.type === "FunctionDeclaration" &&
        ["refresh", "action", "showHistory"].includes(node.id!.name),
    )
    .map((node) => {
      const { start, end } = node as typeof node & {
        start: number;
        end: number;
      };
      return source.slice(start, end);
    })
    .join("\n");
  return new Function(
    "invoke",
    "parseCommandError",
    transpile(`
      let session = 0, historyId = null, runs = [{ id: "previous" }],
        editing = {}, secrets = { token: "draft" }, pending = false,
        error = null, refreshRequest = null, pluginState = null, loaded = false;
      ${functions}
      return { refresh, showHistory,
        close: () => { session++; historyId = null; runs = []; },
        state: () => ({ historyId, runs, error, pending, editing, secrets }) };
    `),
  )(invoke, parseCommandError);
}

it("opens history and fetches the selected instance after an in-flight poll", async () => {
  let resolve!: (value: unknown) => void;
  const invoke = vi.fn().mockReturnValueOnce(
    new Promise((done) => {
      resolve = done;
    }),
  );
  invoke.mockImplementation((command) =>
    Promise.resolve(command === "get_plugin_runs" ? [{ id: "new" }] : {}),
  );
  const settings = setupHistory(invoke);
  const poll = settings.refresh();
  const opening = settings.showHistory("instance-2");
  expect(settings.state()).toMatchObject({
    historyId: "instance-2",
    runs: [],
    pending: true,
    editing: null,
    secrets: {},
  });
  resolve({});
  await poll;
  await opening;
  expect(invoke).toHaveBeenCalledWith("get_plugin_runs", {
    instanceId: "instance-2",
  });
  expect(settings.state().runs).toEqual([{ id: "new" }]);
  expect(settings.state().pending).toBe(false);
});

it("reports history failures safely and ignores results after closing", async () => {
  const failed = setupHistory(
    vi.fn().mockRejectedValue({ code: "INTERNAL_ERROR" }),
  );
  await failed.showHistory("instance-1");
  expect(failed.state().error).toEqual({ code: "INTERNAL_ERROR" });
  expect(failed.state().pending).toBe(false);

  let resolve!: (value: unknown) => void;
  const invoke = vi.fn().mockImplementation((command) =>
    command === "get_plugin_runs"
      ? new Promise((done) => {
          resolve = done;
        })
      : Promise.resolve({}),
  );
  const settings = setupHistory(invoke);
  const opening = settings.showHistory("instance-1");
  await vi.waitFor(() => expect(resolve).toBeTypeOf("function"));
  settings.close();
  resolve([{ id: "late" }]);
  await opening;
  expect(settings.state().runs).toEqual([]);
});

it("uses inline validation instead of browser form popups", () => {
  const forms = source.match(/<form\b[^>]*>/g)!;
  expect(forms).toHaveLength(4);
  expect(forms.every((form) => /\bnovalidate\b/.test(form))).toBe(true);
});

it.each([
  [" ", true],
  ["Instance", false],
])(
  "marks missing name or consent invalid without submitting (%s, %s)",
  async (name, consent) => {
    const node = parse(source, { modern: true }).instance!.content.body.find(
      (node) => node.type === "FunctionDeclaration" && node.id!.name === "save",
    )!;
    const { start, end } = node as typeof node & { start: number; end: number };
    const action = vi.fn();
    const focusEditorError = vi.fn();
    const settings = new Function(
      "action",
      "invalidPluginSettings",
      "name",
      "consent",
      "focusEditorError",
      transpile(`
    let editing={settings:[]},boardId="7",invalidInterval=false,config={},secrets={},savedSecrets=[],invalid=[],saveAttempted=false,error=null;
    ${source.slice(start, end)}
    return {save,state:()=>({saveAttempted,error})};
  `),
    )(action, () => [], name, consent, focusEditorError);
    await settings.save();
    expect(settings.state()).toEqual({
      saveAttempted: true,
      error: null,
    });
    expect(action).not.toHaveBeenCalled();
    expect(focusEditorError).toHaveBeenCalledOnce();
  },
);

it("waits for inline errors to render, then focuses and scrolls to the first error", async () => {
  const node = parse(source, { modern: true }).instance!.content.body.find(
    (node) =>
      node.type === "FunctionDeclaration" &&
      node.id!.name === "focusEditorError",
  )!;
  const { start, end } = node as typeof node & { start: number; end: number };
  const target = { focus: vi.fn(), scrollIntoView: vi.fn() };
  const editorForm = { querySelector: vi.fn().mockReturnValue(target) };
  const tick = vi.fn().mockResolvedValue(undefined);
  const focus = new Function(
    "tick",
    "editorForm",
    transpile(`${source.slice(start, end)}; return focusEditorError;`),
  )(tick, editorForm);
  await focus();
  expect(tick).toHaveBeenCalledOnce();
  expect(tick.mock.invocationCallOrder[0]).toBeLessThan(
    editorForm.querySelector.mock.invocationCallOrder[0],
  );
  expect(target.focus).toHaveBeenCalledWith({ preventScroll: true });
  expect(target.scrollIntoView).toHaveBeenCalledWith({
    block: "center",
    inline: "nearest",
  });
});

function setupCreateColumn(invoke: ReturnType<typeof vi.fn>, boardId = "12") {
  const node = parse(source, { modern: true }).instance!.content.body.find(
    (node) =>
      node.type === "FunctionDeclaration" && node.id!.name === "createColumn",
  )!;
  const { start, end } = node as typeof node & { start: number; end: number };
  const board = {
    active_board_id: 7,
    get_boards: vi.fn(),
    get_columns: vi.fn(),
  };
  return new Function(
    "invoke",
    "parseCommandError",
    "board",
    "boardId",
    transpile(`
    let session=0,pending=false,editing={},newColumnName=" Todo ",newColumnField="column",creatingColumn=false,newColumnError=null,columns=[],config={};
    ${source.slice(start, end)}
    return {createColumn,board,state:()=>({columns,config,newColumnField,newColumnError,creatingColumn})};
  `),
  )(invoke, parseCommandError, board, boardId);
}

it("creates and selects a column without switching the workspace", async () => {
  const invoke = vi.fn().mockResolvedValue(3);
  const settings = setupCreateColumn(invoke);
  await settings.createColumn();
  expect(invoke).toHaveBeenCalledWith("add_column_to_board", {
    boardId: 12,
    name: "Todo",
  });
  expect(settings.state()).toEqual({
    columns: [{ id: 3, name: "Todo" }],
    config: { column: 3 },
    newColumnField: null,
    newColumnError: null,
    creatingColumn: false,
  });
  expect(settings.board.get_columns).not.toHaveBeenCalled();
});

it("keeps column creation open on failure and requires a board", async () => {
  const invoke = vi.fn().mockRejectedValue({ code: "PERMISSION_DENIED" });
  const settings = setupCreateColumn(invoke);
  await settings.createColumn();
  expect(settings.state().newColumnError).toEqual({
    code: "PERMISSION_DENIED",
  });
  expect(settings.state().newColumnField).toBe("column");
  invoke.mockClear();
  await setupCreateColumn(invoke, "").createColumn();
  expect(invoke).not.toHaveBeenCalled();
});
const functions = parse(source, { modern: true })
  .instance!.content.body.filter(
    (node) =>
      node.type === "FunctionDeclaration" &&
      ["chooseBoard", "refreshColumns"].includes(node.id!.name),
  )
  .map((node) => {
    const { start, end } = node as typeof node & { start: number; end: number };
    return source.slice(start, end);
  })
  .join("\n");

const installPackageNode = parse(source, {
  modern: true,
}).instance!.content.body.find(
  (node) =>
    node.type === "FunctionDeclaration" && node.id!.name === "installPackage",
)!;
const installPackageRange = installPackageNode as typeof installPackageNode & {
  start: number;
  end: number;
};
const installPackageSource = source.slice(
  installPackageRange.start,
  installPackageRange.end,
);

function setupInstall(invoke: ReturnType<typeof vi.fn>) {
  const updateFunctions = parse(source, { modern: true })
    .instance!.content.body.filter(
      (node) =>
        node.type === "FunctionDeclaration" &&
        ["showUpdate", "confirmUpdate", "closeUpdate"].includes(node.id!.name),
    )
    .map((node) => {
      const { start, end } = node as typeof node & {
        start: number;
        end: number;
      };
      return source.slice(start, end);
    })
    .join("\n");
  return new Function(
    "invoke",
    "parseCommandError",
    "isNewerPluginVersion",
    transpile(`
    let session = 0, installation = null, open=true, error=null,
      updateTarget=null,updatePreview=null,updateSuccess=false;
    async function action(work){try{await work();}catch(cause){error=parseCommandError(cause)??{code:"INTERNAL_ERROR"};}}
    ${updateFunctions}
    ${installPackageSource}
    return { installPackage, confirmUpdate, closeUpdate, state: () => installation,
      updateState:()=>({updatePreview,updateSuccess,error}),
      close: () => { session++; open=false; installation = null; closeUpdate(); } };
  `),
  )(invoke, parseCommandError, isNewerPluginVersion);
}

it("shows the selected filename while installing and preserves the successful package", async () => {
  let resolve!: (pkg: unknown) => void;
  const invoke = vi.fn().mockImplementation(
    () =>
      new Promise((done) => {
        resolve = done;
      }),
  );
  const settings = setupInstall(invoke);
  const pending = settings.installPackage({ path: "C:\\plugins\\release.zip" });
  expect(settings.state()).toEqual({
    status: "installing",
    source: "release.zip",
  });
  const pkg = { id: "sample", name: { en: "Sample", "zh-TW": "範例" } };
  resolve({ kind: "installed", package: pkg });
  await pending;
  expect(settings.state()).toEqual({
    status: "success",
    source: "release.zip",
    package: pkg,
  });
});

it("keeps a structured failure beside installation feedback", async () => {
  const failure = { code: "INVALID_ARGUMENT" };
  const settings = setupInstall(vi.fn().mockRejectedValue(failure));
  await expect(
    settings.installPackage({ url: "https://example.com/package.zip" }),
  ).resolves.toBeUndefined();
  expect(settings.state()).toEqual({
    status: "failed",
    source: null,
    error: failure,
  });
});

it("uses a safe error for unknown installation failures", async () => {
  const settings = setupInstall(
    vi.fn().mockRejectedValue("private runtime details"),
  );
  await settings.installPackage({ path: "/plugins/manifest.json" });
  expect(settings.state()).toEqual({
    status: "failed",
    source: "manifest.json",
    error: { code: "INTERNAL_ERROR" },
  });
});

it("does not restore installation feedback after the dialog has closed", async () => {
  for (const failed of [false, true]) {
    let settle!: (value: unknown) => void;
    const invoke = vi.fn().mockImplementation(
      () =>
        new Promise((resolve, reject) => {
          settle = failed ? reject : resolve;
        }),
    );
    const settings = setupInstall(invoke);
    const pending = settings
      .installPackage({ path: "manifest.json" })
      .catch(() => undefined);
    settings.close();
    settle(
      failed
        ? { code: "INVALID_ARGUMENT" }
        : { kind: "installed", package: { id: "sample", name: "Sample" } },
    );
    await pending;
    expect(settings.state()).toBeNull();
  }
});

it("offers ZIP and manifest files and installs the selected local package", async () => {
  const node = parse(source, { modern: true }).instance!.content.body.find(
    (node) =>
      node.type === "FunctionDeclaration" && node.id!.name === "install",
  )!;
  const { start, end } = node as typeof node & { start: number; end: number };
  const openFile = vi.fn().mockResolvedValue("release.zip");
  const invoke = vi.fn().mockResolvedValue({
    kind: "installed",
    package: { id: "sample", name: "Sample" },
  });
  const install = new Function(
    "openFile",
    "installPackage",
    "action",
    "m",
    transpile(`${source.slice(start, end)}; return install;`),
  )(
    openFile,
    setupInstall(invoke).installPackage,
    (work: () => Promise<unknown>) => work(),
    {
      plugin_package_file: () => "Plugin package",
    },
  );
  await install();
  expect(openFile).toHaveBeenCalledWith({
    multiple: false,
    filters: [{ name: "Plugin package", extensions: ["zip", "json"] }],
  });
  expect(invoke).toHaveBeenCalledWith("install_plugin_package", {
    path: "release.zip",
  });
  invoke.mockClear();
  openFile.mockResolvedValue(null);
  await install();
  expect(invoke).not.toHaveBeenCalled();
});

function setupDrop(options: Record<string, unknown> = {}) {
  const node = parse(source, { modern: true }).instance!.content.body.find(
    (node) =>
      node.type === "FunctionDeclaration" &&
      node.id!.name === "handlePluginDrop",
  )!;
  const { start, end } = node as typeof node & { start: number; end: number };
  const bounds = { left: 10, right: 200, top: 20, bottom: 160 };
  const invoke = vi.fn().mockResolvedValue(
    options.response ?? {
      kind: "installed",
      package: { id: "sample", name: "Sample" },
    },
  );
  const settings = new Function(
    "options",
    "invoke",
    "action",
    "window",
    "parseCommandError",
    transpile(`
    let open = options.open ?? true, pending = options.pending ?? false, editing = options.editing ?? null, updateTarget=options.updateTarget??null;
    async function showUpdate(target){updateTarget=target;}
    let dropZone = options.hidden ? null : {
      getBoundingClientRect: () => options.bounds,
      closest: () => ({ getBoundingClientRect: () => options.viewport ?? options.bounds })
    };
    let dragging = false, error = null, installation = null, session = 0;
    ${installPackageSource}
    ${source.slice(start, end)}
    return { handlePluginDrop, updateTarget:()=>updateTarget, state: () => ({ dragging, error, installation }) };
  `),
  )(
    { bounds, ...options },
    invoke,
    (work: () => Promise<unknown>) => work(),
    {
      devicePixelRatio: 2,
    },
    parseCommandError,
  );
  return { ...settings, invoke };
}

it("highlights native file drags and installs one ZIP or manifest using scaled coordinates", async () => {
  for (const path of [
    "C:\\plugins\\release.ZIP",
    "C:\\plugins\\manifest.json",
    "/plugins/manifest.json",
  ]) {
    const settings = setupDrop();
    const position = new PhysicalPosition(100, 100);
    await settings.handlePluginDrop({ type: "enter", paths: [path], position });
    expect(settings.state().dragging).toBe(true);
    expect(settings.invoke).not.toHaveBeenCalled();
    await settings.handlePluginDrop({ type: "drop", paths: [path], position });
    expect(settings.state().dragging).toBe(false);
    expect(settings.state().installation.status).toBe("success");
    expect(settings.invoke).toHaveBeenCalledWith("install_plugin_package", {
      path,
    });
  }
});

it("ignores drops outside the visible local panel or while installation is unavailable", async () => {
  for (const options of [
    { open: false },
    { pending: true },
    { editing: {} },
    { updateTarget: {} },
    { hidden: true },
    { viewport: { left: 10, right: 200, top: 80, bottom: 160 } },
  ]) {
    const settings = setupDrop(options);
    await settings.handlePluginDrop({
      type: "drop",
      paths: ["release.zip"],
      position: new PhysicalPosition(100, 100),
    });
    expect(settings.invoke).not.toHaveBeenCalled();
  }
  const settings = setupDrop();
  await settings.handlePluginDrop({
    type: "over",
    position: new PhysicalPosition(1000, 1000),
  });
  expect(settings.state().dragging).toBe(false);
  await settings.handlePluginDrop({ type: "leave" });
  expect(settings.state().dragging).toBe(false);
});

it("keeps hover feedback steady without installing or refreshing during drag movement", async () => {
  const settings = setupDrop();
  for (let move = 0; move < 3; move++) {
    await settings.handlePluginDrop({
      type: "over",
      position: new PhysicalPosition(100 + move, 100),
    });
    expect(settings.state().dragging).toBe(true);
    expect(settings.state().installation).toBeNull();
    expect(settings.invoke).not.toHaveBeenCalled();
  }
});

it("prevents browser file navigation only while the plugin dialog is open", () => {
  const node = parse(source, { modern: true }).instance!.content.body.find(
    (node) =>
      node.type === "FunctionDeclaration" &&
      node.id!.name === "preventFileNavigation",
  )!;
  const { start, end } = node as typeof node & { start: number; end: number };
  for (const open of [true, false]) {
    const handler = new Function(
      "open",
      transpile(`${source.slice(start, end)}; return preventFileNavigation;`),
    )(open);
    for (const types of [["Files"], ["text/plain"]]) {
      const preventDefault = vi.fn();
      handler({ dataTransfer: { types }, preventDefault });
      expect(preventDefault).toHaveBeenCalledTimes(
        open && types.includes("Files") ? 1 : 0,
      );
    }
  }
});

it("rejects multiple files and unsupported dropped files", async () => {
  for (const paths of [
    [],
    ["release.zip", "other.zip"],
    ["settings.json"],
    ["component.wasm"],
  ]) {
    const settings = setupDrop();
    await settings.handlePluginDrop({
      type: "drop",
      paths,
      position: new PhysicalPosition(100, 100),
    });
    expect(settings.invoke).not.toHaveBeenCalled();
    expect(settings.state().installation).toEqual({
      status: "failed",
      source: null,
      error: { code: "INVALID_ARGUMENT" },
    });
  }
});

function setup(invoke: ReturnType<typeof vi.fn>) {
  return new Function(
    "invoke",
    "parseCommandError",
    transpile(`
      let boardId = "1", columnRequest = 0, columns = [], columnsLoading = false, error = null;
      const editing = { settings: [{ key: "column", type: "column" }] };
      const config = { column: 7 };
      ${functions}
      return { chooseBoard, refreshColumns,
        state: () => ({ columns, columnsLoading, config, error }) };
    `),
  )(invoke, (error: unknown) => error);
}

it("fetches newly added columns without clearing the selected column", async () => {
  const invoke = vi.fn().mockResolvedValueOnce([]);
  const settings = setup(invoke);
  await settings.refreshColumns();
  expect(settings.state().columns).toEqual([]);
  invoke.mockResolvedValueOnce([{ id: 7, name: "New column" }]);
  await settings.refreshColumns();
  expect(invoke).toHaveBeenLastCalledWith("get_board_columns", { boardId: 1 });
  expect(settings.state().columns).toEqual([{ id: 7, name: "New column" }]);
  expect(settings.state().config.column).toBe(7);
});

it("ignores a stale refresh after switching boards", async () => {
  let resolve!: (columns: unknown[]) => void;
  const invoke = vi
    .fn()
    .mockReturnValueOnce(new Promise((done) => (resolve = done)))
    .mockResolvedValueOnce([{ id: 8, name: "Other board" }]);
  const settings = setup(invoke);
  const stale = settings.refreshColumns();
  await settings.chooseBoard("2");
  resolve([{ id: 7, name: "Old board" }]);
  await stale;
  expect(settings.state().columns).toEqual([{ id: 8, name: "Other board" }]);
  expect(settings.state().config.column).toBeUndefined();
});

it("keeps column loading active until the newest board request finishes", async () => {
  const resolve: ((columns: unknown[]) => void)[] = [];
  const invoke = vi
    .fn()
    .mockImplementation(() => new Promise((done) => resolve.push(done)));
  const settings = setup(invoke);
  const old = settings.refreshColumns();
  const current = settings.chooseBoard("2");
  expect(settings.state().columnsLoading).toBe(true);
  resolve[0]([{ id: 7, name: "Old" }]);
  await old;
  expect(settings.state().columnsLoading).toBe(true);
  expect(settings.state().columns).toEqual([]);
  resolve[1]([]);
  await current;
  expect(settings.state().columnsLoading).toBe(false);
  await settings.chooseBoard("");
  expect(settings.state().columnsLoading).toBe(false);
});

it("keeps existing options and reports structured refresh failures", async () => {
  const invoke = vi.fn().mockResolvedValueOnce([{ id: 7, name: "Column" }]);
  const settings = setup(invoke);
  await settings.refreshColumns();
  invoke.mockRejectedValueOnce({ code: "BOARD_NOT_FOUND" });
  await settings.refreshColumns();
  expect(settings.state().columns).toEqual([{ id: 7, name: "Column" }]);
  expect(settings.state().error).toEqual({ code: "BOARD_NOT_FOUND" });
  expect(settings.state().columnsLoading).toBe(false);
});

function setupCreateBoard(
  invoke: ReturnType<typeof vi.fn>,
  newBoardName = " New board ",
) {
  const node = parse(source, { modern: true }).instance!.content.body.find(
    (node) =>
      node.type === "FunctionDeclaration" && node.id!.name === "createBoard",
  )!;
  const { start, end } = node as typeof node & { start: number; end: number };
  const board = { boards: [{ id: 7, name: "Existing" }], active_board_id: 7 };
  const chooseBoard = vi.fn().mockResolvedValue(undefined);
  const settings = new Function(
    "invoke",
    "parseCommandError",
    "board",
    "chooseBoard",
    "newBoardName",
    transpile(`
    let session = 0, creatingBoard = false, pending = false, editing = {}, consent = true;
    let newBoardOpen = true, newBoardError = null;
    ${source.slice(start, end)}
    return { createBoard, state: () => ({ creatingBoard, consent, newBoardOpen, newBoardError }) };
  `),
  )(invoke, parseCommandError, board, chooseBoard, newBoardName);
  return { ...settings, board, chooseBoard };
}

it("creates and selects a target board without switching the workspace or keeping old consent", async () => {
  const created = { id: 12, name: "New board" };
  const invoke = vi.fn().mockResolvedValue(created);
  const settings = setupCreateBoard(invoke);
  await settings.createBoard();
  expect(invoke).toHaveBeenCalledWith("create_board", { name: "New board" });
  expect(settings.board.boards).toEqual([{ id: 7, name: "Existing" }, created]);
  expect(settings.board.active_board_id).toBe(7);
  expect(settings.chooseBoard).toHaveBeenCalledWith("12");
  expect(settings.state()).toEqual({
    creatingBoard: false,
    consent: false,
    newBoardOpen: false,
    newBoardError: null,
  });
});

it("keeps board creation open with a safe error and rejects blank names", async () => {
  const invoke = vi.fn().mockRejectedValue({ code: "BOARD_NAME_REQUIRED" });
  const settings = setupCreateBoard(invoke);
  await settings.createBoard();
  expect(settings.state().newBoardOpen).toBe(true);
  expect(settings.state().newBoardError).toEqual({
    code: "BOARD_NAME_REQUIRED",
  });
  expect(settings.state().creatingBoard).toBe(false);
  expect(settings.chooseBoard).not.toHaveBeenCalled();
  invoke.mockClear();
  await setupCreateBoard(invoke, " ").createBoard();
  expect(invoke).not.toHaveBeenCalled();
});

it("shows removal confirmation outside the scrolling plugin list and retains failures", () => {
  const confirmation = source.slice(
    source.indexOf("  <AlertDialog.Root"),
    source.indexOf("  </AlertDialog.Root>"),
  );
  expect(source.indexOf("  <AlertDialog.Root")).toBeGreaterThan(
    source.indexOf("    </ScrollArea>"),
  );
  expect(confirmation).toContain("open={removal !== null}");
  expect(confirmation).toContain("!value && !pending");
  expect(confirmation).toContain("pluginErrorMessage(error)");
  expect(confirmation).toContain("onclick={remove}");
  expect(confirmation).not.toContain("AlertDialog.Action");
});

const updatePreview = {
  token: "review-token",
  current_version: "1.0.0",
  requires_review: false,
  package: {
    id: "sample",
    version: "1.1.0",
    api_version: "1.0.0",
    storage_schema_version: 1,
  },
};
const updateSelection = {
  kind: "update",
  current_package: { ...updatePreview.package, version: "1.0.0" },
  preview: updatePreview,
};
it("routes dropped update packages through the same entry without applying them", async () => {
  const settings = setupDrop({ response: updateSelection });
  await settings.handlePluginDrop({
    type: "drop",
    paths: ["new.zip"],
    position: new PhysicalPosition(100, 100),
  });
  expect(settings.invoke).toHaveBeenCalledExactlyOnceWith(
    "install_plugin_package",
    { path: "new.zip" },
  );
  expect(settings.updateTarget()).toEqual(updateSelection.current_package);
  expect(settings.state().installation).toBeNull();
});
it.each([{ path: "new.zip" }, { url: "https://example.com/new.zip" }])(
  "uses the installation entry to preview updates without applying them (%s)",
  async (input) => {
    const invoke = vi.fn().mockResolvedValue(updateSelection),
      settings = setupInstall(invoke);
    await settings.installPackage(input);
    expect(invoke).toHaveBeenCalledExactlyOnceWith(
      "install_plugin_package",
      input,
    );
    expect(settings.state()).toBeNull();
    expect(settings.updateState().updatePreview).toEqual(updatePreview);
    await settings.confirmUpdate();
    expect(invoke).toHaveBeenLastCalledWith("confirm_plugin_update", {
      token: "review-token",
    });
    expect(settings.updateState()).toEqual({
      updatePreview: null,
      updateSuccess: true,
      error: null,
    });
  },
);
it("preserves update preview for retry after a structured apply failure", async () => {
  const invoke = vi
    .fn()
    .mockResolvedValueOnce(updateSelection)
    .mockRejectedValueOnce({ code: "PLUGIN_UPDATE_BUSY" });
  const settings = setupInstall(invoke);
  await settings.installPackage({ path: "new.zip" });
  await settings.confirmUpdate();
  expect(settings.updateState()).toEqual({
    updatePreview,
    updateSuccess: false,
    error: { code: "PLUGIN_UPDATE_BUSY" },
  });
});
it("discards a late update preview after the settings dialog closes", async () => {
  let resolve!: (value: unknown) => void;
  const invoke = vi
    .fn()
    .mockReturnValueOnce(
      new Promise((done) => {
        resolve = done;
      }),
    )
    .mockResolvedValue(undefined);
  const settings = setupInstall(invoke),
    loading = settings.installPackage({ path: "new.zip" });
  settings.close();
  resolve(updateSelection);
  await loading;
  expect(settings.updateState().updatePreview).toBeNull();
  expect(invoke).toHaveBeenLastCalledWith("discard_plugin_update", {
    token: "review-token",
  });
});
it("rejects incompatible or non-newer automatic update previews", async () => {
  for (const change of [
    { id: "other" },
    { api_version: "2.0.0" },
    { storage_schema_version: 2 },
    { version: "0.9.0" },
  ]) {
    const invoke = vi.fn().mockResolvedValue({
      ...updateSelection,
      preview: {
        ...updatePreview,
        package: { ...updatePreview.package, ...change },
      },
    });
    const settings = setupInstall(invoke);
    await settings.installPackage({ path: "new.zip" });
    expect(settings.state().error).toEqual({ code: "INVALID_ARGUMENT" });
    expect(settings.updateState().updatePreview).toBeNull();
    expect(invoke).toHaveBeenLastCalledWith("discard_plugin_update", {
      token: "review-token",
    });
  }
});
it("rejects stale automatic update previews", async () => {
  const invoke = vi.fn().mockResolvedValue({
    ...updateSelection,
    preview: { ...updatePreview, current_version: "1.0.1" },
  });
  const settings = setupInstall(invoke);
  await settings.installPackage({ path: "new.zip" });
  expect(settings.state().error).toEqual({ code: "PLUGIN_UPDATE_STALE" });
  expect(invoke).toHaveBeenLastCalledWith("discard_plugin_update", {
    token: "review-token",
  });
});
it("discards cancelled updates without applying them", async () => {
  const invoke = vi.fn().mockResolvedValue(updateSelection),
    settings = setupInstall(invoke);
  await settings.installPackage({ path: "new.zip" });
  settings.closeUpdate();
  expect(settings.updateState().updatePreview).toBeNull();
  expect(invoke).toHaveBeenLastCalledWith("discard_plugin_update", {
    token: "review-token",
  });
  expect(
    invoke.mock.calls.some(([command]) => command === "confirm_plugin_update"),
  ).toBe(false);
});
it("opens updated settings with preserved values, no new grants, and credentials requiring review", () => {
  const node = parse(source, { modern: true }).instance!.content.body.find(
    (node) => node.type === "FunctionDeclaration" && node.id!.name === "edit",
  )!;
  const { start, end } = node as typeof node & { start: number; end: number };
  const edit = new Function(
    "pluginText",
    "pluginDefaults",
    transpile(`
    let editing=null,instanceId,name,config,secrets,savedSecrets,domains,customDomains,
      enabled,interval,consent,invalid,saveAttempted,historyId,error;
    async function chooseBoard(){}
    ${source.slice(start, end)}
    return (pkg,instance)=>{edit(pkg,instance);return {config,savedSecrets,domains,enabled};};
  `),
  )(
    () => "Plugin",
    () => ({}),
  );
  const pkg = {
    id: "sample",
    domains: ["new.example.com"],
    settings: [
      { key: "title", type: "text" },
      { key: "token", type: "secret" },
    ],
  };
  const instance = {
    id: "one",
    name: "Instance",
    board_id: 1,
    config: { title: "Keep", removed: "Retired" },
    secret_fields: ["token"],
    allowed_domains: ["old.example.com"],
    enabled: false,
    credentials_need_review: true,
  };
  expect(edit(pkg, instance)).toEqual({
    config: { title: "Keep" },
    savedSecrets: [],
    domains: [],
    enabled: false,
  });
  expect(
    edit(
      { ...pkg, allow_custom_domains: true },
      { ...instance, credentials_need_review: false },
    ),
  ).toEqual({
    config: { title: "Keep" },
    savedSecrets: ["token"],
    domains: ["old.example.com"],
    enabled: false,
  });
});
