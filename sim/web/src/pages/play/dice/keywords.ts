// Keywords as the engine names them. The ones with a die of their own are drawn as dice. Storm and
// rush have no official die and no marker on the card: what they allow shows as the colour of the
// card's frame whenever the card may attack. Any other keyword is written out.
const DIE_KEYWORDS = ["guard", "intimidate", "bane", "drain", "aura", "assail"] as const
export type DieKeyword = (typeof DIE_KEYWORDS)[number]
export const isDieKeyword = (keyword: string): keyword is DieKeyword =>
  (DIE_KEYWORDS as readonly string[]).includes(keyword)

const UNMARKED_KEYWORDS = ["storm", "rush"] as const
export const isUnmarkedKeyword = (keyword: string): boolean =>
  (UNMARKED_KEYWORDS as readonly string[]).includes(keyword)

const NAMED_KEYWORDS = [...DIE_KEYWORDS, ...UNMARKED_KEYWORDS] as const
export type NamedKeyword = (typeof NAMED_KEYWORDS)[number]
/** Keywords we have a word for; any other arrives on screen as the engine wrote it. */
export const isNamedKeyword = (keyword: string): keyword is NamedKeyword =>
  (NAMED_KEYWORDS as readonly string[]).includes(keyword)
