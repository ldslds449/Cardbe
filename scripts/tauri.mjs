import { execFileSync, spawn } from "node:child_process";
import { createServer } from "node:net";
import { fileURLToPath } from "node:url";
import path from "node:path";
import {
  mkdirSync,
  existsSync,
  readFileSync,
  readdirSync,
  statSync,
  renameSync,
  writeFileSync,
} from "node:fs";

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
const portAvailable = (port) =>
  new Promise((resolve, reject) => {
    const server = createServer();
    server.once("error", (error) => {
      if (["EADDRINUSE", "EACCES"].includes(error.code)) {
        resolve(false);
      } else {
        reject(error);
      }
    });
    server.listen({ port, host: devHost, exclusive: true }, () =>
      server.close(() => resolve(true)),
    );
  });

if (args[0] === "dev") {
  const target = path.join(root, "src-tauri", "target");
  const portFile = path.join(target, ".dev-port");
  const migrationFile = path.join(target, ".dev-migrate-from");
  const validPort = (port) =>
    Number.isInteger(port) && port >= 1 && port <= 65535;
  const hasPort = args[1] && /^\d+$/.test(args[1]);
  delete env.CARDBE_DEV_MIGRATE_FROM;
  let migrateFrom;
  if (!hasPort && process.platform !== "win32") {
    try {
      migrateFrom = Number(readFileSync(migrationFile, "utf8").trim());
      if (!validPort(migrateFrom)) {
        throw new Error(`Invalid migration record: ${migrationFile}`);
      }
    } catch (error) {
      if (error.code !== "ENOENT") {
        throw error;
      }
    }
  }
  let savedPort;
  let previous = [];
  try {
    savedPort = Number(readFileSync(portFile, "utf8").trim());
  } catch (error) {
    if (error.code !== "ENOENT") {
      throw error;
    }
  }
  try {
    previous = readdirSync(target, { withFileTypes: true })
      .filter(
        (entry) => entry.isDirectory() && /^(debug|dev-\d+)$/.test(entry.name),
      )
      .map((entry) => ({
        port: entry.name === "debug" ? 1420 : Number(entry.name.slice(4)),
        modified: statSync(path.join(target, entry.name)).mtimeMs,
      }))
      .filter((entry) => validPort(entry.port))
      .sort((a, b) => b.modified - a.modified);
  } catch (error) {
    if (error.code !== "ENOENT") {
      throw error;
    }
  }
  let port = hasPort
    ? Number(args.splice(1, 1)[0])
    : validPort(savedPort)
      ? savedPort
      : (previous[0]?.port ?? 1420);
  if (!validPort(port)) {
    console.error("Usage: pnpm tauri dev [port] (port must be 1-65535)");
    process.exit(1);
  }
  const usedPorts = [
    ...new Set([...previous.map((entry) => entry.port), savedPort]),
  ]
    .filter(validPort)
    .sort((a, b) => a - b);
  console.log(`Previously used dev ports: ${usedPorts.join(", ") || "none"}`);
  const originalPort = port;
  if (!hasPort && !(await portAvailable(port))) {
    // Keep this directory's data together; never switch to an existing data set.
    const candidates = usedPorts.filter((candidate) => candidate !== port);
    for (let candidate = port + 1; candidate <= 65535; candidate++) {
      candidates.push(candidate);
    }
    for (let candidate = 1420; candidate < port; candidate++) {
      candidates.push(candidate);
    }
    port = undefined;
    for (const candidate of new Set(candidates)) {
      const candidateData = path.join(root, ".cardbe-debug", String(candidate));
      if (existsSync(candidateData)) {
        continue;
      }
      if (await portAvailable(candidate)) {
        port = candidate;
        break;
      }
    }
    if (!port) {
      throw new Error("No available development port found");
    }
    console.log(`Dev port ${originalPort} is occupied; switching to ${port}`);
  }
  mkdirSync(target, { recursive: true });
  if (port !== originalPort) {
    const sourceData = path.join(root, ".cardbe-debug", String(originalPort));
    if (existsSync(sourceData) && migrateFrom === undefined) {
      const destinationData = path.join(root, ".cardbe-debug", String(port));
      if (process.platform !== "win32") {
        // Persist across build failures; Tauri clears this after migrating under its OS lock.
        migrateFrom = originalPort;
        writeFileSync(migrationFile, String(migrateFrom));
      } else {
        // Windows refuses renames while the OS data lock is held, even by an older Cardbe.
        if (existsSync(destinationData)) {
          throw new Error(
            `Refusing to overwrite existing data: ${destinationData}`,
          );
        }
        try {
          renameSync(sourceData, destinationData);
        } catch (error) {
          if (!["EPERM", "EACCES", "EBUSY"].includes(error.code)) {
            throw error;
          }
          throw new Error(
            `Could not move development data from ${originalPort} to ${port}. Close any Cardbe still using this data and retry.`,
            { cause: error },
          );
        }
        console.log(`Moved development data from ${originalPort} to ${port}`);
      }
    }
    const directories = (value) => [
      path.join(target, value === 1420 ? "debug" : `dev-${value}`),
      path.join(
        root,
        value === 1420 ? ".svelte-kit" : `.svelte-kit-dev-${value}`,
      ),
      path.join(
        root,
        "node_modules",
        value === 1420 ? ".vite" : `.vite-cardbe-${value}`,
      ),
    ];
    const destinations = directories(port);
    directories(originalPort).forEach((source, index) => {
      const destination = destinations[index];
      if (!existsSync(source) || existsSync(destination)) {
        return;
      }
      try {
        renameSync(source, destination);
        console.log(
          `Moved ${path.relative(root, source)} to ${path.relative(root, destination)}`,
        );
      } catch (error) {
        console.warn(
          `Could not move cache ${path.relative(root, source)}; it will be rebuilt: ${error.message}`,
        );
      }
    });
  }
  if (migrateFrom !== undefined) {
    env.CARDBE_DEV_MIGRATE_FROM = String(migrateFrom);
    console.log(
      `Tauri will migrate development data from ${migrateFrom} to ${port}`,
    );
  }
  writeFileSync(portFile, String(port));
  console.log(`Using dev port ${port}`);
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
