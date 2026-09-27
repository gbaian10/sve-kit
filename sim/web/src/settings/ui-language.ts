import { isUiLanguage, type UiLanguage } from "../i18n/languages"

const STORAGE_KEY = "sve-kit:ui-language"

// Storage access throws in some private modes and when blocked by the browser.
export function loadUiLanguage(storage: Storage = localStorage): UiLanguage | undefined {
  try {
    const value = storage.getItem(STORAGE_KEY)
    return isUiLanguage(value) ? value : undefined
  } catch {
    return undefined
  }
}

export function saveUiLanguage(language: UiLanguage, storage: Storage = localStorage): boolean {
  try {
    storage.setItem(STORAGE_KEY, language)
    return true
  } catch {
    return false
  }
}
