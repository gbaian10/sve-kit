import type { Region } from "./search"

// DeckLog (Bushiroad's deck list site) keeps one system per edition; a code from one is unknown, or
// a different deck, in the other. Its API allows no cross-origin reads, so our Worker looks codes up.

/** `GET {DECKLOG_PATH}/{jp|en}/{code}` answers a `DeckLogDeck` or `{ error: DeckLogError }`. */
export const DECKLOG_PATH = "/api/decklog"

export const DECKLOG_ORIGINS: Readonly<Record<Region, string>> = {
  jp: "https://decklog.bushiroad.com",
  en: "https://decklog-en.bushiroad.com",
}

// Codes seen so far are 4–6 upper-case letters and digits; DeckLog's own input allows up to ten.
const CODE = /^[0-9A-Z]{4,10}$/u
const DECKLOG_URL =
  /^(?:https?:\/\/)?decklog(-en)?\.bushiroad\.com\/(?:view|copy)\/([0-9A-Za-z]{4,10})\/?(?:[?#]\S*)?$/iu

/** A code typed or pasted in any case; DeckLog issues upper-case codes. */
export function normalizeDeckLogCode(text: string): string | undefined {
  const code = text.trim().toUpperCase()
  return CODE.test(code) ? code : undefined
}

export interface DeckLogReference {
  /** Set by a DeckLog link; a bare code leaves the edition to the importer. */
  readonly region: Region | undefined
  readonly code: string
}

/** A whole input that is a DeckLog link or a bare code; anything else is not one. */
export function deckLogReferenceOf(text: string): DeckLogReference | undefined {
  const trimmed = text.trim()
  const link = DECKLOG_URL.exec(trimmed)
  if (link) {
    const code = normalizeDeckLogCode(link[2] ?? "")
    return code === undefined ? undefined : { region: link[1] ? "en" : "jp", code }
  }
  const code = normalizeDeckLogCode(trimmed)
  return code === undefined ? undefined : { region: undefined, code }
}

/** One printing of a DeckLog list, by the card number DeckLog shows. */
export interface DeckLogCard {
  readonly cardNo: string
  readonly count: number
}

export interface DeckLogDeck {
  readonly region: Region
  readonly code: string
  /** DeckLog's construction code (`N` Standard, `X` Crossover, `G` Gloryfinder), if any. */
  readonly format: string | null
  /** The class or title DeckLog shows for the deck, in the edition's language. */
  readonly className: string | null
  readonly leader: readonly DeckLogCard[]
  readonly main: readonly DeckLogCard[]
  readonly evolve: readonly DeckLogCard[]
}

export const DECKLOG_ERRORS = [
  "invalid-code",
  "not-found",
  "not-sve",
  "rate-limited",
  "upstream-unavailable",
  "upstream-invalid",
  "not-configured",
  "store-unavailable",
  "server-error",
] as const

export type DeckLogError = (typeof DECKLOG_ERRORS)[number]

// DeckLog's card numbers as seen in both editions: `BP09-LD02`, `PR-339`, `BP09-R047EN`, `SDD03-LD01EN`.
const CARD_NO = /^[A-Z0-9][A-Za-z0-9]*-[A-Za-z0-9Ⓢ]{1,16}$/u
const MAX_COPIES = 60
const MAX_ROWS = 120

/** A list section in DeckLog's shape or ours, or `undefined` unless every row is well formed. */
export function readDeckLogCards(value: unknown): DeckLogCard[] | undefined {
  if (!Array.isArray(value) || value.length > MAX_ROWS) return undefined
  const cards: DeckLogCard[] = []
  for (const row of value as unknown[]) {
    if (typeof row !== "object" || row === null) return undefined
    const record = row as Record<string, unknown>
    const cardNo = record["card_number"] ?? record["cardNo"]
    const count = record["num"] ?? record["count"]
    if (
      typeof cardNo !== "string" ||
      cardNo.length > 32 ||
      !CARD_NO.test(cardNo) ||
      typeof count !== "number" ||
      !Number.isInteger(count) ||
      count < 1 ||
      count > MAX_COPIES
    )
      return undefined
    cards.push({ cardNo, count })
  }
  return cards
}
