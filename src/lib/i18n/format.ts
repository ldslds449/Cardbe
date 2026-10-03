import { getLocale } from "./locale.svelte";

export function formatDate(
  value: Date | number,
  options?: Intl.DateTimeFormatOptions,
) {
  return new Intl.DateTimeFormat(getLocale(), options).format(value);
}
export function formatDateTime(
  value: Date | number,
  options: Intl.DateTimeFormatOptions = {
    dateStyle: "medium",
    timeStyle: "short",
  },
) {
  return formatDate(value, options);
}
export function formatNumber(
  value: number,
  options?: Intl.NumberFormatOptions,
) {
  return new Intl.NumberFormat(getLocale(), options).format(value);
}
