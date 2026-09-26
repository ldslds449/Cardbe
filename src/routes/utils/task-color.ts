import { mode } from "mode-watcher";

export const task_color_presets = [
  { name: "Slate", light: "#64748b", dark: "#6d7e94" },
  { name: "Red", light: "#ef4444", dark: "#d46666" },
  { name: "Orange", light: "#f97316", dark: "#d47f50" },
  { name: "Amber", light: "#f59e0b", dark: "#c89a45" },
  { name: "Green", light: "#22c55e", dark: "#5dad76" },
  { name: "Cyan", light: "#06b6d4", dark: "#49a8b4" },
  { name: "Blue", light: "#3b82f6", dark: "#608dcc" },
  { name: "Violet", light: "#8b5cf6", dark: "#8870c4" },
  { name: "Pink", light: "#ec4899", dark: "#ca6592" },
] as const;

export type TaskColorPreset = (typeof task_color_presets)[number];

export function preset_display_color(preset: TaskColorPreset): string {
  return mode.current === "light" ? preset.light : preset.dark;
}

export function is_preset_color(
  color: string,
  preset: TaskColorPreset,
): boolean {
  const normalized_color = color.toLowerCase();
  return normalized_color === preset.light || normalized_color === preset.dark;
}

export function display_task_color(color: string): string {
  const preset = task_color_presets.find((item) =>
    is_preset_color(color, item),
  );
  return preset ? preset_display_color(preset) : color;
}
