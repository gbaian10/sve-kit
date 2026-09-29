import { normalizeText } from "./normalize"

// Card numbers look like `BP01-051`, `BP01-051EN`, `SD01-L01`, `BP01-002a`, `PR-001`. Users type
// them loosely (`bp01-51`, `BP01 051`); the comparison key folds case and width, drops separators,
// splits off the set code, then a letter prefix (`L` for leaders, `T` for tokens), the number
// without leading zeros and a letter suffix. Without separators the set code is ambiguous
// (`pr001` vs `bp01051`), so callers pass the snapshot's set codes when they have them; the fallback
// takes two digits after the letters except for the digit-less promo code.
const TAIL = /^([a-z]*)(\d+)([a-z]*)$/u
const SETS_WITHOUT_DIGITS = new Set(["pr"])

function splitSet(flat: string, sets: ReadonlySet<string> | undefined): [string, string] | null {
  if (sets) {
    for (let length = Math.min(flat.length, 6); length > 0; length -= 1) {
      const candidate = flat.slice(0, length)
      if (sets.has(candidate)) return [candidate, flat.slice(length)]
    }
    return null
  }
  const letters = /^[a-z]+/u.exec(flat)?.[0]
  if (letters === undefined) return null
  const digits = SETS_WITHOUT_DIGITS.has(letters)
    ? ""
    : (/^\d{2}/u.exec(flat.slice(letters.length))?.[0] ?? "")
  return [letters + digits, flat.slice(letters.length + digits.length)]
}

export interface CardNoKey {
  readonly set: string
  /** Letter prefix plus the number without leading zeros, e.g. `51` or `l1`. */
  readonly number: string
  readonly suffix: string
}

/** The comparison key of a card number, or null when the text does not look like one. */
export function cardNoKey(text: string, sets?: ReadonlySet<string>): CardNoKey | null {
  const parts = splitSet(normalizeText(text), sets)
  if (!parts) return null
  const match = TAIL.exec(parts[1])
  if (!match) return null
  return {
    set: parts[0],
    number: `${match[1] ?? ""}${String(Number(match[2] ?? "0"))}`,
    suffix: match[3] ?? "",
  }
}

/** Whether a typed card number matches a printed one under the loose rules. */
export function cardNoMatches(typed: string, printed: string, sets?: ReadonlySet<string>): boolean {
  const a = cardNoKey(typed, sets)
  const b = cardNoKey(printed, sets)
  return (
    a !== null && b !== null && a.set === b.set && a.number === b.number && a.suffix === b.suffix
  )
}

/** A stable lookup key (`bp01|51|en`) so many printings can be indexed once. */
export function cardNoLookupKey(text: string, sets?: ReadonlySet<string>): string | null {
  const key = cardNoKey(text, sets)
  return key === null ? null : `${key.set}|${key.number}|${key.suffix}`
}

/** True when `typed` is a prefix of the printed number's key (set or set + leading digits). */
export function cardNoPrefixMatches(typed: string, printed: string): boolean {
  const flat = normalizeText(typed)
  if (flat === "") return false
  const printedFlat = normalizeText(printed)
  return printedFlat.startsWith(flat)
}
