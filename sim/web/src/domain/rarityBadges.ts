import type { Region } from "./search"

export const RARITY_BADGE_HEIGHT = 11
export const RARITY_COLUMN_MIN_WIDTH = Math.ceil((RARITY_BADGE_HEIGHT * 56) / 21) + 10

export const BADGE_STYLES = ["br", "sr", "gr", "lg", "sl", "sp", "none"] as const
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
  "br:BR": "br",
  "br:B": "br-en",
  "br:PR": "pr-bronze",
  "sr:SR": "sr",
  "sr:S": "sr-en",
  "sr:PR": "pr-silver",
  "gr:GR": "gr",
  "gr:G": "gr-en",
  "gr:PR": "pr-gold",
  "lg:LG": "lg",
  "lg:L": "lg-en",
  "lg:PR": "pr-red",
  "sl:SL": "sl",
  "sl:UR": "ur",
  "sl:U": "ur-en",
  "sl:PR": "pr-pastel",
  "sp:SP": "sp",
  "sp:SSP": "ssp",
  "sp:PR": "pr-rainbow",
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
