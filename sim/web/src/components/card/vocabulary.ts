import { type Catalog, type Row, stringValue } from "../../data"
import type { DisplayedText } from "../../domain/nameDisplay"
import type { TextLang } from "../../domain/search"

type CardIndex = Catalog["index"]

// Match Catalog.classLabel's fallback without exposing private data-layer helpers.
export type VocabularyKind = "class" | "type" | "trait"

export function vocabularyLabel(
  index: CardIndex,
  kind: VocabularyKind,
  code: string,
  lang: TextLang,
): string {
  const row = index.vocabulary(kind, code)
  return (row ? rowLabel(index, row, "label_unit_id", lang) : undefined)?.text ?? code
}

/** A labelled row's translation into `lang`, else its original text (sets, vocabulary). */
export function rowLabel(
  index: CardIndex,
  row: Row,
  unitField: string,
  lang: TextLang,
): DisplayedText | undefined {
  for (const entry of row["translations"] as Row[]) {
    if (entry["target_lang"] !== lang) continue
    const translation = index.translation(stringValue(entry["translation_id"]))
    const unit = translation ? index.textUnit(stringValue(translation["text_unit_id"])) : undefined
    if (unit) return { text: stringValue(unit["text"]), lang }
  }
  const unit = index.textUnit(stringValue(row[unitField]))
  return unit
    ? { text: stringValue(unit["text"]), lang: stringValue(unit["lang"]) as TextLang }
    : undefined
}
