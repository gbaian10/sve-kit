import { type Catalog, type Row, stringValue } from "../../data"
import type { TextLang } from "../../domain/search"

type CardIndex = Catalog["index"]

// Match Catalog.classLabel's fallback without exposing private data-layer helpers.
export type VocabularyKind = "class" | "type" | "trait" | "rarity"

export function vocabularyLabel(
  index: CardIndex,
  kind: VocabularyKind,
  code: string,
  lang: TextLang,
): string {
  const row = index.vocabulary(kind, code)
  if (!row) return code
  for (const entry of row["translations"] as Row[]) {
    if (entry["target_lang"] !== lang) continue
    const translation = index.translation(stringValue(entry["translation_id"]))
    const unit = translation ? index.textUnit(stringValue(translation["text_unit_id"])) : undefined
    if (unit) return stringValue(unit["text"])
  }
  const unit = index.textUnit(stringValue(row["label_unit_id"]))
  return unit ? stringValue(unit["text"]) : code
}
