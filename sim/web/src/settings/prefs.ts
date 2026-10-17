import { useSyncExternalStore } from "react"

import { type Accent, isAccent, isThemePref, type ThemePref } from "../domain/theme"
import { isUiLanguage, type UiLanguage } from "../i18n/languages"
import {
  DEFAULT_LIST_VISIBILITY,
  isListVisibility,
  isListWidths,
  type ListVisibility,
  type ListWidths,
} from "./list-columns"

export const PREFS_KEY = "sve-kit:prefs"
const PREFS_VERSION = 1

export const CARD_EDITIONS = ["jp", "en"] as const
type CardEdition = (typeof CARD_EDITIONS)[number]

export const TEXT_DISPLAYS = ["translated", "original", "both"] as const
type TextDisplay = (typeof TEXT_DISPLAYS)[number]

const GRID_DENSITIES = [2, 3] as const
type GridDensity = (typeof GRID_DENSITIES)[number]

const VIEW_MODES = ["grid", "table", "list"] as const
type ViewMode = (typeof VIEW_MODES)[number]

export interface Prefs {
  /** null = not chosen yet; the app detects it from the browser languages. */
  readonly uiLanguage: UiLanguage | null
  readonly cardEdition: CardEdition
  readonly showOtherEdition: Readonly<Record<CardEdition, boolean>>
  readonly nameDisplay: TextDisplay
  readonly effectLanguage: TextDisplay
  readonly infiniteScroll: boolean
  readonly preloadPrintings: boolean
  readonly preloadNeighbours: boolean
  readonly symbolLabels: boolean
  readonly termEmphasis: boolean
  readonly theme: ThemePref
  /** null = the theme's default accent. */
  readonly accent: Accent | null
  readonly banRegion: CardEdition
  readonly gridDensity: GridDensity
  readonly viewMode: ViewMode
  readonly listWidths: ListWidths
  readonly listVisibility: ListVisibility
}

export const DEFAULT_PREFS: Prefs = {
  uiLanguage: null,
  cardEdition: "jp",
  showOtherEdition: { jp: false, en: true },
  nameDisplay: "translated",
  effectLanguage: "both",
  infiniteScroll: true,
  preloadPrintings: false,
  preloadNeighbours: false,
  symbolLabels: false,
  termEmphasis: true,
  theme: "system",
  accent: null,
  banRegion: "jp",
  gridDensity: 2,
  viewMode: "grid",
  listWidths: {},
  listVisibility: DEFAULT_LIST_VISIBILITY,
}

function oneOf<T extends string | number>(values: readonly T[]): (value: unknown) => value is T {
  return (value): value is T => (values as readonly unknown[]).includes(value)
}

const isCardEdition = oneOf(CARD_EDITIONS)
const isTextDisplay = oneOf(TEXT_DISPLAYS)
const isGridDensity = oneOf(GRID_DENSITIES)
const isViewMode = oneOf(VIEW_MODES)
const isBoolean = (value: unknown): value is boolean => typeof value === "boolean"
const isNullableUiLanguage = (value: unknown): value is UiLanguage | null =>
  value === null || isUiLanguage(value)
const isNullableAccent = (value: unknown): value is Accent | null =>
  value === null || isAccent(value)
const isEditionSwitches = (value: unknown): value is Readonly<Record<CardEdition, boolean>> =>
  typeof value === "object" &&
  value !== null &&
  CARD_EDITIONS.every((edition) => isBoolean((value as Record<string, unknown>)[edition]))

type Guards = { readonly [K in keyof Prefs]: (value: unknown) => value is Prefs[K] }

const guards: Guards = {
  uiLanguage: isNullableUiLanguage,
  cardEdition: isCardEdition,
  showOtherEdition: isEditionSwitches,
  nameDisplay: isTextDisplay,
  effectLanguage: isTextDisplay,
  infiniteScroll: isBoolean,
  preloadPrintings: isBoolean,
  preloadNeighbours: isBoolean,
  symbolLabels: isBoolean,
  termEmphasis: isBoolean,
  theme: isThemePref,
  accent: isNullableAccent,
  banRegion: isCardEdition,
  gridDensity: isGridDensity,
  viewMode: isViewMode,
  listWidths: isListWidths,
  listVisibility: isListVisibility,
}

type StorageSource = Storage | (() => Storage) | undefined

// Even reading `localStorage` can throw (opaque origins, blocked site data), so resolve it inside try.
function resolveStorage(source: StorageSource): Storage {
  if (source === undefined) return localStorage
  return typeof source === "function" ? source() : source
}

// Trust only this build's version, as theme-boot.js does; DEFAULT_PREFS excludes removed fields.
function sanitize(raw: unknown): Prefs {
  if (typeof raw !== "object" || raw === null || Array.isArray(raw)) return DEFAULT_PREFS
  const record = raw as Record<string, unknown>
  if (record["v"] !== PREFS_VERSION) return DEFAULT_PREFS
  const prefs: Record<string, unknown> = {}
  for (const key of Object.keys(DEFAULT_PREFS) as (keyof Prefs)[]) {
    const value = record[key]
    prefs[key] = guards[key](value) ? value : DEFAULT_PREFS[key]
  }
  return prefs as unknown as Prefs
}

/** Never throws; every field falls back to its default on its own. */
export function readPrefs(source?: StorageSource): Prefs {
  try {
    const storage = resolveStorage(source)
    const raw = storage.getItem(PREFS_KEY)
    if (raw !== null) return sanitize(JSON.parse(raw))
    return DEFAULT_PREFS
  } catch {
    return DEFAULT_PREFS
  }
}

export function writePrefs(prefs: Prefs, source?: StorageSource): boolean {
  try {
    resolveStorage(source).setItem(PREFS_KEY, JSON.stringify({ v: PREFS_VERSION, ...prefs }))
    return true
  } catch {
    return false
  }
}

export interface PrefsStore {
  readonly get: () => Prefs
  /** Applies the patch in memory first, so the UI follows even when persisting fails. */
  readonly set: (patch: Partial<Prefs>) => boolean
  readonly subscribe: (listener: () => void) => () => void
  /** Re-reads storage (another tab wrote it, or a test cleared it) and notifies subscribers. */
  readonly reload: () => void
}

export function createPrefsStore(source?: StorageSource): PrefsStore {
  let current = readPrefs(source)
  const listeners = new Set<() => void>()
  return {
    get: () => current,
    set: (patch) => {
      current = { ...current, ...patch }
      for (const listener of listeners) listener()
      return writePrefs(current, source)
    },
    subscribe: (listener) => {
      listeners.add(listener)
      return () => {
        listeners.delete(listener)
      }
    },
    reload: () => {
      current = readPrefs(source)
      for (const listener of listeners) listener()
    },
  }
}

export const prefsStore: PrefsStore = createPrefsStore()

export function usePrefs(): Prefs {
  return useSyncExternalStore(prefsStore.subscribe, prefsStore.get, prefsStore.get)
}

export function showsOtherEdition(prefs: Prefs, edition: CardEdition): boolean {
  return prefs.showOtherEdition[edition]
}
