import { isUiLanguage, type UiLanguage } from "../i18n/languages"

const STORAGE_KEY = "sve-kit:ui-language"

// Even reading `localStorage` can throw (opaque origins, blocked site data), so resolve it inside try.
export function loadUiLanguage(storage?: Storage): UiLanguage | undefined {
  try {
    const value = (storage ?? localStorage).getItem(STORAGE_KEY)
    return isUiLanguage(value) ? value : undefined
  } catch {
    return undefined
  }
}

export function saveUiLanguage(language: UiLanguage, storage?: Storage): boolean {
  try {
    ;(storage ?? localStorage).setItem(STORAGE_KEY, language)
    return true
  } catch {
    return false
  }
}
