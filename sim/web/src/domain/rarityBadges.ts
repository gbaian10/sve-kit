import type { Region } from "./search"

export const BADGE_STYLES = [
  "bronze",
  "silver",
  "gold",
  "red",
  "pastel",
  "rainbow",
  "none",
] as const
type BadgeStyle = (typeof BADGE_STYLES)[number]
export type RarityBadge = { readonly style: BadgeStyle; readonly text?: string }

export interface RarityBadgeInput {
  readonly rarityCode?: string | null
  readonly premium?: boolean | null
  readonly region: Region
  readonly rarityBadge?: RarityBadge | null
}

const BASE_BADGES = {
  br: "br",
  sr: "sr",
  gr: "gr",
  lg: "lg",
  sl: "sl",
  ur: "ur",
  sp: "sp",
  ssp: "ssp",
} as const
const EN_BADGES = { br: "br-en", sr: "sr-en", gr: "gr-en", lg: "lg-en", ur: "ur-en" } as const
const APPEARANCE_BADGES = {
  "bronze:BR": "br",
  "bronze:B": "br-en",
  "bronze:PR": "pr-bronze",
  "silver:SR": "sr",
  "silver:S": "sr-en",
  "silver:PR": "pr-silver",
  "gold:GR": "gr",
  "gold:G": "gr-en",
  "gold:PR": "pr-gold",
  "red:LG": "lg",
  "red:L": "lg-en",
  "red:PR": "pr-red",
  "pastel:SL": "sl",
  "pastel:UR": "ur",
  "pastel:U": "ur-en",
  "pastel:PR": "pr-pastel",
  "rainbow:SP": "sp",
  "rainbow:SSP": "ssp",
  "rainbow:PR": "pr-rainbow",
} as const
export type RarityBadgeAsset =
  | (typeof BASE_BADGES)[keyof typeof BASE_BADGES]
  | (typeof EN_BADGES)[keyof typeof EN_BADGES]
  | (typeof APPEARANCE_BADGES)[keyof typeof APPEARANCE_BADGES]

function lookup<T>(map: Readonly<Record<string, T>>, key: string): T | undefined {
  return Object.hasOwn(map, key) ? map[key] : undefined
}

/** Face appearance affects display only; callers retain the official code for search and sorting. */
export function selectRarityBadges({
  rarityCode,
  region,
  rarityBadge,
}: RarityBadgeInput): readonly RarityBadgeAsset[] {
  const code = rarityCode?.toLowerCase() ?? ""
  if (rarityBadge?.style === "none") return []
  if (code === "-" && !rarityBadge) return []
  const face = rarityBadge
    ? lookup(APPEARANCE_BADGES, `${rarityBadge.style}:${rarityBadge.text ?? ""}`)
    : undefined
  const base =
    face ?? (region === "en" ? lookup(EN_BADGES, code) : undefined) ?? lookup(BASE_BADGES, code)
  return base ? [base] : []
}
