// @vitest-environment node
import { describe, expect, it } from "vitest"

import { annotatedOrigin } from "../test-utils/annotated-snapshot"
import { createAnnotatedTextResolver, sharedAnnotatedTextResolver } from "./annotated-text"
import {
  arrayValue,
  canonicalText,
  type JsonObject,
  objectValue,
  stringValue,
} from "./format-v3/json"

async function historicalCandidateOrigin() {
  const { client } = await annotatedOrigin()
  await client.load()
  const loaded = client.snapshot()
  if (!loaded) throw new Error("synthetic snapshot missing")
  const detail = await client.fragments("detail/jp/family-family/0")
  const revision = detail.find((fragment) => fragment.table === "face_revision")
  if (!revision) throw new Error("synthetic revision missing")
  const historyKey = "history/jp/family-family/0"
  const historyValue: JsonObject = {
    ...revision.value,
    base: null,
    partition: "history",
    purpose: "history",
    region: "jp",
    segment: "family-family:band-0",
  }
  const history = {
    ...revision,
    file: historyKey,
    value: historyValue,
    rows: revision.rows.map((row) => ({ ...row, id: "historical" })),
    identity: canonicalText([
      "face_revision",
      historyValue["owner"] ?? null,
      historyValue["segment"] ?? null,
      "history",
    ]),
  }
  const files = new Map(loaded.files)
  files.set(historyKey, {
    key: historyKey,
    role: "text",
    purpose: "history",
    region: "jp",
    segment: "family-family:band-0",
    row_counts: [
      [
        "face_revision",
        "home_set",
        "family",
        "history",
        "family-family:band-0",
        revision.rows.length,
      ],
    ],
  })
  const snapshot = {
    ...loaded,
    files,
    bootstrap: loaded.bootstrap.map((fragment) =>
      fragment.table === "face"
        ? {
            ...fragment,
            rows: fragment.rows.map((row) => ({
              ...row,
              wording: [
                {
                  region: "jp",
                  state: "pending",
                  display: { revision_id: "revision", basis: "candidates" },
                  candidates: [
                    { printing_id: "printing", revision_id: "historical" },
                    { printing_id: "printing", revision_id: "revision" },
                  ],
                  undated_printing_ids: [],
                },
              ],
            })),
          }
        : fragment,
    ),
  }
  const requested: string[] = []
  return {
    requested,
    client: {
      ...client,
      snapshot: () => snapshot,
      fragments: async (key: string) => {
        requested.push(key)
        return key === historyKey ? [history] : client.fragments(key)
      },
    },
    revision: objectValue(revision.rows[0]),
    historyKey,
  }
}

describe.each([undefined, "card"])("the native C → P → R annotation slice (%s)", (cardId) => {
  const createResolver = (client: Parameters<typeof createAnnotatedTextResolver>[0]) =>
    createAnnotatedTextResolver(client, cardId)
  it("discovers and resolves a candidate's source sections with their original ordinals", async () => {
    const { client, historyKey, revision } = await historicalCandidateOrigin()
    const unitId = stringValue(revision["effect_unit_id"])
    const source = {
      ...client,
      fragments: async (key: string) => {
        const fragments = await client.fragments(key)
        return key === historyKey
          ? fragments.map((fragment) => ({
              ...fragment,
              rows: fragment.rows.map((row) => ({
                ...row,
                sections: [1, 3].map((ordinal) => ({
                  ordinal,
                  label_unit_id: ordinal === 3 ? unitId : null,
                  text_unit_id: unitId,
                })),
              })),
            }))
          : fragments
      },
    }
    const resolver = createResolver(source)
    const owner = { kind: "face_revision", id: "historical" }
    const effect = await resolver.resolve({ owner, field: "effect", ordinal: null }, "ja")
    expect(effect).toEqual(
      await createAnnotatedTextResolver(source).resolve(
        { owner, field: "effect", ordinal: null },
        "ja",
      ),
    )
    expect(effect?.originalSections).toEqual([
      { ordinal: 1 },
      { ordinal: 3, label: "Synthetic text" },
    ])
    for (const { ordinal } of effect?.originalSections ?? []) {
      const section = await resolver.resolve({ owner, field: "section", ordinal }, "ja")
      expect(section?.original.unit["text"]).toBe("Synthetic text")
      expect(section?.translated).toBeUndefined()
    }
  })
  it("locates a direct historical candidate through its face without a translation receiver", async () => {
    const { client, requested, revision, historyKey } = await historicalCandidateOrigin()
    const resolver = createResolver(client)
    const pointer = {
      owner: { kind: "face_revision", id: "historical" },
      field: "effect",
      ordinal: null,
    }
    const value = await resolver.resolve(pointer, "ja")
    expect(value?.original.unit["id"]).toBe(revision["effect_unit_id"])
    expect(value?.original.unit["text"]).toBe("Synthetic text")
    expect(requested).toContain(historyKey)
    expect(requested.filter((key) => key === historyKey)).toHaveLength(1)
    const before = requested.length
    expect(await resolver.resolve(pointer, "ja")).toBe(value)
    expect(requested).toHaveLength(before)
    await expect(
      resolver.resolve({ ...pointer, owner: { kind: "face_revision", id: "unlisted" } }, "ja"),
    ).rejects.toThrow("owner")
    expect(requested).toHaveLength(before)
  })
  it("validates bootstrap name annotations and their explanation without loading unrelated files", async () => {
    const { client, requests } = await annotatedOrigin()
    await client.load()
    expect(client.status().state).toBe("ready")
    const before = requests.length
    const resolver = createResolver(client)
    const value = await resolver.resolve(
      { owner: { kind: "face_revision", id: "revision" }, field: "name", ordinal: null },
      "zh-Hant",
    )
    expect(value?.original.annotation?.["occurrences"]).toHaveLength(1)
    expect(value?.translated?.annotation?.["occurrences"]).toHaveLength(1)
    expect(value?.translated?.unit["text"]).toBe("Synthetic translation")
    expect(value?.selection?.["basis"]).toBe("own_source")
    expect(value?.translation?.["low_confidence"]).toBe(false)
    expect(value?.explanations[0]?.reference).toEqual({ kind: "keyword", id: "keyword" })
    const full = createAnnotatedTextResolver(client)
    for (const field of ["label", "effect"])
      for (const language of ["ja", "en", "zh-Hant"]) {
        const pointer = { owner: { kind: "keyword", id: "keyword" }, field, ordinal: null }
        expect(await resolver.resolve(pointer, language)).toEqual(
          await full.resolve(pointer, language),
        )
      }
    expect(requests).toHaveLength(before)
  })
  it.each(["matching", "wrong-text", "wrong-concept", "conflicting-copy"])(
    "validates %s name witnesses against the materialized owner",
    async (mutation) => {
      const { client } = await annotatedOrigin()
      await client.load()
      const snapshot = client.snapshot()
      const fragment = snapshot?.bootstrap.find((f) => f.table === "face_revision")
      const revision = fragment?.rows[0]
      if (!snapshot || !fragment || !revision) throw new Error("synthetic revision missing")
      const pointer = {
        owner: { kind: "face_revision", id: "revision" },
        field: "name",
        ordinal: null,
      }
      const source = {
        pointer,
        text_unit_id: revision["name_unit_id"] ?? null,
        name_concept_id: revision["name_concept_id"] ?? null,
      }
      const altered = {
        ...source,
        ...(mutation === "wrong-text" ? { text_unit_id: "wrong-text" } : {}),
        ...(mutation === "wrong-concept" || mutation === "conflicting-copy"
          ? { name_concept_id: "wrong-concept" }
          : {}),
      }
      // A bad single witness fails on its own; a conflicting copy in another file fails even
      // though the copy read last matches the owner.
      const files =
        mutation === "matching"
          ? [[source]]
          : mutation === "conflicting-copy"
            ? [[altered], [source]]
            : [[altered]]
      const loaded = {
        ...snapshot,
        bootstrap: [
          ...snapshot.bootstrap,
          ...files.map((nameSources) => ({
            ...fragment,
            table: "__name_sources",
            rows: [],
            nameSources,
          })),
        ],
      }
      const patched = { ...client, snapshot: () => loaded }
      const resolve = () => createResolver(patched).resolve(pointer, "zh-Hant")
      if (mutation === "matching") {
        const expected = await createResolver(client).resolve(pointer, "zh-Hant")
        expect(await resolve()).toEqual(expected)
        expect(await resolve()).toEqual(expected)
      } else await expect(resolve()).rejects.toThrow("public-annotation/owner")
    },
  )
  it("reports an explanation without its text as a missing reference", async () => {
    const { client } = await annotatedOrigin()
    await client.load()
    const snapshot = client.snapshot()
    if (!snapshot) throw new Error("synthetic snapshot missing")
    for (const fragment of snapshot.bootstrap)
      if (fragment.table === "keyword")
        for (const row of fragment.rows) row["definition_unit_id"] = null
    await expect(
      createResolver(client).resolve(
        { owner: { kind: "face_revision", id: "revision" }, field: "name", ordinal: null },
        "zh-Hant",
      ),
    ).rejects.toThrow("public-annotation/reference")
  })
  it("projects a confirmed card-name target with the fields its link and label need", async () => {
    const { client } = await annotatedOrigin()
    await client.load()
    const snapshot = client.snapshot()
    if (!snapshot) throw new Error("synthetic snapshot missing")
    // The producer confirms the concept-to-card relation; the reader only projects it.
    for (const fragment of snapshot.bootstrap) {
      if (fragment.table === "annotation_concept")
        for (const row of fragment.rows)
          if (row["id"] === "term") {
            row["category"] = "card_name"
            row["card_ids"] = ["card"]
          }
      if (fragment.table === "annotation_set")
        for (const row of fragment.rows)
          for (const raw of arrayValue(row["occurrences"]))
            objectValue(raw)["reference"] = { kind: "card_name", term_id: "term" }
    }
    const value = await createResolver(client).resolve(
      { owner: { kind: "face_revision", id: "revision" }, field: "name", ordinal: null },
      "zh-Hant",
    )
    expect(value?.concepts.map((concept) => concept["card_ids"])).toEqual([["card"]])
    expect(
      value?.cards.map((card) => ({
        cardId: card.cardId,
        intId: card.intId,
        cardNo: card.cardNo,
        cardNoState: card.cardNoState,
        region: card.region,
        name: card.name.original.text,
      })),
    ).toEqual([
      {
        cardId: "card",
        intId: 20001,
        cardNo: "TEST-001Ⓢa",
        cardNoState: "official",
        region: "jp",
        name: stringValue(value?.original.unit["text"]),
      },
    ])
  })
  it("uses the exact effect owner and treats verified absent annotations as empty", async () => {
    const { client, requests } = await annotatedOrigin()
    await client.load()
    const before = requests.length
    const value = await createResolver(client).resolve(
      { owner: { kind: "face_revision", id: "revision" }, field: "effect", ordinal: null },
      "en",
    )
    expect(value?.original.annotation).toBeNull()
    expect(value?.translated).toBeUndefined()
    expect(requests.length).toBeGreaterThan(before)
  })
  it("shares one resolver per loaded snapshot and resolves each text once", async () => {
    const { client, requests } = await annotatedOrigin()
    await client.load()
    const resolver = sharedAnnotatedTextResolver(client, cardId)
    expect(sharedAnnotatedTextResolver(client, cardId)).toBe(resolver)
    const effect = {
      owner: { kind: "face_revision", id: "revision" },
      field: "effect",
      ordinal: null,
    }
    const name = { ...effect, field: "name" }
    expect(resolver.peek(effect, "en")).toBeUndefined()
    // Texts resolved together load their shared files once and still pass validation.
    const [first, again, other] = await Promise.all([
      resolver.resolve(effect, "en"),
      resolver.resolve(effect, "en"),
      resolver.resolve(name, "zh-Hant"),
    ])
    expect(again).toBe(first)
    expect(other?.translated?.unit["text"]).toBe("Synthetic translation")
    expect(resolver.peek(effect, "en")).toBe(first)
    const settled = requests.length
    expect(await resolver.resolve(effect, "en")).toBe(first)
    expect(requests).toHaveLength(settled)
  })
  it("does not expose settled text after the client's snapshot is replaced", async () => {
    const { client } = await annotatedOrigin()
    await client.load()
    let current = client.snapshot()
    const resolver = createResolver({ ...client, snapshot: () => current })
    const pointer = {
      owner: { kind: "face_revision", id: "revision" },
      field: "name",
      ordinal: null,
    }
    expect(await resolver.resolve(pointer, "en")).not.toBeNull()
    current = null
    expect(resolver.peek(pointer, "en")).toBeUndefined()
    await expect(resolver.resolve(pointer, "en")).rejects.toThrow("snapshot replaced")
  })
  it("rejects text finishing after the client's snapshot is replaced", async () => {
    const { client } = await annotatedOrigin()
    await client.load()
    let current = client.snapshot()
    const resolver = createResolver({ ...client, snapshot: () => current })
    const pointer = {
      owner: { kind: "face_revision", id: "revision" },
      field: "name",
      ordinal: null,
    }
    const pending = resolver.resolve(pointer, "en")
    current = null
    await expect(pending).rejects.toThrow("snapshot replaced")
    expect(resolver.peek(pointer, "en")).toBeUndefined()
  })
  it("rejects an absent owner rather than borrowing annotations of the same text", async () => {
    const { client } = await annotatedOrigin()
    await client.load()
    await expect(
      createResolver(client).resolve(
        { owner: { kind: "face_revision", id: "other" }, field: "name", ordinal: null },
        "zh-Hant",
      ),
    ).rejects.toThrow()
  })
  it("matches complete printing names, flavor and sections against full initialization", async () => {
    const { client } = await annotatedOrigin()
    await client.load()
    const effectPointer = {
      owner: { kind: "face_revision", id: "revision" },
      field: "effect",
      ordinal: null,
    }
    const effect = await createAnnotatedTextResolver(client).resolve(effectPointer, "ja")
    if (!effect) throw new Error("synthetic effect missing")
    const unitId = stringValue(effect.original.unit["id"])
    const patched = {
      ...client,
      fragments: async (file: string) =>
        (await client.fragments(file)).map((fragment) =>
          fragment.table === "printing"
            ? {
                ...fragment,
                rows: fragment.rows.map((row) => ({
                  ...row,
                  faces: arrayValue(row["faces"]).map((raw) => ({
                    ...objectValue(raw),
                    name_concept_id: null,
                    printed_name_unit_id: unitId,
                    printed_effect_unit_id: unitId,
                    flavor_unit_id: unitId,
                    printed_text_state: "verified",
                    sections: [{ ordinal: 3, label_unit_id: unitId, text_unit_id: unitId }],
                  })),
                })),
              }
            : fragment,
        ),
    }
    const actual = createResolver(patched)
    const full = createAnnotatedTextResolver(patched)
    await actual.resolve(effectPointer, "ja")
    await full.resolve(effectPointer, "ja")
    for (const field of ["name", "effect", "flavor", "section"])
      for (const language of ["ja", "en", "zh-Hant"]) {
        const pointer = {
          owner: { kind: "printing_face", id: "printing", face_id: "face" },
          field,
          ordinal: field === "section" ? 3 : null,
        }
        const value = await actual.resolve(pointer, language)
        expect(value).toEqual(await full.resolve(pointer, language))
        expect(value?.original.unit["id"]).toBe(unitId)
        expect(value?.printedState).toBe("verified")
        if (field === "effect")
          expect(value?.originalSections).toEqual([{ ordinal: 3, label: "Synthetic text" }])
      }
  })
  it.each(["name", "effect", "flavor"])(
    "keeps an unknown printed %s separate from valid current text",
    async (field) => {
      const { client } = await annotatedOrigin()
      await client.load()
      const resolver = createResolver(client)
      expect(
        await resolver.resolve(
          {
            owner: { kind: "printing_face", id: "printing", face_id: "face" },
            field,
            ordinal: null,
          },
          "zh-Hant",
        ),
      ).toBeNull()
    },
  )
})
