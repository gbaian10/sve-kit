import { createContext } from "react"

import type { RulesRegion } from "../../data"

/** The `lang` of an edition's original text. */
export const RULES_LANG: Readonly<Record<RulesRegion, string>> = { jp: "ja", en: "en" }

/** Anchor id of a node: the key with `:` made URL-friendly (`B-1:6.1.1.1` → `r-B-1-6.1.1.1`). */
export const ruleAnchor = (key: string) => `r-${key.replace(":", "-")}`

/**
 * The clause keys a cited number may link to. The rules page provides every key of its version;
 * a preview provides none, so text inside it stays plain.
 */
export const RuleKeysContext = createContext<{ readonly keys: ReadonlySet<string> }>({
  keys: new Set(),
})
