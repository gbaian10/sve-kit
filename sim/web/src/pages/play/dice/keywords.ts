// Keywords as the engine names them. The ones with a die of their own are drawn as dice; the rest
// are written out.
const DIE_KEYWORDS = ["guard", "intimidate", "bane", "drain", "aura", "assail"] as const
export type DieKeyword = (typeof DIE_KEYWORDS)[number]
export const isDieKeyword = (keyword: string): keyword is DieKeyword =>
  (DIE_KEYWORDS as readonly string[]).includes(keyword)

const NAMED_KEYWORDS = [...DIE_KEYWORDS, "storm", "rush"] as const
export type NamedKeyword = (typeof NAMED_KEYWORDS)[number]
/** Keywords we have a word for; any other arrives on screen as the engine wrote it. */
export const isNamedKeyword = (keyword: string): keyword is NamedKeyword =>
  (NAMED_KEYWORDS as readonly string[]).includes(keyword)
