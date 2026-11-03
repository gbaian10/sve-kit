import {
  type Deck,
  type DeckBasis,
  type DeckEntry,
  type DeckFormat,
  type DeckLeader,
  FORMATS_OF,
} from "./types"

const isRecord = (value: unknown): value is Record<string, unknown> =>
  typeof value === "object" && value !== null && !Array.isArray(value)

const isString = (value: unknown): value is string => typeof value === "string"

// The list formats these times, so a stored value that is not a date would break the whole page.
const isTime = (value: unknown): value is string =>
  isString(value) && !Number.isNaN(Date.parse(value))

function optionalPrinting(value: Record<string, unknown>): { printingId?: string } {
  const printingId = value["printingId"]
  return isString(printingId) ? { printingId } : {}
}

function parseEntries(value: unknown): DeckEntry[] | null {
  if (!Array.isArray(value)) return null
  const entries: DeckEntry[] = []
  for (const item of value as unknown[]) {
    if (!isRecord(item) || !isString(item["cardId"])) return null
    const count = item["count"]
    if (typeof count !== "number" || !Number.isInteger(count) || count < 1) return null
    entries.push({ cardId: item["cardId"], count, ...optionalPrinting(item) })
  }
  return entries
}

function parseBasis(value: unknown): DeckBasis | null {
  if (!isRecord(value)) return null
  if (value["kind"] === "class" && isString(value["classCode"]))
    return { kind: "class", classCode: value["classCode"] }
  if (value["kind"] === "title" && isString(value["titleCode"]))
    return { kind: "title", titleCode: value["titleCode"] }
  return null
}

// Decks saved before formats were recorded are Standard decks.
function parseFormat(value: unknown, edition: "jp" | "en"): DeckFormat | null {
  if (value === undefined) return "standard"
  return FORMATS_OF[edition].find((format) => format === value) ?? null
}

function parseLeader(value: unknown): DeckLeader | null | undefined {
  if (value === null) return null
  if (!isRecord(value) || !isString(value["cardId"])) return undefined
  return { cardId: value["cardId"], ...optionalPrinting(value) }
}

/**
 * Reads one stored record. Browser storage is untyped and a record can be damaged or come from
 * another build, so anything that does not match exactly is refused as a whole.
 */
export function parseDeck(value: unknown): Deck | null {
  if (!isRecord(value)) return null
  const { id, name, edition, createdAt, updatedAt, revision } = value
  const basis = parseBasis(value["basis"])
  const leader = parseLeader(value["leader"])
  const main = parseEntries(value["main"])
  const evolve = parseEntries(value["evolve"])
  const side = parseEntries(value["side"])
  if (!isString(id) || !isString(name) || !isTime(createdAt) || !isTime(updatedAt)) return null
  if (edition !== "jp" && edition !== "en") return null
  const format = parseFormat(value["format"], edition)
  if (format === null) return null
  if (typeof revision !== "number" || !Number.isInteger(revision) || revision < 1) return null
  if (basis === null || leader === undefined) return null
  if (main === null || evolve === null || side === null) return null
  return {
    id,
    name,
    edition,
    format,
    basis,
    leader,
    main,
    evolve,
    side,
    createdAt,
    updatedAt,
    revision,
  }
}
