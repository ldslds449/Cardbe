// Copies the built release files into one directory, named after the app
// version, operating system and architecture, such as
// `Cardbe_1.9.0_windows_x64_setup.exe` or `Cardbe_1.9.0_macos_arm64.dmg`.
// Usage: node scripts/stage-dist.mjs <output directory>
import {
  copyFileSync,
  existsSync,
  mkdirSync,
  readdirSync,
  readFileSync,
} from "node:fs";
import path from "node:path";
import { fileURLToPath } from "node:url";

const root = path.resolve(path.dirname(fileURLToPath(import.meta.url)), "..");
const release = path.join(root, "src-tauri", "target", "release");

const SYSTEMS = { win32: "windows", darwin: "macos" };
const ARCHITECTURES = { x64: "x64", arm64: "arm64" };

/** Release files per platform: directory, file filter and name suffix. */
const SOURCES = {
  win32: [
    {
      dir: "",
      accepts: (file) => file === "Cardbe.exe",
      suffix: "_portable.exe",
    },
    {
      dir: "bundle/nsis",
      accepts: (file) => file.endsWith(".exe"),
      suffix: "_setup.exe",
    },
    {
      dir: "bundle/msi",
      accepts: (file) => file.endsWith(".msi"),
      suffix: ".msi",
    },
  ],
  darwin: [
    {
      dir: "bundle/dmg",
      accepts: (file) => file.endsWith(".dmg"),
      suffix: ".dmg",
    },
  ],
};

export function dist_name(version, platform, arch, suffix) {
  const system = SYSTEMS[platform];
  const architecture = ARCHITECTURES[arch];
  if (!system || !architecture) {
    throw new Error(`Unsupported release platform: ${platform} ${arch}`);
  }
  return `Cardbe_${version}_${system}_${architecture}${suffix}`;
}

export function stage(
  output,
  { platform = process.platform, arch = process.arch } = {},
) {
  const sources = SOURCES[platform];
  if (!sources) {
    throw new Error(`Unsupported release platform: ${platform}`);
  }
  const { version } = JSON.parse(
    readFileSync(path.join(root, "package.json"), "utf8"),
  );
  mkdirSync(output, { recursive: true });
  return sources.map(({ dir, accepts, suffix }) => {
    const directory = path.join(release, dir);
    const matches = existsSync(directory)
      ? readdirSync(directory).filter(accepts)
      : [];
    if (matches.length !== 1) {
      throw new Error(
        `Expected one release file in ${directory}, found ${matches.length}`,
      );
    }
    const target = path.join(
      output,
      dist_name(version, platform, arch, suffix),
    );
    copyFileSync(path.join(directory, matches[0]), target);
    return target;
  });
}

if (
  process.argv[1] &&
  path.resolve(process.argv[1]) === fileURLToPath(import.meta.url)
) {
  const output = process.argv[2];
  if (!output) {
    console.error("Usage: node scripts/stage-dist.mjs <output directory>");
    process.exit(1);
  }
  for (const file of stage(path.resolve(output))) {
    console.log(file);
  }
}
