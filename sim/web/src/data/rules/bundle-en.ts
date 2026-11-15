import { buildRules } from "./bundle"
import type { RulesFile, RulesIndex } from "./model"

// Its own chunk, so English readers never download the Japanese text and its translation.
// The English rules are shown as published; nothing translates them.
export const RULES = buildRules(
  import.meta.glob<RulesFile | RulesIndex>(
    "../../../../../authored/rules/comprehensive/en/*/*.json",
    { import: "default", eager: true },
  ),
  {},
)
