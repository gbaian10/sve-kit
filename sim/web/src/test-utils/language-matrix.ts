import fixture from "../../../../tests/fixtures/snapshot-contract/v3/language-matrix.json"
import { arrayValue, canonicalText, type JsonObject, objectValue, stringValue } from "../data"
import type { Region } from "../domain/search"
import type { UiLanguage } from "../i18n"
import { isUiLanguage } from "../i18n/languages"

function region(value: string): Region {
  if (value !== "jp" && value !== "en") throw new Error("invalid matrix region")
  return value
}

function uiLanguage(value: string): UiLanguage {
  if (!isUiLanguage(value)) throw new Error("invalid matrix UI language")
  return value
}

function origin(value: string | null): "official" | "project" | "machine" | null {
  if (value !== null && value !== "official" && value !== "project" && value !== "machine")
    throw new Error("invalid matrix origin")
  return value
}

function authority(
  value: string | null,
): "sve_official" | "digital_official" | "unofficial" | null {
  if (
    value !== null &&
    value !== "sve_official" &&
    value !== "digital_official" &&
    value !== "unofficial"
  )
    throw new Error("invalid matrix authority")
  return value
}

export const languageCases = fixture.cases.map((row) => ({
  ...row,
  region: region(row.region),
  ui_language: uiLanguage(row.ui_language),
  expected: {
    ...row.expected,
    origin: origin(row.expected.origin),
    authority: authority(row.expected.authority),
  },
}))
export const invalidLanguageCases = fixture.invalid_cases.map((row) => ({
  ...row,
  region: region(row.region),
}))
export const languageSnapshot = fixture.snapshot
export const languageTexts = fixture.texts

export async function invalidLanguageSnapshot(
  row: (typeof invalidLanguageCases)[number],
): Promise<JsonObject> {
  const snapshot = structuredClone(objectValue(languageSnapshot))
  const manifest = objectValue(snapshot["manifest"])
  const payloads = objectValue(snapshot["payloads"])
  let changed = 0
  for (const [key, raw] of Object.entries(payloads)) {
    const payload = objectValue(raw)
    const tables = objectValue(payload["tables"] ?? {})
    for (const fragment of arrayValue(tables["translation"] ?? []))
      for (const raw of arrayValue(arrayValue(fragment)[4])) {
        const cells = arrayValue(raw)
        if (cells[fixture.translation_columns.indexOf("id")] !== row.translation_id) continue
        cells[fixture.translation_columns.indexOf(row.field)] = row.value
        changed += 1
        const bytes = new TextEncoder().encode(canonicalText(payload))
        const hash =
          "sha256:" +
          Array.from(new Uint8Array(await crypto.subtle.digest("SHA-256", bytes)), (byte) =>
            byte.toString(16).padStart(2, "0"),
          ).join("")
        const file = arrayValue(manifest["files"])
          .map((value) => objectValue(value))
          .find((file) => file["key"] === key)
        if (!file) throw new Error("missing matrix translation file")
        file["sha256"] = hash
        file["bytes"] = bytes.byteLength
        file["path"] = `snapshots/blobs/${hash.slice(7)}.json`
        for (const rawFile of arrayValue(manifest["files"]))
          for (const field of ["dependencies", "source_refs"])
            for (const rawRef of arrayValue(objectValue(rawFile)[field])) {
              const ref = objectValue(rawRef)
              if (stringValue(ref["key"]) === key) ref["sha256"] = hash
            }
      }
  }
  if (changed !== 1) throw new Error("matrix mutation must change one translation")
  return snapshot
}
