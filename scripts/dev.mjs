import { spawnSync } from "node:child_process";
import { fileURLToPath } from "node:url";
import path from "node:path";

const root = path.resolve(path.dirname(fileURLToPath(import.meta.url)), "..");

const result = spawnSync(
  process.execPath,
  [path.join(root, "scripts", "tauri.mjs"), "dev", ...process.argv.slice(2)],
  {
    cwd: root,
    stdio: "inherit",
    env: {
      ...process.env,
      // Compiler-cache wrappers such as kache (configured in Cargo) cannot
      // cache incremental builds.
      CARGO_INCREMENTAL: "0",
    },
  },
);

if (result.error) {
  console.error(`Could not start Tauri: ${result.error.message}`);
}
process.exit(result.status ?? 1);
