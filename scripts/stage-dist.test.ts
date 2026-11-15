import { expect, it } from "vite-plus/test";
import { dist_name } from "./stage-dist.mjs";

it("names release files after the system and architecture", () => {
  expect(dist_name("1.9.0", "win32", "x64", "_setup.exe")).toBe(
    "Cardbe_1.9.0_windows_x64_setup.exe",
  );
  expect(dist_name("1.9.0", "win32", "x64", "_portable.exe")).toBe(
    "Cardbe_1.9.0_windows_x64_portable.exe",
  );
  expect(dist_name("1.9.0", "darwin", "arm64", ".dmg")).toBe(
    "Cardbe_1.9.0_macos_arm64.dmg",
  );
  expect(dist_name("1.9.0", "darwin", "x64", ".dmg")).toBe(
    "Cardbe_1.9.0_macos_x64.dmg",
  );
  expect(() => dist_name("1.9.0", "linux", "x64", ".deb")).toThrow();
});
