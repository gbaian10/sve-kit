import { cardNoKey, cardNoLookupKey } from "./cardNo"
import { normalizeText } from "./normalize"
import type { MatchedField, Region, SearchEntry, SearchName, SearchOptions } from "./search"

export interface Match {
  readonly field: MatchedField
  /** Lower is better: 0 exact/loose card number, 1 number prefix, 2 name prefix, 3 name contains. */
  readonly rank: number
  readonly printingId: string
}

/** The printing a card opens as: the edition's default, else the other region's, else the first. */
export function representativePrinting(entry: SearchEntry, edition: Region): string | undefined {
  return (
    entry.defaultPrinting[edition] ??
    entry.defaultPrinting[edition === "jp" ? "en" : "jp"] ??
    entry.printings[0]?.id
  )
}

function textMatch(
  names: readonly SearchName[],
  flat: string,
): { readonly rank: 2 | 3; readonly index: number } | null {
  let contains = -1
  for (const [index, name] of names.entries()) {
    if (name.normalized === "") continue
    if (name.normalized.startsWith(flat)) return { rank: 2, index }
    if (contains === -1 && name.normalized.includes(flat)) contains = index
  }
  return contains === -1 ? null : { rank: 3, index: contains }
}

/** How one card matches the typed text, or null; the text must already be non-empty.
 */
export function matchEntry(text: string, entry: SearchEntry, options: SearchOptions): Match | null {
  const flat = normalizeText(text)
  if (flat === "") return null
  const typedKey = cardNoLookupKey(text, options.sets)
  if (typedKey !== null) {
    const exact = entry.printings.find((printing) => printing.lookupKey === typedKey)
    if (exact) return { field: "cardNo", rank: 0, printingId: exact.id }
  }
  // Digits alone (`51`, `051`) are the number part of any set (design: "part of the number");
  // a whole-number hit counts as an exact one, a shorter run of digits as a prefix.
  const digits = /^\d+$/u.test(flat) ? String(Number(flat)) : null
  if (digits !== null) {
    const whole = entry.printings.find(
      (printing) => printing.key !== null && printing.key.number === digits,
    )
    if (whole) return { field: "cardNo", rank: 0, printingId: whole.id }
  }
  // `bp01-5` should list BP01-050…059 too: leading zeros make a plain prefix miss, so a typed
  // number is compared by set and number digits; text without a number falls back to the flat form.
  const typed = cardNoKey(text, options.sets)
  const prefix = entry.printings.find(
    (printing) =>
      printing.flat.startsWith(flat) ||
      (typed !== null &&
        printing.key !== null &&
        printing.key.set === typed.set &&
        printing.key.number.startsWith(typed.number) &&
        typed.suffix === "") ||
      (digits !== null && printing.key !== null && printing.key.number.startsWith(digits)),
  )
  if (prefix) return { field: "cardNo", rank: 1, printingId: prefix.id }
  const representative = representativePrinting(entry, options.edition)
  if (representative === undefined) return null
  const name = textMatch(entry.names, flat)
  const alias = textMatch(entry.aliases, flat)
  if (name !== null && (alias === null || name.rank <= alias.rank))
    return { field: "name", rank: name.rank, printingId: representative }
  if (alias !== null) return { field: "alias", rank: alias.rank, printingId: representative }
  return null
}

/** Ranked suggestions for the typed text; the order is the sequence prev/next follows (§2.2). */
