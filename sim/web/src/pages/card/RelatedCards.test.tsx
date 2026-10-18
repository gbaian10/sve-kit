import { screen, waitFor, within } from "@testing-library/react"
import userEvent from "@testing-library/user-event"
import { afterEach, beforeEach, describe, expect, it, vi } from "vitest"

import type { CardSummary, Catalog, SnapshotClient } from "../../data"
import { DEFAULT_PREFS, prefsStore } from "../../settings"
import { renderInRouter } from "../../test-utils"
import { type PrintingEntry, RelatedCards } from "./RelatedCards"

const state = vi.hoisted(() => ({
  rows: [] as { from_card_id: string; to_card_id: string }[],
  failRows: false,
  mentions: [] as string[],
  loads: [] as string[],
}))

vi.mock("../../app/card-related", () => ({
  loadCardRelated: (_client: unknown, _snapshot: unknown, cardId: string) => {
    state.loads.push(cardId)
    return state.failRows
      ? Promise.reject(new Error("synthetic engine failure"))
      : Promise.resolve(state.rows.map((row, index) => ({ id: `r${String(index)}`, ...row })))
  },
}))

vi.mock("../../app/snapshot", () => ({
  useImageIndex: () => undefined,
  useAnnotatedTexts: (_client: unknown, pointers: readonly { field: string }[]) =>
    pointers.map(({ field }) => ({
      readiness: { basic: "ready", source: "none", annotations: "ready" },
      value:
        field === "effect"
          ? {
              original: {
                unit: { text: "x".repeat(state.mentions.length), lang: "ja" },
                annotation: {
                  occurrences: state.mentions.map((id, index) => ({
                    reference: { kind: "card_name", term_id: `term:name.${id}` },
                    ranges: [{ start: index, end: index + 1 }],
                    bold: true,
                  })),
                },
              },
              concepts: state.mentions.map((id) => ({
                id: `term:name.${id}`,
                category: "card_name",
                explanations: [],
                card_ids: [`c:${id}`],
              })),
              explanations: [],
              cards: state.mentions.map((id) => summary(id)),
            }
          : null,
    })),
}))

const NAMES: Record<string, string> = {
  base: "Synthetic base",
  evolve: "Synthetic evolve",
  token: "Synthetic token",
  other: "Synthetic other",
  reprint: "Synthetic base",
}
const NUMBERS: Record<string, string> = {
  base: "BP05-052",
  evolve: "BP05-053",
  token: "PR-108",
  other: "BP05-070",
  reprint: "EBD02-005",
}

function summary(id: string): CardSummary {
  return {
    cardId: id === "reprint" ? "c:base" : `c:${id}`,
    printingId: `p:${id}`,
    faceId: `f:${id}`,
    cardNo: NUMBERS[id] ?? id,
    region: "jp",
    classCode: null,
    name: { original: { lang: "ja", text: NAMES[id] ?? id }, translations: {} },
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
    facesOf: (id: string) => [{ id: `f:${id.slice(2)}` }],
    displayRevision: (faceId: string) => ({
      special_kinds: faceId === "f:evolve" ? ["evolve"] : [],
    }),
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

async function open(entries: PrintingEntry[] = [], cardId = "c:base") {
  return renderInRouter(
    <RelatedCards
      client={client}
      catalog={catalog}
      edition="jp"
      cardId={cardId}
      printingId="p:base"
      revisionIds={["rev:base"]}
      printings={entries}
      linkTo={(card) => ({ to: `/cards/${card.cardNo}`, state: null })}
    />,
  )
}

beforeEach(() => {
  state.rows = []
  state.failRows = false
  state.mentions = []
  state.loads = []
})
afterEach(() => {
  prefsStore.set(DEFAULT_PREFS)
})

describe("RelatedCards", () => {
  it("renders nothing for a card with no relations, mentions or other printings", async () => {
    await open()
    await waitFor(() => {
      expect(state.loads).toEqual(["c:base"])
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
    ).toEqual(["進化", "參考", "其他版本"])
    expect(screen.getByRole("tab", { name: "進化" })).toHaveAttribute("aria-selected", "true")
    const panel = screen.getByRole("tabpanel")
    // The number is the visible label; the name lives in the accessible name and the tooltip.
    const evolve = within(panel).getByRole("link", { name: "Synthetic evolve（BP05-053）" })
    expect(evolve).toHaveTextContent("BP05-053")
    expect(evolve).not.toHaveTextContent("Synthetic evolve")
    expect(evolve).toHaveAttribute("title", "Synthetic evolve")
    expect(evolve).toHaveAttribute("href", "/cards/BP05-053")
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
    expect(screen.queryByRole("tab", { name: "進化" })).not.toBeInTheDocument()
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
    expect(screen.getByRole("tab", { name: "進化" })).toHaveAttribute("tabindex", "-1")
    await user.keyboard("{ArrowLeft}")
    expect(screen.getByRole("tab", { name: "進化" })).toHaveAttribute("aria-selected", "true")
    expect(screen.getByRole("tab", { name: "進化" })).toHaveFocus()
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
    expect(await screen.findByRole("tab", { name: "進化" })).toBeInTheDocument()
    expect(screen.queryByRole("alert")).not.toBeInTheDocument()
  })

  it("never lists the card itself among its relations", async () => {
    state.rows = [{ from_card_id: "c:base", to_card_id: "c:base" }]
    state.mentions = ["base"]
    await open()
    await waitFor(() => {
      expect(state.loads).toEqual(["c:base"])
    })
    expect(screen.queryByRole("tablist")).not.toBeInTheDocument()
  })
})
