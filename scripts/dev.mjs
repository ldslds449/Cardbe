import { execSync, spawnSync } from "node:child_process";

const repoRoot = execSync("git rev-parse --show-toplevel", {
  encoding: "utf8",
}).trim();

const pnpm = process.platform === "win32" ? "pnpm.cmd" : "pnpm";

console.log(`SCCACHE_BASEDIRS=${repoRoot}`);

const result = spawnSync(pnpm, ["tauri", "dev"], {
  stdio: "inherit",
  shell: process.platform === "win32",
  env: {
    ...process.env,
    SCCACHE_BASEDIRS: repoRoot,
    CARGO_INCREMENTAL: "0",
  },
});

process.exit(result.status ?? 1);
