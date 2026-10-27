/** Permanent IDs are decimal values, never parsed from card numbers or array positions. */
export function imageUrl(
  base: string,
  intId: number,
  ordinal: number,
  size: string,
  version: number,
): string {
  return `${base}/images/${size}/${String(intId)}${ordinal === 0 ? "" : `-f${String(ordinal)}`}.webp?v=${String(version)}`
}
