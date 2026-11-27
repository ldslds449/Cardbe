import { getLocale } from "#lib/i18n/index.js";

/** Abbreviated device ID shown in device lists and sync notifications. */
export function short_device_id(node_id: string) {
  return `${node_id.slice(0, 8)}…${node_id.slice(-8)}`;
}

/** Locale-aware list of abbreviated device IDs. */
export function format_device_ids(node_ids: readonly string[]) {
  return new Intl.ListFormat(getLocale(), {
    style: "short",
    type: "unit",
  }).format(node_ids.map(short_device_id));
}
