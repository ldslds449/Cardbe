import { createServer } from "node:net";
import { randomUUID } from "node:crypto";
import {
  mkdirSync,
  readdirSync,
  readFileSync,
  writeFileSync,
  linkSync,
  unlinkSync,
  existsSync,
} from "node:fs";
import path from "node:path";

const validPort = (value) =>
  Number.isInteger(value) && value > 0 && value <= 65535;
export function listen(port, host = "127.0.0.1") {
  return new Promise((resolve, reject) => {
    const server = createServer((socket) => socket.destroy());
    server.once("error", (error) => {
      if (["EADDRINUSE", "EACCES"].includes(error.code)) {
        resolve(null);
      } else {
        reject(error);
      }
    });
    server.listen({ port, host, exclusive: true }, () => resolve(server));
  });
}
export const close = (server) =>
  new Promise((resolve) => (server ? server.close(resolve) : resolve()));

// Publish complete metadata atomically; concurrent launches must see the same guards.
function publish(file, profile) {
  const temporary = `${file}.${randomUUID()}.tmp`;
  writeFileSync(temporary, JSON.stringify(profile));
  try {
    linkSync(temporary, file);
    return true;
  } catch (error) {
    if (error.code === "EEXIST") {
      return false;
    }
    throw error;
  } finally {
    unlinkSync(temporary);
  }
}
async function createProfile(directory, legacyPort) {
  const launch = await listen(0);
  let runtime;
  let keepLease = false;
  try {
    runtime = await listen(0);
    if (!launch || !runtime) {
      throw new Error("Could not reserve instance guards");
    }
    const id = randomUUID().replaceAll("-", "");
    const profile = {
      id,
      launchPort: launch.address().port,
      runtimePort: runtime.address().port,
      port: legacyPort ?? 1420,
      legacyPort,
    };
    const file = path.join(
      directory,
      legacyPort ? `legacy-${legacyPort}.json` : `${id}.json`,
    );
    if (!publish(file, profile)) {
      return null;
    }
    keepLease = true;
    return { ...profile, file, lease: launch, runtimeReservation: runtime };
  } finally {
    if (!keepLease) {
      await close(runtime);
      await close(launch);
    }
  }
}
export async function selectInstance(root, preferredPort) {
  const directory = path.join(root, ".instances");
  mkdirSync(directory, { recursive: true });
  // Create a stable mapping for old port directories. Migration itself happens
  // in the desktop under its existing OS data lock, never in this JS launcher.
  for (const entry of readdirSync(root, { withFileTypes: true })) {
    const port = Number(entry.name);
    if (
      entry.isDirectory() &&
      validPort(port) &&
      String(port) === entry.name &&
      !existsSync(path.join(directory, `legacy-${port}.json`))
    ) {
      const profile = await createProfile(directory, port);
      if (profile) {
        await close(profile.runtimeReservation);
        await close(profile.lease);
      }
    }
  }
  const profiles = readdirSync(directory)
    .filter((name) => name.endsWith(".json"))
    .map((name) => {
      const file = path.join(directory, name);
      const profile = JSON.parse(readFileSync(file, "utf8"));
      if (
        !/^[a-f0-9]{32}$/.test(profile.id) ||
        !validPort(profile.launchPort) ||
        !validPort(profile.runtimePort) ||
        !validPort(profile.port) ||
        (profile.legacyPort !== undefined && !validPort(profile.legacyPort))
      ) {
        throw new Error(`Invalid development instance metadata: ${file}`);
      }
      let lastPort;
      try {
        lastPort = Number(
          readFileSync(path.join(directory, profile.id + ".port"), "utf8"),
        );
      } catch (error) {
        if (error.code !== "ENOENT") {
          throw error;
        }
      }
      return {
        ...profile,
        port: validPort(lastPort) ? lastPort : profile.port,
        file,
      };
    });
  let primary;
  try {
    primary = readFileSync(path.join(root, ".primary"), "utf8").trim();
  } catch (error) {
    if (error.code !== "ENOENT") {
      throw error;
    }
  }
  const rank = (profile) =>
    preferredPort && profile.port === preferredPort
      ? 0
      : profile.id === primary
        ? 1
        : profile.legacyPort === 1420
          ? 2
          : 3;
  profiles.sort((a, b) => rank(a) - rank(b) || a.id.localeCompare(b.id));
  for (const profile of profiles) {
    const lease = await listen(profile.launchPort);
    if (!lease) {
      continue;
    }
    let runtime;
    try {
      runtime = await listen(profile.runtimePort);
    } catch (error) {
      await close(lease);
      throw error;
    }
    if (!runtime) {
      await close(lease);
      continue;
    }
    try {
      writeFileSync(path.join(root, ".primary"), profile.id, { flag: "wx" });
    } catch (error) {
      if (error.code !== "EEXIST") {
        await close(runtime);
        await close(lease);
        throw error;
      }
    }
    return { ...profile, lease, runtimeReservation: runtime };
  }
  const profile = await createProfile(directory);
  try {
    writeFileSync(path.join(root, ".primary"), profile.id, { flag: "wx" });
  } catch (error) {
    if (error.code !== "EEXIST") {
      await close(profile.runtimeReservation);
      await close(profile.lease);
      throw error;
    }
  }
  return profile;
}
export async function selectPort(root, preferred, host) {
  const directory = path.join(root, ".ports");
  mkdirSync(directory, { recursive: true });
  for (let offset = 0; offset <= 65535 - 1420 + 1; offset++) {
    const port = offset === 0 ? preferred : 1419 + offset;
    const file = path.join(directory, port + ".json");
    let lease;
    if (!existsSync(file)) {
      const candidate = await listen(0);
      if (!candidate) {
        throw new Error("Could not reserve port guard");
      }
      try {
        if (publish(file, { guardPort: candidate.address().port })) {
          lease = candidate;
        }
      } finally {
        if (!lease) {
          await close(candidate);
        }
      }
    }
    if (!lease) {
      const { guardPort } = JSON.parse(readFileSync(file, "utf8"));
      if (!validPort(guardPort)) {
        throw new Error("Invalid port guard metadata: " + file);
      }
      lease = await listen(guardPort);
      if (!lease) {
        continue;
      }
    }
    let probe;
    try {
      probe = await listen(port, host);
    } catch (error) {
      await close(lease);
      throw error;
    }
    if (probe) {
      await close(probe);
      return { port, lease };
    }
    await close(lease);
  }
  throw new Error("No available development port");
}
