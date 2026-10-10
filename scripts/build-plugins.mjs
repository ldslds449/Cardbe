import { spawnSync } from "node:child_process";
import { mkdirSync } from "node:fs";
import { fileURLToPath } from "node:url";
import path from "node:path";

const root = path.resolve(path.dirname(fileURLToPath(import.meta.url)), "..");
function run(command, args) {
  const result = spawnSync(command, args, {
    cwd: root,
    stdio: "inherit",
  });
  if (result.error) {
    throw result.error;
  }
  if (result.status !== 0) {
    process.exit(result.status ?? 1);
  }
}
// Only test fixtures are built here; production plugins release independently.
// wasm32-wasip2 adds WASI imports that Cardbe deliberately does not link.
const fixture = path.join(root, "src-tauri/test-fixtures/runtime-fixture");
const output = path.join(fixture, "dist");
mkdirSync(output, { recursive: true });
run("cargo", [
  "build",
  "--manifest-path",
  "src-tauri/test-fixtures/runtime-fixture/Cargo.toml",
  "--locked",
  "--release",
  "--target",
  "wasm32-unknown-unknown",
  "--target-dir",
  path.join(fixture, "target"),
]);
run("wasm-tools", [
  "component",
  "new",
  path.join(
    fixture,
    "target/wasm32-unknown-unknown/release/cardbe_runtime_fixture.wasm",
  ),
  "-o",
  path.join(output, "component.wasm"),
]);
console.log(`Built test-only component: ${output}`);
