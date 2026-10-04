import { spawn } from "node:child_process";
import { once } from "node:events";
import {
  mkdtempSync,
  mkdirSync,
  copyFileSync,
  writeFileSync,
  readFileSync,
  rmSync,
  existsSync,
  renameSync,
} from "node:fs";
import { createServer } from "node:net";
import { tmpdir } from "node:os";
import path from "node:path";
import { selectInstance, selectPort, listen, close } from "./dev-instance.mjs";
import { expect, it } from "vite-plus/test";

it("automatically changes ports while preserving a migrated random instance ID", async () => {
  const root = mkdtempSync(path.join(tmpdir(), "cardbe-port-test-"));
  const server = createServer();
  const nextServer = createServer();
  try {
    server.listen(0, "127.0.0.1");
    await once(server, "listening");
    const address = server.address();
    if (!address || typeof address === "string") {
      throw new Error("No port");
    }
    const port = address.port;
    mkdirSync(path.join(root, "scripts"));
    for (const name of ["tauri.mjs", "dev-instance.mjs"]) {
      copyFileSync(
        new URL(`./${name}`, import.meta.url),
        path.join(root, "scripts", name),
      );
    }
    const cliDir = path.join(root, "node_modules", "@tauri-apps", "cli");
    mkdirSync(cliDir, { recursive: true });
    writeFileSync(
      path.join(cliDir, "tauri.js"),
      `console.log('RESULT:' + JSON.stringify({id:process.env.CARDBE_DEV_INSTANCE_ID,port:Number(process.env.CARDBE_DEV_PORT),legacy:process.env.CARDBE_DEV_LEGACY_PORT,target:process.env.CARGO_TARGET_DIR,args:process.argv.slice(2)}))`,
    );
    const data = path.join(root, ".cardbe-debug", String(port));
    mkdirSync(data, { recursive: true });
    writeFileSync(path.join(data, "identity"), "original");
    const launch = async (preferred: number) => {
      const child = spawn(
        process.execPath,
        [path.join(root, "scripts", "tauri.mjs"), "dev", String(preferred)],
        {
          env: {
            ...process.env,
            CARDBE_BUILD_COMMIT: "test",
            TAURI_DEV_HOST: "127.0.0.1",
          },
          stdio: ["ignore", "pipe", "pipe"],
        },
      );
      let output = "";
      child.stdout.on("data", (chunk) => {
        output += chunk;
      });
      child.stderr.on("data", (chunk) => {
        output += chunk;
      });
      const [code] = await once(child, "close");
      expect(code, output).toBe(0);
      const result = output.match(/RESULT:(.*)/);
      expect(result, output).not.toBeNull();
      return JSON.parse(result![1]) as {
        legacy: string;
        id: string;
        port: number;
        target: string;
        args: string[];
      };
    };
    const first = await launch(port);
    expect(first.id).toMatch(/^[a-f0-9]{32}$/);
    expect(first.port).not.toBe(port);
    const migrated = path.join(root, ".cardbe-debug", first.id);
    expect(first.legacy).toBe(String(port));
    expect(readFileSync(path.join(data, "identity"), "utf8")).toBe("original");
    expect(existsSync(migrated)).toBe(false);
    // The real desktop performs this migration under its data lock (Rust test).
    renameSync(data, migrated);

    nextServer.listen(first.port, "127.0.0.1");
    await once(nextServer, "listening");
    const second = await launch(first.port);
    expect(second.id).toBe(first.id);
    expect(second.port).not.toBe(first.port);
    expect(readFileSync(path.join(migrated, "identity"), "utf8")).toBe(
      "original",
    );
    expect(
      JSON.parse(second.args[second.args.indexOf("--config") + 1]).build.devUrl,
    ).toBe(`http://127.0.0.1:${second.port}`);
    expect(second.target).toBe(
      path.join(
        root,
        "src-tauri",
        "target",
        second.port === 1420 ? "" : `dev-${second.port}`,
      ),
    );
  } finally {
    await Promise.all(
      [server, nextServer].map(
        (listener) => new Promise((resolve) => listener.close(resolve)),
      ),
    );
    rmSync(root, { recursive: true, force: true });
  }
}, 60000);

it("keeps launching and orphaned runtime instances separate, then reuses an idle ID", async () => {
  const root = mkdtempSync(path.join(tmpdir(), "cardbe-instance-test-"));
  const leases = [];
  try {
    const first = await selectInstance(root);
    leases.push(first.lease);
    if ("runtimeReservation" in first) {
      leases.push(first.runtimeReservation);
    }
    const second = await selectInstance(root);
    leases.push(second.lease);
    if ("runtimeReservation" in second) {
      leases.push(second.runtimeReservation);
    }
    expect(second.id).not.toBe(first.id);
    await close(first.runtimeReservation);
    const runtime = await listen(first.runtimePort);
    expect(runtime).not.toBeNull();
    leases.push(runtime);
    await close(first.lease);
    const third = await selectInstance(root);
    leases.push(third.lease, third.runtimeReservation);
    expect(third.id).not.toBe(first.id);
    expect(third.id).not.toBe(second.id);
    await close(runtime);
    const reused = await selectInstance(root);
    leases.push(reused.lease);
    if ("runtimeReservation" in reused) {
      leases.push(reused.runtimeReservation);
    }
    expect(reused.id).toBe(first.id);
  } finally {
    await Promise.all(leases.map(close));
    rmSync(root, { recursive: true, force: true });
  }
});

it("reserves a selected port throughout compilation and releases it afterward", async () => {
  const root = mkdtempSync(path.join(tmpdir(), "cardbe-reservation-test-"));
  const leases = [];
  try {
    const first = await selectPort(root, 1420, "127.0.0.1");
    leases.push(first.lease);
    const second = await selectPort(root, first.port, "127.0.0.1");
    leases.push(second.lease);
    expect(second.port).not.toBe(first.port);
    await close(first.lease);
    const reused = await selectPort(root, first.port, "127.0.0.1");
    leases.push(reused.lease);
    expect(reused.port).toBe(first.port);
  } finally {
    await Promise.all(leases.map(close));
    rmSync(root, { recursive: true, force: true });
  }
});
