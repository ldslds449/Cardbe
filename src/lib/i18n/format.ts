import { getLocale } from "./locale.svelte";

// Constructing an Intl formatter costs far more than formatting with one, and
// views format thousands of dates per render.
const date_formats = new Map<string, Intl.DateTimeFormat>();
const number_formats = new Map<string, Intl.NumberFormat>();

function cached<T>(
  cache: Map<string, T>,
  locale: string,
  options: object | undefined,
  create: () => T,
): T {
  const key = `${locale}|${options ? JSON.stringify(options) : ""}`;
  let format = cache.get(key);
  if (!format) {
    format = create();
    cache.set(key, format);
  }
  return format;
}

export function formatDate(
  value: Date | number,
  options?: Intl.DateTimeFormatOptions,
) {
  const locale = getLocale();
  return cached(
    date_formats,
    locale,
    options,
    () => new Intl.DateTimeFormat(locale, options),
  ).format(value);
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
  const locale = getLocale();
  return cached(
    number_formats,
    locale,
    options,
    () => new Intl.NumberFormat(locale, options),
  ).format(value);
}
