// `data_version` is the contract's batch identifier (`YYYYMMDDTHHMMSSZ-NNNN`, optionally with a
// `preview-` prefix): unique and sortable, but not meant to be read. Pages show the date and, only
// when several batches share a day, the serial.
const PATTERN = /^(preview-)?(\d{4})(\d{2})(\d{2})T\d{6}Z-(\d{4})$/

export interface DataVersionParts {
  readonly preview: boolean
  readonly date: string
  readonly serial: number
}

export function parseDataVersion(version: string): DataVersionParts | null {
  const match = PATTERN.exec(version)
  if (!match) return null
  return {
    preview: match[1] !== undefined,
    date: `${match[2] ?? ""}-${match[3] ?? ""}-${match[4] ?? ""}`,
    serial: Number(match[5]),
  }
}

/** `2026-09-29`, `2026-09-29 #2`, or the raw string when it is not a contract version. */
export function formatDataVersion(version: string): string {
  const parts = parseDataVersion(version)
  if (!parts) return version
  return parts.serial > 1 ? `${parts.date} #${String(parts.serial)}` : parts.date
}
