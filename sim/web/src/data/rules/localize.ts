import type { RuleNode, TranslationEntry } from "./model"

/** One displayed piece: the translation when there is one, otherwise the Japanese original. */
export interface LocalizedText {
  readonly text: string
  readonly translated: boolean
}

export interface LocalizedNode {
  readonly node: RuleNode
  readonly title?: LocalizedText
  readonly text: LocalizedText
  readonly examples: readonly LocalizedText[]
  readonly table?: {
    readonly header: readonly LocalizedText[]
    readonly rows: readonly (readonly LocalizedText[])[]
  }
  /** Some non-empty original piece is still shown untranslated ("譯文準備中"). */
  readonly pending: boolean
}

function pick(original: string, translation: string | null | undefined): LocalizedText {
  // An empty original (a heading-only node's body) needs no translation to be complete.
  if (original === "") return { text: "", translated: true }
  return translation
    ? { text: translation, translated: true }
    : { text: original, translated: false }
}

/** Falls back per piece, down to a single example or table cell, so a partial entry still shows. */
export function localizeNode(node: RuleNode, entry: TranslationEntry | undefined): LocalizedNode {
  const title = node.title === undefined ? undefined : pick(node.title, entry?.title)
  const text = pick(node.text, entry?.text)
  const examples = node.examples.map((example, index) => pick(example, entry?.examples?.[index]))
  const table = node.table && {
    header: node.table.header.map((cell, index) => pick(cell, entry?.table?.header[index])),
    rows: node.table.rows.map((row, rowIndex) =>
      row.map((cell, index) => pick(cell, entry?.table?.rows[rowIndex]?.[index])),
    ),
  }
  const pieces = [
    ...(title ? [title] : []),
    text,
    ...examples,
    ...(table ? [...table.header, ...table.rows.flat()] : []),
  ]
  return {
    node,
    ...(title ? { title } : {}),
    text,
    examples,
    ...(table ? { table } : {}),
    pending: pieces.some((piece) => !piece.translated),
  }
}

/** The original only: what the Japanese view, and the English view's original toggle, render. */
export function originalNode(node: RuleNode): LocalizedNode {
  return localizeNode(node, undefined)
}
