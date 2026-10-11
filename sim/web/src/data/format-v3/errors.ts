import type { JsonPath } from "./json"

export type ReaderErrorCode =
  | "json"
  | "shape"
  | "unsupported-version"
  | "config-programs-count"
  | "payload-set"
  | "blob-integrity"
  | "base-missing"
  | "row-index"
  | "face-ordinal"
  | "public-annotation/reference"
  | "public-annotation/owner"

export class SnapshotError extends Error {
  readonly code: ReaderErrorCode
  readonly path: JsonPath

  constructor(code: ReaderErrorCode, detail: string, path: JsonPath = []) {
    super(path.length > 0 ? `${code} at ${path.join("/")}: ${detail}` : `${code}: ${detail}`)
    this.name = "SnapshotError"
    this.code = code
    this.path = path
  }
}

export function fail(code: ReaderErrorCode, detail: string, path: JsonPath = []): never {
  throw new SnapshotError(code, detail, path)
}
