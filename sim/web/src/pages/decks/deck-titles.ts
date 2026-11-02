import { arrayValue, type Catalog, nameOf, objectValue, type Row, stringValue } from "../../data"
import type { Region, TextLang } from "../../domain/search"

export interface TitleOption {
  readonly code: string
  /** How many cards of the edition carry the title. */
  readonly count: number
}

const strings = (value: unknown): string[] =>
  Array.isArray(value)
    ? (value as unknown[]).filter((item): item is string => typeof item === "string")
    : []

/** Title (作品名) codes on the edition's cards, most cards first. */
export function titleOptions(catalog: Catalog, edition: Region): TitleOption[] {
  const index = catalog.index
  const counts = new Map<string, number>()
  for (const card of index.cards) {
    const front = index.facesOf(stringValue(card["id"]))[0]
    if (!front) continue
    const revision = index.displayRevision(stringValue(front["id"]), edition)
    for (const title of strings(revision?.["titles"]))
      counts.set(title, (counts.get(title) ?? 0) + 1)
  }
  return [...counts]
    .map(([code, count]) => ({ code, count }))
    .sort((a, b) => b.count - a.count || a.code.localeCompare(b.code))
}

/** A product family's name: the original (`lang` null) or its translation into `lang`. */
function familyText(catalog: Catalog, family: Row, lang: TextLang | null): string | undefined {
  const index = catalog.index
  if (lang === null) {
    const unit = index.textUnit(stringValue(family["name_unit_id"]))
    return unit ? stringValue(unit["text"]) : undefined
  }
  for (const raw of arrayValue(family["translations"] ?? [])) {
    const entry = objectValue(raw)
    if (entry["target_lang"] !== lang) continue
    const translation = index.translation(stringValue(entry["translation_id"]))
    const unit = translation ? index.textUnit(stringValue(translation["text_unit_id"])) : undefined
    if (unit) return stringValue(unit["text"])
  }
  return undefined
}

export function titleLabel(catalog: Catalog, code: string, lang: TextLang): string {
  const row = catalog.index.vocabulary("title", code)
  const name = row ? nameOf(catalog.index, row, "label") : undefined
  if (!name) return code
  const translated = name.translations[lang]
  if (translated) return translated
  // The title vocabulary carries English only. Collaboration packs are named after their title in
  // every language (合作補充包「賽馬娘 Pretty Derby」), so the adopted name is the pack's quoted text.
  for (const family of catalog.index.families) {
    if (family["kind"] !== "collaboration") continue
    if (!familyText(catalog, family, null)?.includes(name.original.text)) continue
    const packName = familyText(catalog, family, lang)
    if (packName === undefined) continue
    const quoted = /「([^」]+)」|“([^”]+)”|"([^"]+)"/u.exec(packName)
    return quoted?.[1] ?? quoted?.[2] ?? quoted?.[3] ?? packName
  }
  return name.original.text
}
