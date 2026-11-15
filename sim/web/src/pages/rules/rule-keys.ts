import { createContext, type ReactNode } from "react"

/** Anchor id of a node: the key with `:` made URL-friendly (`B-1:6.1.1.1` → `r-B-1-6.1.1.1`). */
export const ruleAnchor = (key: string) => `r-${key.replace(":", "-")}`

export interface RuleLinkTarget {
  readonly key: string
  readonly number: string
}

/** What a cited number needs to become a link; the page provides it once for all its text. */
export const RuleKeysContext = createContext<{
  readonly keys: ReadonlySet<string>
  readonly renderLink?: (target: RuleLinkTarget, label: string) => ReactNode
}>({ keys: new Set() })
