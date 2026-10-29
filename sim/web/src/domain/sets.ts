import type { DisplayedText, NameSource } from "./nameDisplay"
import { compareCodePoints } from "./normalize"
import { editionUrl } from "./query/codec"
import type { Region, TextLang } from "./search"

/** A set name in the interface's text language, else the original. */
export function setLabel(name: NameSource, lang: TextLang): DisplayedText {
  const translated = name.translations[lang]
  return translated ? { text: translated, lang } : name.original
}

export interface SetCaption {
  /** The public code to show before the name, or null when the name already starts with it. */
  readonly code: string | null
  readonly name: DisplayedText
  /** Code and name as one string, for labels that take plain text. */
  readonly text: string
}

// One caption rule for every list, dropdown, header and chip (maintainer, 2026-10-18): a name that
// already starts with the public code is shown alone, so the code is never printed twice.
export function setCaption(
  set: { readonly code: string; readonly publicCode?: string; readonly name: NameSource },
  lang: TextLang,
): SetCaption {
  const publicCode = set.publicCode ?? set.code
  const name = setLabel(set.name, lang)
  const code = name.text.startsWith(publicCode) ? null : publicCode
  return { code, name, text: code === null ? name.text : `${code} ${name.text}` }
}

// The kinds with a section of their own on `/sets` (architecture §2.1); every other kind, known or
// not, lands in "decks and special products" so a new kind in the snapshot is never dropped.
const MAIN_SET_KINDS = ["booster", "collaboration", "special_pack", "promo"] as const
type MainSetKind = (typeof MAIN_SET_KINDS)[number]

export interface GroupedSets<T> {
  readonly main: ReadonlyMap<MainSetKind, readonly T[]>
  readonly other: readonly T[]
}

function isMainSetKind(kind: string | undefined): kind is MainSetKind {
  return (MAIN_SET_KINDS as readonly string[]).includes(kind ?? "")
}

/** Splits sets into the main sections (in `MAIN_SET_KINDS` order, present kinds only) and the rest. */
export function groupSets<T extends { readonly kind?: string }>(
  sets: readonly T[],
): GroupedSets<T> {
  const main = new Map<MainSetKind, T[]>()
  const other: T[] = []
  for (const set of sets) {
    if (isMainSetKind(set.kind)) {
      const bucket = main.get(set.kind)
      if (bucket) bucket.push(set)
      else main.set(set.kind, [set])
    } else other.push(set)
  }
  const ordered = new Map<MainSetKind, readonly T[]>()
  for (const kind of MAIN_SET_KINDS) {
    const bucket = main.get(kind)
    if (bucket) ordered.set(kind, bucket)
  }
  return { main: ordered, other }
}

// The home page's set entries (#519). Product release dates only order sets and are shown; they
// never decide which printings a set holds (`set=` stays on the printing's home set).
export interface SetFacts {
  readonly id: string
  readonly publicCode: string
  readonly kind: string
}

export interface ProductFacts {
  /** All sets this product belongs to; empty when no links are recorded. */
  readonly familyIds: readonly string[]
  readonly region: string
  /** ISO day, or null when the product has no day-precision date. */
  readonly releasedOn: string | null
}

/**
 * Each set with its first release in the region. A set counts only through a product that links
 * to it and has a known day; a set without one is left out rather than dated by its code.
 */
export function datedSets<T extends SetFacts>(
  sets: readonly T[],
  products: readonly ProductFacts[],
  region: Region,
): (T & { readonly releasedOn: string })[] {
  const firstRelease = new Map<string, string>()
  for (const product of products) {
    if (product.region !== region || product.releasedOn === null) continue
    for (const familyId of product.familyIds) {
      const known = firstRelease.get(familyId)
      if (known === undefined || product.releasedOn < known)
        firstRelease.set(familyId, product.releasedOn)
    }
  }
  return sets.flatMap((set) => {
    const releasedOn = firstRelease.get(set.id)
    return releasedOn === undefined ? [] : [{ ...set, releasedOn }]
  })
}

/** The newest booster and collaboration sets, newest first. */
export function recentSets<T extends SetFacts & { readonly releasedOn: string }>(
  sets: readonly T[],
  limit: number,
): T[] {
  return sets
    .filter((set) => set.kind === "booster" || set.kind === "collaboration")
    .sort(
      (a, b) =>
        compareCodePoints(b.releasedOn, a.releasedOn) ||
        compareCodePoints(b.publicCode, a.publicCode),
    )
    .slice(0, limit)
}

export interface PackCounts {
  /** Distinct cards among the matching printings. */
  readonly cards: number
}

export function setCardsPath(code: string, edition: Region = "jp"): string {
  return editionUrl(`/cards?set=${encodeURIComponent(code.toLowerCase())}`, edition)
}
