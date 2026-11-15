import type { RulesBundle } from "./bundle"

/** The rules text is its own chunk: only the rules page pays for downloading it. */
export async function loadComprehensiveRules(): Promise<RulesBundle> {
  const { RULES } = await import("./bundle")
  return RULES
}
