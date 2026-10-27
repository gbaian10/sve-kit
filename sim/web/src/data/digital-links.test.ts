// @vitest-environment node
import { describe, expect, it } from "vitest"

import type { LoadedSnapshot, SnapshotClient } from "./client"
import { digitalLinkUrl, loadCardDigitalLinks } from "./digital-links"
import type { Row } from "./format-v3/decode"
import type { JsonObject } from "./format-v3/json"
import type { Fragment } from "./format-v3/reader"

const link = (id: string, digitalCard: string, extra: Row = {}): Row => ({
  id,
  card_id: "c:one",
  face_id: null,
  digital_card_id: digitalCard,
  digital_phase: null,
  relation: "same_card",
  effect_similarity: null,
  review_level: "model_reviewed",
  ...extra,
})
const ENDPOINTS = [
  {
    game: "sv1",
    card_url_template: "https://portal.example/card/{official_id}?lang={provider_lang}",
    language_map: { en: "en", ja: "ja", "zh-Hant": "zh-tw" },
  },
  {
    game: "svwb",
    card_url_template: "https://wb.example/{provider_lang}/card/?card_id={official_id}",
    language_map: { en: "en", ja: "ja", "zh-Hant": "cht" },
  },
]
const config = (endpoints: unknown = ENDPOINTS) =>
  ({ digital_endpoints: endpoints }) as unknown as JsonObject

describe("digitalLinkUrl", () => {
  const sv1 = { game: "sv1", officialId: "900211010" } as const
  const svwb = { game: "svwb", officialId: "90021110" } as const

  it("maps the interface language to each game's own language code", () => {
    expect(digitalLinkUrl(config(), sv1, "zh-TW")).toBe(
      "https://portal.example/card/900211010?lang=zh-tw",
    )
    expect(digitalLinkUrl(config(), svwb, "zh-TW")).toBe(
      "https://wb.example/cht/card/?card_id=90021110",
    )
    expect(digitalLinkUrl(config(), sv1, "ja")).toContain("lang=ja")
    expect(digitalLinkUrl(config(), svwb, "en")).toContain("/en/card/")
  })

  it("encodes the replaced values and leaves the rest of the template alone", () => {
    const url = digitalLinkUrl(config(), { game: "sv1", officialId: "1&x=2#../$&" }, "en")
    expect(url).toBe("https://portal.example/card/1%26x%3D2%23..%2F%24%26?lang=en")
  })

  it("gives no URL when the endpoint, the language or an https template is missing", () => {
    expect(digitalLinkUrl(config([ENDPOINTS[1]]), sv1, "en")).toBeUndefined()
    expect(
      digitalLinkUrl(config([{ ...ENDPOINTS[0], language_map: { ja: "ja" } }]), sv1, "en"),
    ).toBeUndefined()
    expect(
      digitalLinkUrl(
        config([{ ...ENDPOINTS[0], card_url_template: "javascript:alert({official_id})" }]),
        sv1,
        "en",
      ),
    ).toBeUndefined()
    expect(
      digitalLinkUrl(
        config([{ ...ENDPOINTS[0], card_url_template: "http://portal.example/{official_id}" }]),
        sv1,
        "en",
      ),
    ).toBeUndefined()
  })
})

const fragment = (table: string, rows: Row[]): Fragment => ({
  file: "f",
  table,
  value: {},
  rows,
  identity: table,
})
const file = (table: string, owner: string | null): JsonObject => ({
  purpose: "engine",
  row_counts: [[table, owner === null ? "global" : "home_set", owner, "detail", "seg", 1]],
})
const snapshot = {
  manifestHash: "h1",
  files: new Map([
    ["links/BP01", file("digital_link", "BP01")],
    ["links/BP02", file("digital_link", "BP02")],
    ["cards/0", file("digital_card", null)],
  ]),
} as unknown as LoadedSnapshot

function fakeClient(options: { failing?: string; replaced?: boolean } = {}) {
  const read: string[] = []
  const client = {
    snapshot: () => (options.replaced ? ({ manifestHash: "h2" } as LoadedSnapshot) : snapshot),
    fragments: (key: string) => {
      read.push(key)
      if (key === options.failing) return Promise.reject(new Error("network"))
      return Promise.resolve(
        key === "cards/0"
          ? [
              fragment("digital_card", [
                { id: "d:1", game: "sv1", official_id: "900211010" },
                { id: "d:3", game: "svwb", official_id: "90021110" },
              ]),
            ]
          : [fragment("digital_link", [link("l1", "d:1"), link("l2", "d:3")])],
      )
    },
  } as unknown as SnapshotClient
  return { client, read }
}

describe("loadCardDigitalLinks", () => {
  it("loads the card's digital counterparts even when another home set is unavailable", async () => {
    const { client } = fakeClient({ failing: "links/BP02" })
    const result = await loadCardDigitalLinks(client, snapshot, "c:one", "BP01")
    expect(result.map(({ officialId }) => officialId)).toEqual(["900211010", "90021110"])
  })

  it("answers no links without reading digital cards", async () => {
    const { client, read } = fakeClient()
    expect(await loadCardDigitalLinks(client, snapshot, "c:nobody", "BP01")).toEqual([])
    expect(read).toEqual(["links/BP01"])
  })

  it("fails a failed read and a replaced snapshot rather than answering none", async () => {
    await expect(
      loadCardDigitalLinks(fakeClient({ failing: "links/BP01" }).client, snapshot, "c:one", "BP01"),
    ).rejects.toThrow()
    await expect(
      loadCardDigitalLinks(fakeClient({ failing: "cards/0" }).client, snapshot, "c:one", "BP01"),
    ).rejects.toThrow()
    await expect(
      loadCardDigitalLinks(fakeClient({ replaced: true }).client, snapshot, "c:one", "BP01"),
    ).rejects.toThrow("snapshot replaced")
  })
})
