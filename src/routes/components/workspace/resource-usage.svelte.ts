import { invoke } from "@tauri-apps/api/core";
import { formatNumber } from "$lib/i18n";

export interface UsageGroup {
  /** Share of all logical processors, 0–100; null before a second sample. */
  cpu_percent: number | null;
  memory_bytes: number;
  process_count: number;
}

export interface ResourceUsage {
  total: UsageGroup;
  main: UsageGroup;
  children: UsageGroup;
  logical_cpus: number;
  memory_metric: "private_working_set" | "working_set" | "resident_set";
}

export interface UsagePoint {
  cpu_percent: number | null;
  memory_bytes: number | null;
}

export const POLL_INTERVAL_MS = 1000;
export const HISTORY_LENGTH = 60;
export const UNAVAILABLE = "--";

const MIB = 1024 * 1024;

export function format_cpu(percent: number | null | undefined) {
  if (percent == null || !Number.isFinite(percent)) {
    return UNAVAILABLE;
  }
  return formatNumber(percent / 100, {
    style: "percent",
    minimumFractionDigits: 1,
    maximumFractionDigits: 1,
  });
}

/** Binary units labelled MB / GB, like Task Manager. */
export function format_memory(bytes: number | null | undefined) {
  if (bytes == null || !Number.isFinite(bytes)) {
    return UNAVAILABLE;
  }
  const mib = bytes / MIB;
  if (mib < 1024) {
    return formatNumber(mib, {
      style: "unit",
      unit: "megabyte",
      maximumFractionDigits: 0,
    });
  }
  return formatNumber(mib / 1024, {
    style: "unit",
    unit: "gigabyte",
    minimumFractionDigits: 2,
    maximumFractionDigits: 2,
  });
}

interface MonitorOptions {
  fetch?: () => Promise<ResourceUsage>;
  interval_ms?: number;
  now?: () => number;
}

/**
 * Polls one shared usage sample while any view subscribes. Polling waits for
 * each request before scheduling the next, pauses while the page is hidden,
 * and keeps history only while a history view is open.
 */
export class ResourceMonitor {
  usage = $state<ResourceUsage | null>(null);
  status = $state<"loading" | "ready" | "unavailable">("loading");
  history = $state<UsagePoint[]>([]);

  #fetch: () => Promise<ResourceUsage>;
  #interval: number;
  #now: () => number;
  #subscribers = 0;
  #history_subscribers = 0;
  #timer: ReturnType<typeof setTimeout> | null = null;
  #in_flight = false;

  constructor(options: MonitorOptions = {}) {
    this.#fetch =
      options.fetch ?? (() => invoke<ResourceUsage>("get_resource_usage"));
    this.#interval = options.interval_ms ?? POLL_INTERVAL_MS;
    this.#now = options.now ?? (() => Date.now());
  }

  /** Starts polling for the first subscriber; call the result to leave. */
  subscribe(): () => void {
    this.#subscribers++;
    if (this.#subscribers === 1 && typeof document !== "undefined") {
      document.addEventListener("visibilitychange", this.#visibility_changed);
    }
    this.#resume();
    let left = false;
    return () => {
      if (left) {
        return;
      }
      left = true;
      this.#subscribers--;
      if (this.#subscribers === 0) {
        if (typeof document !== "undefined") {
          document.removeEventListener(
            "visibilitychange",
            this.#visibility_changed,
          );
        }
        this.#pause();
        this.status = "loading";
        this.usage = null;
      }
    };
  }

  /** Records history while the result has not been called. */
  track_history(): () => void {
    this.#history_subscribers++;
    let left = false;
    return () => {
      if (left) {
        return;
      }
      left = true;
      this.#history_subscribers--;
      if (this.#history_subscribers === 0) {
        this.history = [];
      }
    };
  }

  get polling() {
    return this.#timer !== null || this.#in_flight;
  }

  get #active() {
    return (
      this.#subscribers > 0 &&
      (typeof document === "undefined" || document.visibilityState !== "hidden")
    );
  }

  #visibility_changed = () => {
    if (this.#active) {
      this.#resume();
    } else {
      this.#pause();
    }
  };

  #resume() {
    if (this.#active && this.#timer === null && !this.#in_flight) {
      void this.#poll();
    }
  }

  #pause() {
    if (this.#timer !== null) {
      clearTimeout(this.#timer);
      this.#timer = null;
    }
  }

  async #poll() {
    this.#timer = null;
    this.#in_flight = true;
    const started = this.#now();
    let usage: ResourceUsage | null = null;
    try {
      usage = await this.#fetch();
    } catch {
      // The view shows "--"; the backend logs the first failure of a streak.
    }
    this.#in_flight = false;
    if (!this.#active) {
      return;
    }
    this.usage = usage;
    this.status = usage ? "ready" : "unavailable";
    if (this.#history_subscribers > 0) {
      const point = {
        cpu_percent: usage?.total.cpu_percent ?? null,
        memory_bytes: usage?.total.memory_bytes ?? null,
      };
      this.history = [...this.history, point].slice(-HISTORY_LENGTH);
    }
    if (this.#timer === null) {
      const elapsed = this.#now() - started;
      this.#timer = setTimeout(
        () => void this.#poll(),
        Math.max(0, this.#interval - elapsed),
      );
    }
  }
}

export const resource_monitor = new ResourceMonitor();
