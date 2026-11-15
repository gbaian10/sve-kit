/** Comprehensive rules data under authored/rules/comprehensive/ and its translations. */

/** `main` is the rules body; appendix B sections restate body numbers, so each is its own context. */
type RuleContext = "main" | "A" | "B-1" | "B-2" | "C"

interface RuleTable {
  readonly header: readonly string[]
  readonly rows: readonly (readonly string[])[]
}

interface RuleFigure {
  readonly page: number
  readonly caption: string
}

export interface RuleNode {
  /** Unique across the version: the body number itself, or `<context>:<number>` in appendices. */
  readonly key: string
  readonly context: RuleContext
  /** The official number exactly as printed; absent for unnumbered appendix blocks. */
  readonly number?: string
  readonly kind: "chapter" | "section" | "rule" | "block"
  readonly depth: number
  readonly title?: string
  readonly text: string
  /** Omitted when the rule has no examples; the source hash treats that as `[]`. */
  readonly examples?: readonly string[]
  readonly table?: RuleTable
  readonly figure?: RuleFigure
  /** Printed in red: the official marker for text updated in this version. */
  readonly changed?: true
}

export interface RulesFile {
  readonly format: 1
  readonly kind: "comprehensive-rules"
  readonly version: string
  readonly nodes: readonly RuleNode[]
}

interface TranslatedTable {
  readonly header: readonly (string | null)[]
  readonly rows: readonly (readonly (string | null)[])[]
}

/** null marks one untranslated example or cell; the page falls back to the original there. */
export interface TranslationEntry {
  readonly title?: string
  readonly text?: string
  readonly examples?: readonly (string | null)[]
  readonly table?: TranslatedTable
  /** Optional only until `bun run rules:stamp` fills it; the data test requires it. */
  readonly source_sha256?: string
}

export interface RulesTranslationFile {
  readonly format: 1
  readonly kind: "comprehensive-rules-translation"
  readonly version: string
  readonly language: "zh-TW"
  readonly entries: Readonly<Record<string, TranslationEntry>>
}
