import type { RuleContext } from "./model"

export interface RuleReference {
  /** UTF-16 offsets of the number itself inside the text, for linking just that span. */
  readonly start: number
  readonly end: number
  readonly number: string
}

// A clause number has at least one dot, so "（11章）" or "（計6枚）" never match; a list such as
// "（10.5.1、10.5.2）" yields one reference per number.
const GROUP = /（(\d+(?:\.\d+)+(?:[、・～,]\s*\d+(?:\.\d+)+)*)）/gu
const NUMBER = /\d+(?:\.\d+)+/gu

/**
 * Clause numbers cited in full-width parentheses, in text order.
 * @public 待接：#2227 S4 的 RuleRef 以此切出引用；本片先供資料測試驗證每個引用都解析得到。
 */
export function findRuleReferences(text: string): RuleReference[] {
  const found: RuleReference[] = []
  for (const group of text.matchAll(GROUP)) {
    const inner = group[1] ?? ""
    const base = group.index + 1
    for (const number of inner.matchAll(NUMBER))
      found.push({
        start: base + number.index,
        end: base + number.index + number[0].length,
        number: number[0],
      })
  }
  return found
}

/**
 * The node key a citation means: inside appendix B-n its own restated clause first, otherwise
 * the rules body. Undefined when neither exists, so the caller leaves the text unlinked.
 * @public 待接：#2227 S4 的 RuleRef 以此決定跳轉目標。
 */
export function resolveRuleReference(
  number: string,
  context: RuleContext,
  keys: ReadonlySet<string>,
): string | undefined {
  if (context === "B-1" || context === "B-2") {
    const local = `${context}:${number}`
    if (keys.has(local)) return local
  }
  return keys.has(number) ? number : undefined
}
