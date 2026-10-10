import { useSyncExternalStore } from "react"

import { type Accent, isAccent, isThemePref, type ThemePref } from "../domain/theme"
import { isUiLanguage, type UiLanguage } from "../i18n/languages"

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
  readonly nameDisplay: TextDisplay
  readonly effectLanguage: TextDisplay
  readonly symbolLabels: boolean
  readonly termEmphasis: boolean
  readonly theme: ThemePref
  /** null = the theme's default accent. */
  readonly accent: Accent | null
  readonly banRegion: CardEdition
  readonly dataSaver: boolean
  readonly gridDensity: GridDensity
  readonly viewMode: ViewMode
}

export const DEFAULT_PREFS: Prefs = {
  uiLanguage: null,
  cardEdition: "jp",
  nameDisplay: "translated",
  effectLanguage: "translated",
  symbolLabels: false,
  termEmphasis: true,
  theme: "system",
  accent: null,
  banRegion: "jp",
  dataSaver: false,
  gridDensity: 2,
  viewMode: "grid",
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

type Guards = { readonly [K in keyof Prefs]: (value: unknown) => value is Prefs[K] }

const guards: Guards = {
  uiLanguage: isNullableUiLanguage,
  cardEdition: isCardEdition,
  nameDisplay: isTextDisplay,
  effectLanguage: isTextDisplay,
  symbolLabels: isBoolean,
  termEmphasis: isBoolean,
  theme: isThemePref,
  accent: isNullableAccent,
  banRegion: isCardEdition,
  dataSaver: isBoolean,
  gridDensity: isGridDensity,
  viewMode: isViewMode,
}

type StorageSource = Storage | (() => Storage) | undefined

// Even reading `localStorage` can throw (opaque origins, blocked site data), so resolve it inside try.
function resolveStorage(source: StorageSource): Storage {
  if (source === undefined) return localStorage
  return typeof source === "function" ? source() : source
}

// Only the exact version this build writes is trusted; a missing or different `v` means another
// build's layout, and guessing field by field could keep half of it. Same rule in theme-boot.js.
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
