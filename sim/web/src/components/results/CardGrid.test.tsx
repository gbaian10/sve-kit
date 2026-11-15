import { act, screen } from "@testing-library/react"
import { useState } from "react"
import { describe, expect, it, vi } from "vitest"

import type { ImageIndex } from "../../data"
import { renderInRouter, renderRoutes } from "../../test-utils"
import { CardGrid, type GridCell } from "./CardGrid"

function cell(pending: boolean): GridCell {
  return {
    key: "c:synthetic",
    to: "/cards/BP-SYNTHETIC",
    state: null,
    name: { primary: { lang: "ja", text: "Synthetic name" }, missingTranslation: false },
    summary: {
      cardId: "c:synthetic",
      printingId: "p:synthetic",
      faceId: "f:synthetic",
      cardNo: "BP-SYNTHETIC",
      region: "jp",
      classCode: null,
      name: { original: { lang: "ja", text: "Synthetic name" }, translations: {} },
      cost: null,
      attack: null,
      defense: null,
      wordingPending: pending,
    },
  }
}

describe("visible wording label in card results", () => {
  it.each([
    ["zh-TW", "卡文未定"],
    ["ja", "表記未定"],
    ["en", "Wording pending"],
  ] as const)("labels a pending summary in %s", async (language, label) => {
    await renderInRouter(<CardGrid cells={[cell(true)]} images={undefined} onOpen={vi.fn()} />, {
      language,
    })
    expect(screen.getByText(label)).toBeVisible()
    expect(screen.getByText("BP-SYNTHETIC", { selector: "span.whitespace-nowrap" })).toHaveClass(
      "whitespace-nowrap",
    )
  })
  it("does not label a settled summary", async () => {
    await renderInRouter(<CardGrid cells={[cell(false)]} images={undefined} onOpen={vi.fn()} />)
    expect(screen.queryByText("卡文未定")).not.toBeInTheDocument()
  })
})

describe("caption under the card image", () => {
  const restriction = (maxCopies: number | null) => ({
    id: "r:synthetic",
    formatCode: "standard" as const,
    maxCopies,
    effectiveFrom: "2026-01-01",
    sourceUrl: null,
    asOf: "2026-01-01",
  })
  /** Where each text starts in the card link, in reading order; -1 when the link does not show it. */
  const positions = (...texts: readonly string[]) => {
    const shown = screen.getByRole("link").textContent
    return texts.map((text) => shown.indexOf(text))
  }

  it("shows the name and the card number, and no copy limit, for a card with neither a rarity nor a limit", async () => {
    await renderInRouter(<CardGrid cells={[cell(false)]} images={undefined} onOpen={vi.fn()} />)
    expect(screen.getByText("Synthetic name")).toBeVisible()
    expect(screen.getByText("BP-SYNTHETIC")).toBeVisible()
    expect(screen.queryByText("禁止")).not.toBeInTheDocument()
    expect(screen.queryByText(/^限\d/)).not.toBeInTheDocument()
  })

  it("shows the rarity and the copy limit after the names", async () => {
    const value = cell(false)
    const rated: GridCell = {
      ...value,
      name: {
        ...value.name,
        secondary: { lang: "zh-Hant", text: "Synthetic translation" },
      },
      summary: {
        ...value.summary,
        rarity: { original: { lang: "ja", text: "LG" }, translations: {} },
        restrictions: [restriction(1)],
      },
    }
    await renderInRouter(<CardGrid cells={[rated]} images={undefined} onOpen={vi.fn()} />)
    const [name, translation, rarity, limit] = positions(
      "Synthetic name",
      "Synthetic translation",
      "LG",
      "限1",
    )
    expect(name).toBeGreaterThanOrEqual(0)
    expect(translation).toBeGreaterThan(name ?? 0)
    expect(rarity).toBeGreaterThan(translation ?? 0)
    expect(limit).toBeGreaterThan(rarity ?? 0)
  })

  it("shows a ban after the name of a card without a rarity", async () => {
    const value = cell(false)
    const banned: GridCell = {
      ...value,
      summary: { ...value.summary, restrictions: [restriction(0)] },
    }
    await renderInRouter(<CardGrid cells={[banned]} images={undefined} onOpen={vi.fn()} />)
    const [name, ban] = positions("Synthetic name", "禁止")
    expect(name).toBeGreaterThanOrEqual(0)
    expect(ban).toBeGreaterThan(name ?? 0)
  })

  it("shows no copy limit for a restriction that sets none", async () => {
    const value = cell(false)
    const unlimited: GridCell = {
      ...value,
      summary: { ...value.summary, restrictions: [restriction(null)] },
    }
    await renderInRouter(<CardGrid cells={[unlimited]} images={undefined} onOpen={vi.fn()} />)
    expect(screen.queryByText("禁止")).not.toBeInTheDocument()
    expect(screen.queryByText(/^限\d/)).not.toBeInTheDocument()
  })
})

it("shows each grid card from its card_l file without a srcset", async () => {
  const files: ImageIndex = {
    asset: () => ({ availability: "available", publication_state: "approved" }),
    cardImage: () => ({
      src: "/cdn/images/card_l/synthetic.webp?v=1",
      small: "/cdn/images/card_s/synthetic.webp?v=1",
      width: 459,
      height: 641,
    }),
  }
  await renderInRouter(<CardGrid cells={[cell(false)]} images={files} onOpen={vi.fn()} />)

  const image = document.querySelector("img")
  expect(image?.getAttribute("src")).toBe("/cdn/images/card_l/synthetic.webp?v=1")
  expect(image).not.toHaveAttribute("srcset")
})

describe("card results omit translation quality notices", () => {
  it.each([
    ["zh-TW", "待校對", "依日文原文翻譯"],
    ["ja", "翻訳要校正", "日本語原文からの訳"],
    ["en", "Translation awaiting proofreading", "Translated from Japanese source"],
  ] as const)(
    "keeps the original without translation notices in %s",
    async (language, quality, source) => {
      const value = cell(false)
      const translated = {
        ...value,
        name: {
          primary: { lang: "zh-Hant" as const, text: "Synthetic translation" },
          secondary: { lang: "ja" as const, text: "Synthetic original" },
          missingTranslation: false,
          lowConfidence: true,
          jpSource: true,
        },
      }
      await renderInRouter(<CardGrid cells={[translated]} images={undefined} onOpen={vi.fn()} />, {
        language,
      })
      expect(screen.queryByText(quality)).not.toBeInTheDocument()
      expect(screen.queryByText(source)).not.toBeInTheDocument()
      expect(screen.getByText("Synthetic original")).toBeVisible()
      expect(
        screen.getByText("Synthetic translation", { selector: "span.font-medium" }),
      ).toBeVisible()
    },
  )
  it("does not label a translation with false quality flags", async () => {
    const value = cell(false)
    await renderInRouter(
      <CardGrid
        cells={[{ ...value, name: { ...value.name, lowConfidence: false, jpSource: false } }]}
        images={undefined}
        onOpen={vi.fn()}
      />,
    )
    expect(screen.queryByText("待校對")).not.toBeInTheDocument()
    expect(screen.queryByText("依日文原文翻譯")).not.toBeInTheDocument()
  })
})

it("keeps all results searchable and preserves shared cards when only part of a query changes", async () => {
  const values = Array.from({ length: 60 }, (_, index) => ({
    ...cell(false),
    key: `synthetic-${String(index)}`,
    to: `/cards/synthetic-${String(index)}`,
  }))
  const onOpen = vi.fn()
  const changed = values.map((value, index) =>
    index === 30 ? { ...value, key: "replacement", to: "/cards/replacement" } : value,
  )
  function Results() {
    const [items, setItems] = useState(values)
    return (
      <>
        <button
          onClick={() => {
            setItems(changed)
          }}
        >
          Replace
        </button>
        <CardGrid cells={items} images={undefined} onOpen={onOpen} />
      </>
    )
  }
  await renderInRouter(<Results />)
  const links = screen.getAllByRole("link")
  expect(links).toHaveLength(60)
  act(() => {
    screen.getByRole("button", { name: "Replace" }).click()
  })
  const updated = screen.getAllByRole("link")
  expect(updated.map((link) => link.getAttribute("href"))).toEqual(changed.map((value) => value.to))
  expect(updated[0]).toBe(links[0])
  expect(updated[59]).toBe(links[59])
})

it("updates shared labels and image states when the UI language changes without replacing the card", async () => {
  const value = { ...cell(true), name: { ...cell(true).name, missingTranslation: true } }
  const { i18n } = await renderRoutes([
    { path: "*", element: <CardGrid cells={[value]} images={undefined} onOpen={vi.fn()} /> },
  ])
  const link = screen.getByRole("link")
  expect(screen.queryByText("尚無譯名")).not.toBeInTheDocument()
  await act(async () => {
    await i18n.changeLanguage("en")
  })
  expect(screen.getByText("Wording pending")).toBeVisible()
  expect(screen.queryByText("卡文未定")).not.toBeInTheDocument()
  expect(screen.queryByText("尚無譯名")).not.toBeInTheDocument()
  expect(screen.queryByText("卡圖資訊載入中")).not.toBeInTheDocument()
  expect(screen.getByRole("link")).toBe(link)
})
