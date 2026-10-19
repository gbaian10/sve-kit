import { screen, waitFor, within } from "@testing-library/react"
import userEvent from "@testing-library/user-event"
import { afterEach, beforeEach, describe, expect, it, vi } from "vitest"

import type { CardSummary, Catalog, SnapshotClient } from "../../data"
import { DEFAULT_PREFS, prefsStore } from "../../settings"
import { renderInRouter } from "../../test-utils"
import { type PrintingEntry, RelatedCards } from "./RelatedCards"

const state = vi.hoisted(() => {
  /** Card ids named by texts other than the front effect, keyed `<revision>/<field>/<ordinal>`. */
  const texts: Record<string, string[]> = {}
  return {
    rows: [] as { from_card_id: string; to_card_id: string }[],
    failRows: false,
    failPin: false,
    /** Card ids named by the front face's effect text. */
    mentions: [] as string[],
    texts,
    loads: [] as string[],
    pins: [] as (readonly string[])[],
  }
})

vi.mock("../../app/card-related", () => ({
  loadCardRelated: (_client: unknown, _snapshot: unknown, homeSetId: string) => {
    state.loads.push(homeSetId)
    return state.failRows
      ? Promise.reject(new Error("synthetic engine failure"))
      : Promise.resolve(state.rows.map((row, index) => ({ id: `r${String(index)}`, ...row })))
  },
}))

vi.mock("../../app/printing-snapshot", () => ({
  relatedPrintings: (client: unknown, _snapshot: unknown, catalog: unknown, cardIds: string[]) => {
    state.pins.push(cardIds)
    return state.failPin
      ? Promise.reject(new Error("synthetic print failure"))
      : Promise.resolve({ client, catalog })
  },
}))

vi.mock("../../app/snapshot", () => ({
  useImageIndex: () => undefined,
  useAnnotatedTexts: (
    _client: unknown,
    pointers: readonly {
      owner: { id: string }
      field: string
      ordinal: number | null
    }[],
  ) =>
    pointers.map(({ owner, field, ordinal }) => {
      const key = `${owner.id}/${field}/${String(ordinal)}`
      const ids = key === "rev:base/effect/null" ? state.mentions : state.texts[key]
      if (ids === undefined) return { readiness: { basic: "none" }, value: null }
      const prefix = `${owner.id}/section/`
      const sections = Object.keys(state.texts)
        .filter((entry) => entry.startsWith(prefix))
        .map((entry) => ({ ordinal: Number(entry.slice(prefix.length)) }))
      return {
        readiness: { basic: "ready", source: "none", annotations: "ready" },
        value: {
          ...(field === "effect" && sections.length > 0 ? { originalSections: sections } : {}),
          original: {
            unit: { text: "x".repeat(ids.length), lang: "ja" },
            annotation: {
              occurrences: ids.map((id, index) => ({
                reference: { kind: "card_name", term_id: `term:name.${id}` },
                ranges: [{ start: index, end: index + 1 }],
                bold: true,
              })),
            },
          },
          concepts: ids.map((id) => ({
            id: `term:name.${id}`,
            category: "card_name",
            explanations: [],
            card_ids: [`c:${id}`],
          })),
          explanations: [],
          cards: ids.map((id) => summary(id)),
        },
      }
    }),
}))

const NAMES: Record<string, string> = {
  base: "Synthetic base",
  evolve: "Synthetic base",
  evolve2: "Synthetic other evolve",
  token: "Synthetic token",
  other: "Synthetic other",
  reprint: "Synthetic base",
}
const NUMBERS: Record<string, string> = {
  base: "BP05-052",
  evolve: "BP05-053",
  "evolve-r": "BP20-R08",
  "evolve-sl": "BP05-SL11",
  evolve2: "BP05-055",
  token: "PR-108",
  other: "BP05-070",
  reprint: "EBD02-005",
}
/** Printing ids per card, in chip order once sorted: plain numbers first, premium last. */
const PRINTINGS: Record<string, string[]> = {
  "c:base": ["base", "reprint"],
  "c:evolve": ["evolve-sl", "evolve-r", "evolve"],
}
const cardOf = (id: string) =>
  id === "reprint" ? "c:base" : id.startsWith("evolve-") ? "c:evolve" : `c:${id}`

function summary(id: string): CardSummary {
  return {
    cardId: cardOf(id),
    printingId: `p:${id}`,
    faceId: `f:${id}`,
    cardNo: NUMBERS[id] ?? id,
    region: "jp",
    classCode: null,
    name: {
      original: { lang: "ja", text: NAMES[id] ?? NAMES[cardOf(id).slice(2)] ?? id },
      translations: {},
    },
    cost: null,
    attack: null,
    defense: null,
  }
}

const catalog = {
  index: {
    card: (id: string) => ({
      id,
      home_set_id: "BP05",
      regions: [{ region: "jp", default_printing_id: `p:${id.slice(2)}` }],
    }),
    facesOf: (id: string) => [{ id: `f:${id.slice(2)}`, ordinal: 0 }],
    displayRevision: (faceId: string) => ({
      type_code: "follower",
      special_kinds:
        faceId === "f:evolve" || faceId === "f:evolve2"
          ? ["evolve"]
          : faceId === "f:token"
            ? ["token"]
            : [],
    }),
    printingsOf: (id: string) =>
      (PRINTINGS[id] ?? [id.slice(2)]).map((printing) => ({
        id: `p:${printing}`,
        card_id: id,
        region: "jp",
        card_no: NUMBERS[printing] ?? printing,
        rarity_code: "LG",
        premium: printing.endsWith("-sl"),
      })),
  },
  summary: (printingId: string) => summary(printingId.slice(2)),
} as unknown as Catalog

const client = {
  base: "/cdn",
  snapshot: () => ({ manifestHash: "sha256:synthetic" }),
} as unknown as SnapshotClient

const printings = (...ids: string[]): PrintingEntry[] =>
  ids.map((id) => ({
    summary: summary(id),
    rarity: id === "base" ? "傳說" : undefined,
    link: { to: `/cards/${NUMBERS[id] ?? id}`, state: null },
  }))

async function open(
  entries: PrintingEntry[] = [],
  cardId = "c:base",
  revisionIds: string[] = ["rev:base"],
) {
  return renderInRouter(
    <RelatedCards
      client={client}
      catalog={catalog}
      edition="jp"
      cardId={cardId}
      printingId="p:base"
      revisionIds={revisionIds}
      printings={entries}
      linkTo={(card) => ({ to: `/cards/${card.cardNo}`, state: null })}
    />,
  )
}

beforeEach(() => {
  state.rows = []
  state.failRows = false
  state.failPin = false
  state.mentions = []
  state.texts = {}
  state.loads = []
  state.pins = []
})
afterEach(() => {
  prefsStore.set(DEFAULT_PREFS)
})

describe("RelatedCards", () => {
  it("renders nothing for a card with no relations, mentions or other printings", async () => {
    await open()
    await waitFor(() => {
      expect(state.loads).toEqual(["BP05"])
    })
    expect(screen.queryByRole("tablist")).not.toBeInTheDocument()
  })

  it("shows only the tabs with content and opens the first one, in evolve → reference → printings order", async () => {
    state.rows = [
      { from_card_id: "c:base", to_card_id: "c:evolve" },
      { from_card_id: "c:base", to_card_id: "c:other" },
    ]
    state.mentions = ["token"]
    await open(printings("base", "reprint"))
    const tablist = await screen.findByRole("tablist", { name: "關聯卡牌" })
    expect(
      within(tablist)
        .getAllByRole("tab")
        .map((tab) => tab.textContent),
    ).toEqual(["進化後", "參考", "其他版本"])
    expect(screen.getByRole("tab", { name: "進化後" })).toHaveAttribute("aria-selected", "true")
    const panel = screen.getByRole("tabpanel")
    // The number is the visible label; the name lives in the accessible name and the tooltip.
    const evolve = within(panel).getByRole("link", { name: "Synthetic base（BP05-053）" })
    expect(evolve).toHaveTextContent("BP05-053")
    expect(evolve).not.toHaveTextContent("Synthetic base")
    expect(evolve).toHaveAttribute("title", "Synthetic base")
    expect(evolve).toHaveAttribute("href", "/cards/BP05-053")
    // Every printing of the evolve card in the shown edition, in the order of the printing chips.
    expect(
      within(panel)
        .getAllByRole("link")
        .map((link) => link.getAttribute("href")),
    ).toEqual(["/cards/BP05-053", "/cards/BP20-R08", "/cards/BP05-SL11"])
    // The bands are pinned for this card first, then for its evolve pairs as well.
    expect(state.pins).toEqual([["c:base"], ["c:base", "c:evolve"]])
  })

  it("lists what an evolve card evolves from and sends its other relations to the references", async () => {
    state.rows = [
      { from_card_id: "c:evolve", to_card_id: "c:base" },
      { from_card_id: "c:evolve", to_card_id: "c:token" },
    ]
    await open([], "c:evolve", ["rev:evolve"])
    const tablist = await screen.findByRole("tablist", { name: "關聯卡牌" })
    expect(
      within(tablist)
        .getAllByRole("tab")
        .map((tab) => tab.textContent),
    ).toEqual(["進化前", "參考"])
    expect(
      within(screen.getByRole("tabpanel"))
        .getAllByRole("link")
        .map((link) => link.getAttribute("href")),
    ).toEqual(["/cards/BP05-052", "/cards/EBD02-005"])
  })

  it("treats a differently named evolve card as a reference unless its own rows single this card out", async () => {
    state.rows = [{ from_card_id: "c:base", to_card_id: "c:evolve2" }]
    const first = await open()
    await screen.findByRole("tablist")
    expect(screen.queryByRole("tab", { name: "進化後" })).not.toBeInTheDocument()
    const official = screen.getByRole("list", { name: "其他官方關聯" })
    expect(within(official).getByRole("link")).toHaveTextContent("BP05-055")
    first.unmount()
    // The renamed evolve card's rows sit in the same band: this card is its only plain follower.
    state.rows = [
      { from_card_id: "c:base", to_card_id: "c:evolve2" },
      { from_card_id: "c:evolve2", to_card_id: "c:base" },
      { from_card_id: "c:evolve2", to_card_id: "c:token" },
    ]
    const second = await open()
    expect(await screen.findByRole("tab", { name: "進化後" })).toHaveAttribute(
      "aria-selected",
      "true",
    )
    expect(within(screen.getByRole("tabpanel")).getByRole("link")).toHaveTextContent("BP05-055")
    second.unmount()
    await open([], "c:evolve2", ["rev:evolve2"])
    expect(await screen.findByRole("tab", { name: "進化前" })).toBeInTheDocument()
    expect(
      within(screen.getByRole("tabpanel"))
        .getAllByRole("link")
        .map((l) => l.textContent),
    ).toEqual(expect.arrayContaining([expect.stringContaining("BP05-052")]))
  })

  it("falls back to the evolve card's representative printing when the bands cannot be pinned", async () => {
    state.rows = [{ from_card_id: "c:base", to_card_id: "c:evolve" }]
    state.failPin = true
    await open(printings("base", "reprint"))
    await screen.findByRole("tablist")
    expect(screen.getByRole("tab", { name: "進化後" })).toHaveAttribute("aria-selected", "true")
    expect(
      within(screen.getByRole("tabpanel"))
        .getAllByRole("link")
        .map((link) => link.getAttribute("href")),
    ).toEqual(["/cards/BP05-053"])
    expect(screen.queryByRole("alert")).not.toBeInTheDocument()
    expect(screen.getByRole("tab", { name: "其他版本" })).toBeInTheDocument()
  })

  it("splits the reference tab into this card's mentions and the other official relations", async () => {
    state.rows = [
      { from_card_id: "c:base", to_card_id: "c:other" },
      { from_card_id: "c:base", to_card_id: "c:token" },
    ]
    state.mentions = ["token"]
    await open()
    const user = userEvent.setup()
    await screen.findByRole("tablist")
    expect(screen.queryByRole("tab", { name: "進化後" })).not.toBeInTheDocument()
    expect(screen.getByRole("tab", { name: "參考" })).toHaveAttribute("aria-selected", "true")
    const mentions = screen.getByRole("list", { name: "這張卡提到" })
    expect(within(mentions).getByRole("link")).toHaveTextContent("PR-108")
    const official = screen.getByRole("list", { name: "其他官方關聯" })
    expect(within(official).getByRole("link")).toHaveTextContent("BP05-070")
    // Arrow keys move between tabs like a radio group; the panel follows.
    await user.click(screen.getByRole("tab", { name: "參考" }))
    expect(screen.getByRole("tabpanel")).toHaveAccessibleName("參考")
  })

  it("lists the other printings with the current one marked and the rarity under the number", async () => {
    await open(printings("base", "reprint"))
    const user = userEvent.setup()
    await screen.findByRole("tablist")
    expect(screen.getByRole("tab", { name: "其他版本" })).toHaveAttribute("aria-selected", "true")
    const list = screen.getByRole("list", { name: "其他版本" })
    const current = within(list).getByRole("link", { current: "page" })
    expect(current).toHaveTextContent("BP05-052")
    expect(current).toHaveTextContent("傳說")
    expect(within(list).getByRole("link", { name: "Synthetic base（EBD02-005）" })).toHaveAttribute(
      "href",
      "/cards/EBD02-005",
    )
    await user.click(within(list).getByRole("link", { name: "Synthetic base（EBD02-005）" }))
  })

  it("hides a single printing and keeps the tab the reader picked", async () => {
    state.rows = [{ from_card_id: "c:base", to_card_id: "c:evolve" }]
    state.mentions = ["token"]
    await open(printings("base"))
    const user = userEvent.setup()
    await screen.findByRole("tablist")
    expect(screen.queryByRole("tab", { name: "其他版本" })).not.toBeInTheDocument()
    await user.click(screen.getByRole("tab", { name: "參考" }))
    expect(screen.getByRole("tab", { name: "參考" })).toHaveAttribute("aria-selected", "true")
    expect(screen.getByRole("tab", { name: "進化後" })).toHaveAttribute("tabindex", "-1")
    await user.keyboard("{ArrowLeft}")
    expect(screen.getByRole("tab", { name: "進化後" })).toHaveAttribute("aria-selected", "true")
    expect(screen.getByRole("tab", { name: "進化後" })).toHaveFocus()
  })

  it("reports a failed relation read with a retry and still shows the other printings", async () => {
    state.failRows = true
    await open(printings("base", "reprint"))
    const user = userEvent.setup()
    expect(await screen.findByRole("alert")).toHaveTextContent("關聯卡牌載入失敗")
    expect(screen.getByRole("tab", { name: "其他版本" })).toBeInTheDocument()
    state.failRows = false
    state.rows = [{ from_card_id: "c:base", to_card_id: "c:evolve" }]
    await user.click(screen.getByRole("button", { name: "重試" }))
    expect(await screen.findByRole("tab", { name: "進化後" })).toBeInTheDocument()
    expect(screen.queryByRole("alert")).not.toBeInTheDocument()
  })

  it("orders mentions face by face, each face's effect before its sections", async () => {
    state.mentions = ["other"]
    state.texts = {
      "rev:base/section/1": ["token"],
      "rev:back/effect/null": ["evolve"],
      "rev:back/section/2": ["other", "another"],
    }
    await open([], "c:base", ["rev:base", "rev:back"])
    const list = await screen.findByRole("list", { name: "這張卡提到" })
    expect(
      within(list)
        .getAllByRole("link")
        .map((link) => link.getAttribute("href")),
    ).toEqual(["/cards/BP05-070", "/cards/PR-108", "/cards/BP05-053", "/cards/another"])
  })

  it("never lists the card itself among its relations", async () => {
    state.rows = [{ from_card_id: "c:base", to_card_id: "c:base" }]
    state.mentions = ["base"]
    await open()
    await waitFor(() => {
      expect(state.loads).toEqual(["BP05"])
    })
    expect(screen.queryByRole("tablist")).not.toBeInTheDocument()
  })
})
