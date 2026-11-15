import type { UiLanguage } from "../../i18n/languages"
import type { RulesBundle } from "./bundle"
import type { RulesRegion } from "./model"

/**
 * The edition a reader sees: English readers get the English rules; the Chinese view translates
 * the Japanese rules, so it reads that edition, as the Japanese view does.
 */
export const rulesRegionFor = (language: UiLanguage): RulesRegion =>
  language === "en" ? "en" : "jp"

/** Each edition is its own chunk: only the rules page or a preview pays for downloading it. */
export async function loadComprehensiveRules(region: RulesRegion): Promise<RulesBundle> {
  const { RULES } = region === "en" ? await import("./bundle-en") : await import("./bundle-jp")
  return RULES
}
