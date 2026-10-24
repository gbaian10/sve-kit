import { render, screen, within } from "@testing-library/react"
import { afterEach, expect, it } from "vitest"

import { buildSnapshot } from "../../../scripts/fixture/build"
import { type Card, CARDS } from "../../../scripts/fixture/cards"
import { InlineText } from "../../components/card/InlineText"
import { createSnapshotClient, loadCardErrata } from "../../data"
import { DEFAULT_PREFS, prefsStore } from "../../settings"
import { renderInRouter } from "../../test-utils"
import { annotatedOrigin } from "../../test-utils/annotated-snapshot"
import { memoryCache } from "../../test-utils/cache"
import { CardErrata } from "./CardErrata"

afterEach(() => prefsStore.set(DEFAULT_PREFS))

async function renderErrata(cards: readonly Card[] = CARDS) {
  const built = await buildSnapshot({
    cards,
    encodeImage: () => Promise.resolve(new Uint8Array([1])),
  })
  const client = createSnapshotClient("https://cdn.test", {
    region: "jp",
    cacheStorage: memoryCache(),
    fetch: (url) => {
      const bytes = built.files.get(url.slice("https://cdn.test/".length))
      return Promise.resolve(
        bytes ? new Response(bytes.slice().buffer) : new Response(null, { status: 404 }),
      )
    },
  })
  await client.load()
  await renderInRouter(
    <CardErrata
      client={client}
      cardId="c:bp01-020"
      printingId="p:bp01-020"
      faceId="f:bp01-020"
      region="jp"
    />,
  )
  return screen.findByRole("region", { name: "勘誤" })
}

it("shows reconstructed full text and pairs excerpt translations with marked originals", async () => {
  const notice = await renderErrata()
  expect(notice.querySelector("time")).toBeNull()
  expect(within(notice).queryByText(/2026-03-01/u)).not.toBeInTheDocument()
  expect(within(notice).getByRole("link", { name: "官方勘誤公告" })).toHaveAttribute(
    "href",
    "https://example.invalid/errata/bp01-020",
  )
  expect(notice.querySelectorAll("del")).toHaveLength(1)
  expect(within(notice).getAllByText("繁中・日文來源機器譯文（非官方）")).toHaveLength(1)
  expect(notice.querySelectorAll("ins")).toHaveLength(1)
  expect(notice.querySelectorAll('[lang="zh-Hant"] del, [lang="zh-Hant"] ins')).toHaveLength(0)
  for (const translation of notice.querySelectorAll('[lang="zh-Hant"]')) {
    const original = translation.nextElementSibling
    expect(original).toHaveAttribute("data-original")
    expect(original?.querySelector('[lang="ja"]')).not.toBeNull()
  }
  expect(notice.querySelectorAll("del, ins").length).toBe(
    notice.querySelectorAll("[data-original] del, [data-original] ins").length,
  )
  expect(
    within(notice).getByRole("heading", { name: "勘誤前完整文字（依公告還原）" }),
  ).toBeVisible()
  expect(within(notice).getByRole("heading", { name: "勘誤後完整文字" })).toBeVisible()
  expect(within(notice).getByText(/此版次的完整印刷文字尚未確認/u)).toBeVisible()
  expect(within(notice).getByText(/BP01-020（印刷文字未確認）/u)).toBeVisible()
})

it("hides excerpt translations when original-only is preferred", async () => {
  prefsStore.set({ effectLanguage: "original" })
  const notice = await renderErrata()
  expect(notice.querySelector('[lang="zh-Hant"]')).toBeNull()
  expect(notice.querySelectorAll("[data-original]")).toHaveLength(2)
})

it.each([true, false])(
  "pairs source lines only when translation line counts match (%s)",
  async (matching) => {
    const cards = CARDS.map((card) =>
      card.id === "c:bp01-020"
        ? {
            ...card,
            faces: card.faces.map((face) => ({
              ...face,
              previousEffectJa: "Old A.\nKeep B.",
              previousEffectZh: matching ? "舊 A。\n保留 B。" : "整段舊譯。",
              effect: {
                ...face.effect,
                ja: "New A.\nKeep B.",
                zhHant: matching ? "新 A。\n保留 B。" : "整段新譯。",
              },
            })),
          }
        : card,
    )
    const notice = await renderErrata(cards)
    expect(notice.querySelectorAll("[data-line]")).toHaveLength(matching ? 4 : 0)
    expect(notice.querySelectorAll('[lang="zh-Hant"] del, [lang="zh-Hant"] ins')).toHaveLength(0)
    expect(
      [...notice.querySelectorAll("del, ins")].some((node) => node.textContent.includes("Keep")),
    ).toBe(false)
    if (matching)
      for (const line of notice.querySelectorAll("[data-line]")) {
        expect(line.querySelector('[lang="zh-Hant"] + [data-original] [lang="ja"]')).not.toBeNull()
      }
  },
)

it("emphasizes explicit keyword headings without inventing historical annotation references", () => {
  const { container } = render(<InlineText text="【連擊 5】改為選擇。" emphasis />)
  expect(container.querySelector("strong")).toHaveTextContent("【連擊 5】")
})

it("returns no notices for an edition absent from the snapshot", async () => {
  const { client } = await annotatedOrigin()
  await client.load()
  const snapshot = client.snapshot()
  if (!snapshot) throw new Error("missing snapshot")
  await expect(loadCardErrata(client, snapshot, "card", "printing", "face", "en")).resolves.toEqual(
    [],
  )
})
