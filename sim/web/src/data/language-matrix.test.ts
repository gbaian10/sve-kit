// @vitest-environment node
import { describe, expect, it } from "vitest"

import { annotatedOrigin } from "../test-utils/annotated-snapshot"
import {
  invalidLanguageCases,
  invalidLanguageSnapshot,
  languageCases,
  languageSnapshot,
} from "../test-utils/language-matrix"
import { sharedAnnotatedTextResolver } from "./annotated-text"
import { catalogOf } from "./catalog"
import { stringValue } from "./format-v3/json"

describe("the eighteen language choices through the native reader", () => {
  it.each(languageCases)("$id", async (row) => {
    const { client } = await annotatedOrigin(languageSnapshot, row.region)
    await client.load()
    expect(client.status().state).toBe("ready")
    const result = await sharedAnnotatedTextResolver(client).resolve(
      row.receiver,
      row.text_language,
    )
    const snapshot = client.snapshot()
    if (!snapshot) throw new Error("synthetic snapshot missing")
    const index = catalogOf(snapshot).index
    const revision = index.revision(row.receiver.owner.id)
    const cardId = index.face(stringValue(revision?.["face_id"]))?.["card_id"]
    expect(typeof cardId).toBe("string")
    expect(
      await sharedAnnotatedTextResolver(client, stringValue(cardId)).resolve(
        row.receiver,
        row.text_language,
      ),
    ).toEqual(result)
    expect(result).not.toBeNull()
    expect(result?.original.unit["text"]).toBe(row.expected.original)
    expect(result?.translated?.unit["text"] ?? null).toBe(row.expected.translated)
    expect(result?.selection?.["basis"] ?? null).toBe(row.expected.basis)
    expect(result?.translation?.["low_confidence"] ?? false).toBe(row.expected.low_confidence)
    expect(result?.translation?.["origin"] ?? null).toBe(row.expected.origin)
    expect(result?.translation?.["authority"] ?? null).toBe(row.expected.authority)
    if (row.expected.missing) {
      expect(result?.translated).toBeUndefined()
      expect(result?.original.unit["lang"]).not.toBe(row.text_language)
    } else if (row.expected.translated === null) {
      expect(result?.translated).toBeUndefined()
      expect(result?.original.unit["lang"]).toBe(row.text_language)
    } else expect(result?.translated?.unit["lang"]).toBe(row.text_language)
  })

  it.each(invalidLanguageCases)("rejects $id", async (row) => {
    const { client } = await annotatedOrigin(await invalidLanguageSnapshot(row), row.region)
    await client.load()
    expect(client.status().state).toBe("ready")
    await expect(
      sharedAnnotatedTextResolver(client).resolve(row.receiver, row.text_language),
    ).rejects.toThrow("public-annotation/reference")
  })
})
