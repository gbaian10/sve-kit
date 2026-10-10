import type { JsonPath } from "./json"

/** Machine-comparable rejection reasons; tests match on these, never on message text. */
export const READER_ERROR_CODES = [
  "json",
  "shape",
  "schema",
  "unsupported-version",
  "files-unsorted",
  "dependency-hash",
  "dependency-cycle",
  "metadata-unsorted",
  "text-all-membership",
  "dependency-closure",
  "bootstrap-dependency",
  "programs-dependency",
  "config-programs-count",
  "images-config-dependency",
  "payload-set",
  "blob-integrity",
  "non-table-row-counts",
  "fragment-profile",
  "descriptor-mismatch",
  "row-counts-mismatch",
  "duplicate-fragment",
  "rows-unsorted-or-duplicate",
  "fragment-order",
  "base-dependency",
  "base-identity",
  "base-missing",
  "multiple-details",
  "missing-detail",
  "row-index",
  "face-ordinal",
  "face-card-mismatch",
  "translation-partition",
  "translation-duplicate",
  "primary-key-duplicate",
  "dangling-reference",
  "program-ref-unsupported",
  "card-faces-mismatch",
  "manifest-card-reference",
  "text-id-mismatch",
  "current-history-mismatch",
  "current-face-mismatch",
  "wording-observation-face",
  "wording-undated-inventory",
  "wording-candidate-face",
  "wording-candidate-observation",
  "wording-latest-date",
  "wording-latest-candidate",
  "wording-current-display",
  "wording-current-preserved",
  "wording-region-block",
  "wording-display-candidate",
  "wording-display-face",
  "wording-region-coverage",
  "owner-mismatch",
  "printing-owner-conflict",
  "vocabulary-missing",
  "summary-mismatch",
  "mechanic-universe-mismatch",
  "parameter-range-inverted",
  "spelling-undeclared-parameter",
  "spelling-disabled-domain",
  "hints-parameters-differ",
  "support-missing",
  "image-variant-unapproved",
  "printing-image-face",
  "config-language-fallback",
  "config-url-template",
  "text-all-payload",
  "public-annotation/enum",
  "public-annotation/duplicate_key",
  "public-annotation/reference",
  "public-annotation/ordering",
  "public-annotation/range",
  "public-annotation/owner",
  "public-annotation/text_identity",
  "public-annotation/identity",
  "public-annotation/basis",
] as const

export type ReaderErrorCode = (typeof READER_ERROR_CODES)[number]

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
