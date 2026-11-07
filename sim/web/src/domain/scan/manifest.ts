import type { Region } from "../search"

// The published `scan/manifest.json`: which card-finder (det) and recogniser (rec) model to use, and the
// card index built from the recogniser for each card edition. Large files are named by their content
// hash, so only this list is re-read. `format` names the tensor and file conventions this reader knows.

const SCAN_FORMAT = 1

export interface ScanFile {
  /** Path relative to the scan root, e.g. `models/<sha256>.onnx`. */
  readonly file: string
  readonly bytes: number
  /** Lower-case hex SHA-256 of the stored bytes. */
  readonly sha256: string
}

export interface DetModel extends ScanFile {
  readonly version: number
  /** Square input side in pixels. */
  readonly input: number
  /** Pixels per output grid cell. */
  readonly stride: number
  /** How much larger than the aiming frame the single-card finder looks. */
  readonly grow: number
}

export interface Scoring {
  readonly hubWeight: number
  /** Lowest raw similarity that counts as recognised. */
  readonly accept: number
  /** Lowest lead of the best card over the next one that counts as recognised. */
  readonly margin: number
}

export interface RecModel extends ScanFile {
  readonly version: number
  readonly height: number
  readonly width: number
  readonly dim: number
  readonly scoring: Scoring
}

export interface ScanIndexFiles {
  readonly rec: number
  readonly dataVersion: string
  readonly count: number
  readonly vectors: ScanFile
  readonly scale: ScanFile
  readonly hub: ScanFile
  readonly cards: ScanFile
}

export interface ScanManifest {
  readonly det: DetModel
  readonly rec: RecModel
  readonly index: Partial<Record<Region, ScanIndexFiles>>
}

/** `unsupported`: a newer site understands this list; this one has to be updated first. */
export class ScanManifestError extends Error {
  readonly kind: "unsupported" | "invalid"

  constructor(kind: "unsupported" | "invalid", message: string) {
    super(message)
    this.name = "ScanManifestError"
    this.kind = kind
  }
}

type Json = Record<string, unknown>

function invalid(message: string): never {
  throw new ScanManifestError("invalid", message)
}

function object(value: unknown, at: string): Json {
  if (typeof value !== "object" || value === null || Array.isArray(value))
    invalid(`${at} must be an object`)
  return value as Json
}

function positive(value: unknown, at: string, integer = true): number {
  if (
    typeof value !== "number" ||
    !Number.isFinite(value) ||
    value <= 0 ||
    (integer && !Number.isInteger(value))
  )
    invalid(`${at} must be a positive ${integer ? "integer" : "number"}`)
  return value
}

function finite(value: unknown, at: string): number {
  if (typeof value !== "number" || !Number.isFinite(value)) invalid(`${at} must be a number`)
  return value
}

const FILE = /^(?:models|index)\/[0-9a-f]{64}\.[a-z0-9]+$/u
const SHA256 = /^[0-9a-f]{64}$/u

function scanFile(value: unknown, at: string): ScanFile {
  const row = object(value, at)
  const { file, sha256 } = row
  if (typeof file !== "string" || !FILE.test(file)) invalid(`${at}.file is not a scan file path`)
  if (typeof sha256 !== "string" || !SHA256.test(sha256)) invalid(`${at}.sha256 is not a SHA-256`)
  return { file, bytes: positive(row["bytes"], `${at}.bytes`), sha256 }
}

function indexFiles(value: unknown, at: string, rec: RecModel): ScanIndexFiles {
  const row = object(value, at)
  const dataVersion = row["data_version"]
  if (typeof dataVersion !== "string" || dataVersion === "")
    invalid(`${at}.data_version is missing`)
  const index = {
    rec: positive(row["rec"], `${at}.rec`),
    dataVersion,
    count: positive(row["count"], `${at}.count`),
    vectors: scanFile(row["vectors"], `${at}.vectors`),
    scale: scanFile(row["scale"], `${at}.scale`),
    hub: scanFile(row["hub"], `${at}.hub`),
    cards: scanFile(row["cards"], `${at}.cards`),
  }
  if (index.rec !== rec.version) invalid(`${at} was built for another recogniser`)
  // One int8 row of `dim` values and one float32 scale and hub value per card.
  if (
    index.vectors.bytes !== index.count * rec.dim ||
    index.scale.bytes !== index.count * 4 ||
    index.hub.bytes !== index.count * 4
  )
    invalid(`${at} sizes do not match its card count`)
  return index
}

/** Reads the manifest; a `format` this reader does not know is `unsupported`, anything malformed `invalid`. */
export function parseScanManifest(value: unknown): ScanManifest {
  const root = object(value, "manifest")
  if (root["format"] !== SCAN_FORMAT)
    throw new ScanManifestError("unsupported", `manifest format ${String(root["format"])}`)
  const det = object(root["det"], "det")
  const rec = object(root["rec"], "rec")
  const input = rec["input"]
  if (!Array.isArray(input) || input.length !== 2) invalid("rec.input must be [height, width]")
  const scoring = object(rec["scoring"], "rec.scoring")
  const recModel: RecModel = {
    ...scanFile(rec, "rec"),
    version: positive(rec["version"], "rec.version"),
    height: positive(input[0], "rec.input[0]"),
    width: positive(input[1], "rec.input[1]"),
    dim: positive(rec["dim"], "rec.dim"),
    scoring: {
      hubWeight: finite(scoring["hubWeight"], "rec.scoring.hubWeight"),
      accept: finite(scoring["accept"], "rec.scoring.accept"),
      margin: finite(scoring["margin"], "rec.scoring.margin"),
    },
  }
  const detModel: DetModel = {
    ...scanFile(det, "det"),
    version: positive(det["version"], "det.version"),
    input: positive(det["input"], "det.input"),
    stride: positive(det["stride"], "det.stride"),
    grow: positive(det["grow"], "det.grow", false),
  }
  if (detModel.input % detModel.stride !== 0) invalid("det.input must be a multiple of det.stride")
  const indexRow = object(root["index"], "index")
  const index: Partial<Record<Region, ScanIndexFiles>> = {}
  for (const region of ["jp", "en"] as const)
    if (indexRow[region] !== undefined)
      index[region] = indexFiles(indexRow[region], `index.${region}`, recModel)
  return { det: detModel, rec: recModel, index }
}
