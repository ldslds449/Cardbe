import katex from "katex";

export const mathOptions = {
  trust: false,
  maxSize: 20,
  maxExpand: 1000,
  throwOnError: true,
} as const;

export function renderMath(source: string, display = false): string | null {
  if (source.length > 10_000) {
    return null;
  }
  try {
    return katex.renderToString(source, {
      ...mathOptions,
      displayMode: display,
    });
  } catch {
    return null;
  }
}
