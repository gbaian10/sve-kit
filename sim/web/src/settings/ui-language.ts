import type { UiLanguage } from "../i18n/languages"
import { prefsStore, readPrefs, writePrefs } from "./prefs"

// Thin wrappers over the prefs object. Without an explicit storage they go through the shared
// store, so subscribers see the change and a later set() cannot overwrite the language.
export function loadUiLanguage(storage?: Storage): UiLanguage | undefined {
  const prefs = storage ? readPrefs(storage) : prefsStore.get()
  return prefs.uiLanguage ?? undefined
}

export function saveUiLanguage(language: UiLanguage, storage?: Storage): boolean {
  if (storage) return writePrefs({ ...readPrefs(storage), uiLanguage: language }, storage)
  return prefsStore.set({ uiLanguage: language })
}
