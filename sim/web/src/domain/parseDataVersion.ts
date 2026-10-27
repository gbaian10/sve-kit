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
