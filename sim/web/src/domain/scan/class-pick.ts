import { NEUTRAL_CLASS } from "../query/model"

/**
 * None picked means every class. The first class picked brings neutral along, since most decks
 * run neutral cards; tapping neutral again drops it.
 */
export function toggleClass(classes: readonly string[], code: string): string[] {
  if (classes.includes(code)) return classes.filter((item) => item !== code)
  return classes.length === 0 && code !== NEUTRAL_CLASS ? [code, NEUTRAL_CLASS] : [...classes, code]
}
