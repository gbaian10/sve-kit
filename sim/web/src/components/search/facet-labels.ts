import type { TFunction } from "i18next"

import type { NameSource } from "../../domain/nameDisplay"
import type { TextLang } from "../../domain/search"

const LOCATION_KEYS = [
  "main",
  "evolve",
  "advance",
  "leader",
  "token",
  "ep",
  "sep",
  "gold",
  "white",
  "unknown",
] as const
type LocationKey = (typeof LOCATION_KEYS)[number]
const known = (key: string): key is LocationKey =>
  (LOCATION_KEYS as readonly string[]).includes(key)

/** A deck-location token's label: UI copy, never the vocabulary (the maintainer wants "Token"). */
export function locationLabel(t: TFunction, token: string): string {
  const [parent = "", child] = token.split(":")
  if (child === undefined) return known(parent) ? t(`locationFilter.${parent}`) : parent
  // Token kinds are typed by the caller (vocabulary); only leader frames have UI copy.
  return parent === "leader" && known(child) ? t(`locationFilter.${child}`) : child
}

/** A vocabulary option's label in the UI's text language, falling back to the original. */
export function vocabularyOptionLabel(name: NameSource, textLang: TextLang): string {
  return name.translations[textLang] ?? name.original.text
}
