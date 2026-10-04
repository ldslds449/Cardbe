import { execFileSync, spawn } from "node:child_process";
import { writeFileSync } from "node:fs";
import { fileURLToPath } from "node:url";
import path from "node:path";
import { selectInstance, selectPort, close } from "./dev-instance.mjs";

const root = fileURLToPath(new URL("../", import.meta.url));
const cli = path.join(root, "node_modules", "@tauri-apps", "cli", "tauri.js");
const args = process.argv.slice(2);
const env = { ...process.env };
const devHost = process.env.TAURI_DEV_HOST || "127.0.0.1";
let instance;
let portReservation;
try {
  if (args[0] === "dev") {
    const hasPort = args[1] && /^\d+$/.test(args[1]);
    const preferred = hasPort ? Number(args.splice(1, 1)[0]) : undefined;
    if (
      hasPort &&
      (!Number.isInteger(preferred) || preferred < 1 || preferred > 65535)
    ) {
      throw new Error("Usage: vp run tauri dev [preferred-port] (1-65535)");
    }
    const dataRoot = path.join(root, ".cardbe-debug");
    instance = await selectInstance(dataRoot, preferred);
    portReservation = await selectPort(
      dataRoot,
      preferred ?? instance.port,
      devHost,
    );
    const { port } = portReservation;
    writeFileSync(
      path.join(dataRoot, ".instances", instance.id + ".port"),
      String(port),
    );
    env.CARDBE_DEV_INSTANCE_ID = instance.id;
    env.CARDBE_DEV_INSTANCE_GUARD_PORT = String(instance.runtimePort);
    env.CARDBE_DEV_PORT = String(port);
    delete env.CARDBE_DEV_MIGRATE_FROM;
    delete env.CARDBE_DEV_LEGACY_PORT;
    if (instance.legacyPort) {
      env.CARDBE_DEV_LEGACY_PORT = String(instance.legacyPort);
    }
    env.CARGO_TARGET_DIR = path.join(
      root,
      "src-tauri",
      "target",
      port === 1420 ? "" : `dev-${port}`,
    );
    console.log(
      `Dev instance: ${instance.id}\nData: ${path.join(dataRoot, instance.id)}\nPort: ${port} (preferred ${preferred ?? instance.port})`,
    );
    args.push(
      "--config",
      JSON.stringify({
        build: {
          devUrl: `http://${devHost.includes(":") ? `[${devHost}]` : devHost}:${port}`,
        },
      }),
    );
  }
  if (!env.CARDBE_BUILD_COMMIT) {
    try {
      env.CARDBE_BUILD_COMMIT = execFileSync(
        "git",
        ["-c", `safe.directory=${root}`, "rev-parse", "HEAD"],
        { cwd: root, encoding: "utf8" },
      ).trim();
    } catch {
      /* Source archives may not contain git metadata. */
    }
  }
  await close(instance?.runtimeReservation);
  process.exitCode = await new Promise((resolve, reject) => {
    const child = spawn(process.execPath, [cli, ...args], {
      cwd: root,
      env,
      stdio: "inherit",
    });
    child.once("error", reject);
    child.once("exit", (code, signal) => resolve(signal ? 1 : (code ?? 1)));
  });
} catch (error) {
  console.error(`Could not start Tauri: ${error.message}`);
  process.exitCode = 1;
} finally {
  await close(portReservation?.lease);
  await close(instance?.runtimeReservation);
  await close(instance?.lease);
}
