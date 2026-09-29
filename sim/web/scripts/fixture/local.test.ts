// @vitest-environment node
import { describe, expect, it } from "vitest"

import { type JsonObject, stringValue } from "../../src/data/format-v1/json"
import { readSnapshot } from "../../src/data/format-v1/reader"
import { buildSnapshot } from "./build"
import { convertLocal, type LocalRecord, outputTargetError } from "./local"

// Synthetic records in the shape of the private list; no official text is committed.
const face = (over: Partial<LocalRecord["faces"][number]> = {}): LocalRecord["faces"][number] => ({
  name: "試作のテスト",
  card_class: "ロイヤル",
  card_type: "フォロワー",
  traits: ["兵士"],
  rarity: "BR",
  product: "試作パック",
  cost: "2",
  power: "2",
  hp: "2",
  text: "【ファンファーレ】テスト。",
  flavor: "風味。",
  illustrator: "画家A",
  image: "/images/cardlist/TS01/ts01-001.png",
  ...over,
})
const RECORDS: readonly LocalRecord[] = [
  {
    number: "TS01-001",
    faces: [face()],
    release_date: "2026-01-01",
    qa: [{ id: "Q1", date: "2026-02-01", question: "質問？", answer: "回答。" }],
  },
  {
    number: "TS01-002",
    faces: [
      face({
        name: "試作の裏表",
        cost: "-",
        rarity: "GR・プレミアム",
        image: "/images/cardlist/TS01/ts01-002.png",
      }),
      face({
        name: "試作の裏",
        card_type: "フォロワー・エボルヴ",
        cost: "-",
        image: "/images/cardlist/TS01/ts01-002_ura.png",
      }),
    ],
    release_date: "2026-01-01",
    qa: [{ id: "Q1", date: "2026-02-01", question: "質問？", answer: "回答。" }],
  },
  {
    number: "PR-001",
    faces: [
      face({
        card_class: "ニュートラル",
        card_type: "スペル",
        rarity: "-",
        traits: [],
        illustrator: null,
        flavor: null,
        image: "/images/cardlist/PR/pr-001.png",
      }),
    ],
    release_date: null,
    qa: [],
  },
  {
    number: "PR-002",
    faces: [
      face({
        card_class: "-",
        card_type: "リーダー",
        rarity: null,
        traits: null,
        text: null,
        cost: null,
        power: null,
        hp: "20",
        product: null,
        image: "/images/cardlist/PR/pr-002.png",
      }),
    ],
    release_date: null,
    qa: [],
  },
]
const stubImage = ({
  width,
  height,
  seed,
  source,
}: {
  width: number
  height: number
  seed: number
  source: string | null
}) =>
  Promise.resolve(
    new TextEncoder().encode(
      `img:${String(width)}x${String(height)}:${String(seed)}:${source ?? "-"}`,
    ),
  )

describe("convertLocal", () => {
  const converted = convertLocal(RECORDS, {
    imagePath: (set, url) =>
      url.endsWith("ura.png") ? null : `/img/${set}/${url.split("/").at(-1) ?? ""}`,
  })

  it("maps records to cards, families and observed vocabulary", () => {
    expect(converted.cards.map((card) => card.id)).toEqual([
      "c:ts01-001",
      "c:ts01-002",
      "c:pr-001",
      "c:pr-002",
    ])
    expect(Object.keys(converted.families)).toEqual(["set:ts01", "set:pr"])
    expect(converted.families["set:pr"]?.kind).toBe("promo")
    expect(converted.vocabulary.types).toMatchObject({
      follower: { ja: "フォロワー" },
      evolved: { ja: "フォロワー・エボルヴ" },
      spell: { ja: "スペル" },
    })
    expect(converted.vocabulary.traits).toEqual({ trait_1: { ja: "兵士" } })
    const second = converted.cards[1]
    expect(second?.layout).toBe("double_faced")
    expect(second?.printings[0]).toMatchObject({
      rarity: "gold",
      premium: true,
      imagePaths: ["/img/TS01/ts01-002.png", null],
    })
    expect(second?.faces[0]?.cost).toBeNull()
    expect(converted.cards[2]?.class).toBeNull()
    expect(converted.cards[2]?.printings[0]?.rarity).toBeNull()
  })

  it("shares a question between every card that carries it and keeps its date", () => {
    expect(converted.cards[0]?.qa?.[0]).toMatchObject({
      cards: ["c:ts01-001", "c:ts01-002"],
      publishedOn: "2026-02-01",
    })
    expect(converted.cards[1]?.qa).toBeUndefined()
  })

  it("gives every unknown type label its own code", () => {
    const record = (number: string, type: string): LocalRecord => ({
      number,
      faces: [face({ card_type: type })],
      release_date: null,
      qa: [],
    })
    const records = [
      record("TS01-101", "新種類A"),
      record("TS01-102", "新種類B"),
      record("TS01-103", "新種類A"),
    ]
    const result = convertLocal(records, { imagePath: () => null })
    expect(result.cards.map((card) => card.faces[0]?.type)).toEqual(["type_1", "type_2", "type_1"])
    expect(result.vocabulary.types).toEqual({
      type_1: { ja: "新種類A" },
      type_2: { ja: "新種類B" },
    })
  })

  it("filters by set and limit", () => {
    expect(convertLocal(RECORDS, { sets: ["PR"], imagePath: () => null }).cards).toHaveLength(2)
    expect(converted.cards[3]?.faces[0]).toMatchObject({
      effect: { ja: "" },
      cost: null,
      defense: 20,
      traits: [],
    })
    expect(convertLocal(RECORDS, { limit: 2, imagePath: () => null }).cards).toHaveLength(2)
  })

  it("builds a snapshot the reader accepts", async () => {
    const snapshot = await buildSnapshot({ encodeImage: stubImage, synthetic: false, ...converted })
    const payloads = new Map<string, Uint8Array>()
    for (const file of snapshot.manifest["files"] as JsonObject[]) {
      payloads.set(
        stringValue(file["key"]),
        snapshot.files.get(stringValue(file["path"])) ?? new Uint8Array(),
      )
    }
    const view = readSnapshot(snapshot.manifest, payloads)
    expect(view["card"]).toHaveLength(4)
    expect(view["qa"]).toHaveLength(1)
    expect(view["qa_version"]?.[0]?.["cards"] as string[]).toEqual(["c:ts01-001", "c:ts01-002"])
    expect(snapshot.manifest["regions"]).toEqual(["jp"])
    expect(snapshot.manifest["languages"]).toEqual(["en", "ja", "zh-Hant"])
    expect(view["artist"]?.map((row) => row["display_name"])).toEqual(["画家A"])
    expect(view["keyword"]).toEqual([])
    expect(view["qa_version"]?.[0]?.["published_on"]).toBe("2026-02-01")
    // The back face of the second card has no source image: it is marked missing, not painted.
    const assets = new Map(view["image_asset"]?.map((row) => [row["id"], row["availability"]]))
    expect(assets.get("img:ts01-002:0")).toBe("available")
    expect(assets.get("img:ts01-002:1")).toBe("missing")
    expect(assets.get("img:pr-001:0")).toBe("available")
    expect(view["image_variant"]?.some((row) => row["image_id"] === "img:ts01-002:1")).toBe(false)
  })
})

describe("outputTargetError", () => {
  const protectedPaths = {
    repoRoot: "/home/me/sve-kit",
    dataDir: "/home/me/sve-kit-data",
    listDir: "/media/backup/cards",
  }
  it("refuses the repository, its parents, the data root and the list directory", () => {
    expect(outputTargetError("/home/me/sve-kit/sim/web", protectedPaths)).toMatch(
      /inside the repository/,
    )
    expect(outputTargetError("/home/me/sve-kit", protectedPaths)).toMatch(/inside the repository/)
    expect(outputTargetError("/home/me", protectedPaths)).toMatch(/contains the repository/)
    expect(outputTargetError("/", protectedPaths)).toMatch(/contains the repository/)
    expect(outputTargetError("/home/me/sve-kit-data", protectedPaths)).toMatch(/data directory/)
    expect(outputTargetError("/media/backup/cards", protectedPaths)).toMatch(/card list/)
    expect(outputTargetError("/media/backup/cards/out", protectedPaths)).toMatch(/card list/)
    expect(outputTargetError("/media/backup", protectedPaths)).toMatch(/card list/)
  })
  it("accepts a directory beside or under the data root", () => {
    expect(outputTargetError("/home/me/sve-kit-data/derived/web-cdn", protectedPaths)).toBeNull()
    expect(outputTargetError("/home/me/sve-kit-web-cdn", protectedPaths)).toBeNull()
    expect(outputTargetError("/tmp/cdn", protectedPaths)).toBeNull()
  })
})
