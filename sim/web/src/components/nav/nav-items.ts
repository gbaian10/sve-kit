import { House, Layers, Search } from "lucide-react"

export type NavItemKey = "home" | "cards" | "decks"

export interface NavItem {
  readonly key: NavItemKey
  readonly to: string
  readonly Icon: typeof House
  /** Matches nested routes too, so `/cards/BP01-001` still lights up the cards tab. */
  readonly end: boolean
}

// Three tabs for R1 (design 03e, decision A); the battle tab is added when /play ships.
export const NAV_ITEMS: readonly NavItem[] = [
  { key: "home", to: "/", Icon: House, end: true },
  { key: "cards", to: "/cards", Icon: Search, end: false },
  { key: "decks", to: "/decks", Icon: Layers, end: false },
]
