import { execFileSync, spawn } from "node:child_process";
import { fileURLToPath } from "node:url";
import path from "node:path";

const root = fileURLToPath(new URL("../", import.meta.url));
const cli = path.join(root, "node_modules", "@tauri-apps", "cli", "tauri.js");
const args = process.argv.slice(2);
const env = { ...process.env };
const devHost = process.env.TAURI_DEV_HOST || "127.0.0.1";

if (!env.CARDBE_BUILD_COMMIT) {
  try {
    env.CARDBE_BUILD_COMMIT = execFileSync(
      "git",
      ["-c", `safe.directory=${root}`, "rev-parse", "HEAD"],
      { cwd: root, encoding: "utf8" },
    ).trim();
  } catch {
    // Source archives may not contain the git executable or metadata.
  }
}

if (args[0] === "dev") {
  const hasPort = args[1] && /^\d+$/.test(args[1]);
  const port = hasPort ? Number(args.splice(1, 1)[0]) : 1420;
  if (!Number.isInteger(port) || port < 1 || port > 65535) {
    console.error("Usage: pnpm tauri dev [port] (port must be 1-65535)");
    process.exit(1);
  }
  env.CARDBE_DEV_PORT = String(port);
  if (port !== 1420) {
    env.CARGO_TARGET_DIR = path.join(
      root,
      "src-tauri",
      "target",
      `dev-${port}`,
    );
  }
  args.push(
    "--config",
    JSON.stringify({
      build: {
        devUrl: `http://${devHost.includes(":") ? `[${devHost}]` : devHost}:${port}`,
      },
    }),
  );
}

const child = spawn(process.execPath, [cli, ...args], {
  cwd: root,
  env,
  stdio: "inherit",
});

child.once("error", (error) => {
  console.error(`Could not start Tauri: ${error.message}`);
  process.exitCode = 1;
});
child.once("exit", (code, signal) => {
  process.exitCode = signal ? 1 : (code ?? 1);
});
