import { buildRules } from "./bundle"
import type { RulesFile, RulesIndex, RulesTranslationFile } from "./model"

// Imported dynamically: the Japanese text and its translation (about 60 KB gzip) are one chunk,
// fetched only by the rules page or a rule preview and shipped with the site, not the snapshot.
export const RULES = buildRules(
  import.meta.glob<RulesFile | RulesIndex>(
    "../../../../../authored/rules/comprehensive/jp/*/*.json",
    { import: "default", eager: true },
  ),
  import.meta.glob<RulesTranslationFile>(
    "../../../../../authored/translations/comprehensive-rules/zh-TW/*/*.json",
    { import: "default", eager: true },
  ),
)
