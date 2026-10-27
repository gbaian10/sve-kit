import { useSyncExternalStore } from "react"

import { type Accent, isAccent, isThemePref, type ThemePref } from "../domain/theme"
import { isUiLanguage, type UiLanguage } from "../i18n/languages"
import {
  DEFAULT_LIST_VISIBILITY,
  isListWidths,
  LIST_COLUMNS,
  type ListColumn,
  type ListVisibility,
  type ListWidths,
  sanitizeListOrder,
  sanitizeListVisibility,
} from "./list-columns"
import { createPrefsStore, type PrefsStore } from "./prefs-storage"

export const PREFS_KEY = "sve-kit:prefs"
export const PREFS_VERSION = 1

export const CARD_EDITIONS = ["jp", "en"] as const
type CardEdition = (typeof CARD_EDITIONS)[number]

export const TEXT_DISPLAYS = ["original", "translated", "both"] as const
type TextDisplay = (typeof TEXT_DISPLAYS)[number]
/** Flavor text and Q&A may also be hidden; a card name and the effect text always show. */
export const FIELD_DISPLAYS = ["none", ...TEXT_DISPLAYS] as const
export type FieldDisplay = (typeof FIELD_DISPLAYS)[number]

const GRID_DENSITIES = [2, 3] as const
type GridDensity = (typeof GRID_DENSITIES)[number]

const VIEW_MODES = ["grid", "table", "list"] as const
type ViewMode = (typeof VIEW_MODES)[number]

/** How a result opens its card: `auto` = side panel on desktop, modal elsewhere. */
export const CARD_OPEN_MODES = ["auto", "panel", "modal", "page"] as const
export type CardOpenMode = (typeof CARD_OPEN_MODES)[number]

/** Where the card drawer sits on the cards page at sidebar width: the filters take the other side. */
export const CARDS_LAYOUTS = ["cardLeft", "filterLeft"] as const
export type CardsLayout = (typeof CARDS_LAYOUTS)[number]

export interface Prefs {
  /** null = not chosen yet; the app detects it from the browser languages. */
  readonly uiLanguage: UiLanguage | null
  readonly cardEdition: CardEdition
  readonly showOtherEdition: Readonly<Record<CardEdition, boolean>>
  readonly nameDisplay: TextDisplay
  readonly effectLanguage: TextDisplay
  readonly flavorDisplay: FieldDisplay
  readonly qaDisplay: FieldDisplay
  readonly infiniteScroll: boolean
  readonly preloadPrintings: boolean
  readonly preloadNeighbours: boolean
  readonly backgroundDownloads: boolean
  readonly symbolLabels: boolean
  readonly termEmphasis: boolean
  readonly showDigitalLinks: boolean
  /** Whether the card list's filter panel is expanded. */
  readonly filterPanelOpen: boolean
  readonly theme: ThemePref
  /** null = the theme's default accent. */
  readonly accent: Accent | null
  readonly gridDensity: GridDensity
  readonly viewMode: ViewMode
  readonly cardOpenMode: CardOpenMode
  readonly cardsLayout: CardsLayout
  /** The card drawer's width in px once the user has dragged it; null = the default clamp. */
  readonly drawerWidth: number | null
  readonly listWidths: ListWidths
  readonly listMobileWidths: ListWidths
  readonly listVisibility: ListVisibility
  readonly listOrder: readonly ListColumn[]
}

export const DEFAULT_PREFS: Prefs = {
  uiLanguage: null,
  cardEdition: "jp",
  showOtherEdition: { jp: false, en: true },
  nameDisplay: "both",
  effectLanguage: "both",
  flavorDisplay: "both",
  qaDisplay: "both",
  infiniteScroll: true,
  preloadPrintings: false,
  preloadNeighbours: false,
  backgroundDownloads: true,
  symbolLabels: false,
  termEmphasis: true,
  showDigitalLinks: true,
  filterPanelOpen: false,
  theme: "system",
  accent: null,
  gridDensity: 2,
  viewMode: "grid",
  cardOpenMode: "auto",
  cardsLayout: "cardLeft",
  drawerWidth: null,
  listWidths: {},
  listMobileWidths: {},
  listVisibility: DEFAULT_LIST_VISIBILITY,
  listOrder: LIST_COLUMNS,
}

function oneOf<T extends string | number>(values: readonly T[]): (value: unknown) => value is T {
  return (value): value is T => (values as readonly unknown[]).includes(value)
}

const isCardEdition = oneOf(CARD_EDITIONS)
const isTextDisplay = oneOf(TEXT_DISPLAYS)
const isFieldDisplay = oneOf(FIELD_DISPLAYS)
const isGridDensity = oneOf(GRID_DENSITIES)
const isViewMode = oneOf(VIEW_MODES)
const isCardOpenMode = oneOf(CARD_OPEN_MODES)
const isCardsLayout = oneOf(CARDS_LAYOUTS)
const isDrawerWidth = (value: unknown): value is number | null =>
  value === null ||
  (Number.isInteger(value) && (value as number) >= 600 && (value as number) <= 1200)
const isBoolean = (value: unknown): value is boolean => typeof value === "boolean"
const isNullableUiLanguage = (value: unknown): value is UiLanguage | null =>
  value === null || isUiLanguage(value)
const isNullableAccent = (value: unknown): value is Accent | null =>
  value === null || isAccent(value)
const isEditionSwitches = (value: unknown): value is Readonly<Record<CardEdition, boolean>> =>
  typeof value === "object" &&
  value !== null &&
  CARD_EDITIONS.every((edition) => isBoolean((value as Record<string, unknown>)[edition]))

type Sanitizers = { readonly [K in keyof Prefs]: (value: unknown) => Prefs[K] }

function withDefault<T>(guard: (value: unknown) => value is T, fallback: T): (value: unknown) => T {
  return (value) => (guard(value) ? value : fallback)
}

const sanitizers: Sanitizers = {
  uiLanguage: withDefault(isNullableUiLanguage, DEFAULT_PREFS.uiLanguage),
  cardEdition: withDefault(isCardEdition, DEFAULT_PREFS.cardEdition),
  showOtherEdition: withDefault(isEditionSwitches, DEFAULT_PREFS.showOtherEdition),
  nameDisplay: withDefault(isTextDisplay, DEFAULT_PREFS.nameDisplay),
  effectLanguage: withDefault(isTextDisplay, DEFAULT_PREFS.effectLanguage),
  flavorDisplay: withDefault(isFieldDisplay, DEFAULT_PREFS.flavorDisplay),
  qaDisplay: withDefault(isFieldDisplay, DEFAULT_PREFS.qaDisplay),
  infiniteScroll: withDefault(isBoolean, DEFAULT_PREFS.infiniteScroll),
  preloadPrintings: withDefault(isBoolean, DEFAULT_PREFS.preloadPrintings),
  preloadNeighbours: withDefault(isBoolean, DEFAULT_PREFS.preloadNeighbours),
  backgroundDownloads: withDefault(isBoolean, DEFAULT_PREFS.backgroundDownloads),
  symbolLabels: withDefault(isBoolean, DEFAULT_PREFS.symbolLabels),
  termEmphasis: withDefault(isBoolean, DEFAULT_PREFS.termEmphasis),
  showDigitalLinks: withDefault(isBoolean, DEFAULT_PREFS.showDigitalLinks),
  filterPanelOpen: withDefault(isBoolean, DEFAULT_PREFS.filterPanelOpen),
  theme: withDefault(isThemePref, DEFAULT_PREFS.theme),
  accent: withDefault(isNullableAccent, DEFAULT_PREFS.accent),
  gridDensity: withDefault(isGridDensity, DEFAULT_PREFS.gridDensity),
  viewMode: withDefault(isViewMode, DEFAULT_PREFS.viewMode),
  cardOpenMode: withDefault(isCardOpenMode, DEFAULT_PREFS.cardOpenMode),
  cardsLayout: withDefault(isCardsLayout, DEFAULT_PREFS.cardsLayout),
  drawerWidth: withDefault(isDrawerWidth, DEFAULT_PREFS.drawerWidth),
  listWidths: withDefault(isListWidths, DEFAULT_PREFS.listWidths),
  listMobileWidths: withDefault(isListWidths, DEFAULT_PREFS.listMobileWidths),
  listVisibility: sanitizeListVisibility,
  listOrder: sanitizeListOrder,
}

export function sanitize(raw: unknown): Prefs {
  if (typeof raw !== "object" || raw === null || Array.isArray(raw)) return DEFAULT_PREFS
  const record = raw as Record<string, unknown>
  if (record["v"] !== PREFS_VERSION) return DEFAULT_PREFS
  // Flavor and Q&A used to follow the effect text, and flavor had its own on/off switch; a stored
  // choice from before the split carries over once, and the next write drops the old keys.
  const carried: Record<string, unknown> = {
    flavorDisplay: record["showFlavorText"] === false ? "none" : record["effectLanguage"],
    qaDisplay: record["effectLanguage"],
    ...record,
  }
  const prefs: Record<string, unknown> = {}
  for (const key of Object.keys(DEFAULT_PREFS) as (keyof Prefs)[]) {
    prefs[key] = sanitizers[key](carried[key])
  }
  return prefs as unknown as Prefs
}

export const prefsStore: PrefsStore = createPrefsStore()

export function usePrefs(): Prefs {
  return useSyncExternalStore(prefsStore.subscribe, prefsStore.get, prefsStore.get)
}

export function showsOtherEdition(prefs: Prefs, edition: CardEdition): boolean {
  return prefs.showOtherEdition[edition]
}

/** The one value all four text fields share, or "custom" when they differ or one is hidden. */
export function cardTranslation(prefs: Prefs): TextDisplay | "custom" {
  const shared = prefs.nameDisplay
  return [prefs.effectLanguage, prefs.flavorDisplay, prefs.qaDisplay].every(
    (value) => value === shared,
  )
    ? shared
    : "custom"
}

export function cardTranslationPatch(value: TextDisplay): Partial<Prefs> {
  return { nameDisplay: value, effectLanguage: value, flavorDisplay: value, qaDisplay: value }
}
