import { parseDataVersion } from "./parseDataVersion"

// `data_version` is the contract's batch identifier (`YYYYMMDDTHHMMSSZ-NNNN`, optionally with a
// `preview-` prefix): unique and sortable, but not meant to be read. Pages show the date and, only
// when several batches share a day, the serial.
/** `2026-09-29`, `2026-09-29 #2`, or the raw string when it is not a contract version. */
export function formatDataVersion(version: string): string {
  const parts = parseDataVersion(version)
  if (!parts) return version
  return parts.serial > 1 ? `${parts.date} #${String(parts.serial)}` : parts.date
}
