import type { RuleContext } from "./model"

export interface RuleReference {
  /** UTF-16 offsets of the number itself inside the text, for linking just that span. */
  readonly start: number
  readonly end: number
  readonly number: string
}

// Any dotted number standing alone: "（10.5.1）", "（5.16.1.2を適用した後）", "詳しくは10.5.2を参照",
// both ends of "7.2から7.4" or "10.9.1.2-10.9.1.4". At least one dot, so "（11章）" or "（計6枚）" never
// match; a version label ("ver 1.28.0") is not a clause. Callers link only numbers that resolve.
const NUMBER = /(?<![\d.])\d+(?:\.\d+)+(?![\d]|\.\d)/gu
const VERSION_BEFORE = /ver\.?\s*$/iu

/**
 * Clause numbers cited anywhere in the text, in text order.
 * @public 待接：#2227 S4 的 RuleRef 以此切出引用；本片先供資料測試驗證每個引用都解析得到。
 */
export function findRuleReferences(text: string): RuleReference[] {
  const found: RuleReference[] = []
  for (const number of text.matchAll(NUMBER)) {
    if (VERSION_BEFORE.test(text.slice(Math.max(0, number.index - 5), number.index))) continue
    found.push({ start: number.index, end: number.index + number[0].length, number: number[0] })
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
