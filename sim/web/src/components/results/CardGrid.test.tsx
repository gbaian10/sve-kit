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

it("offers each grid card only its largest available image candidate", async () => {
  const largestFiles: ImageIndex = {
    asset: () => ({ availability: "available", publication_state: "approved" }),
    cardImage: (printingId) => {
      const file =
        printingId === "p:small"
          ? { size: "card_m", width: 320, height: 447 }
          : { size: "card_l", width: 459, height: 641 }
      const path = `/cdn/images/${file.size}/synthetic.webp?v=1`
      return {
        src: path,
        srcSet: `/cdn/images/card_s/synthetic.webp?v=1 128w, /cdn/images/card_m/synthetic.webp?v=1 320w, /cdn/images/card_l/synthetic.webp?v=1 459w`,
        width: file.width,
        height: file.height,
      }
    },
  }
  const small = cell(false)
  const large = cell(false)
  await renderInRouter(
    <CardGrid
      cells={[
        { ...large, key: "large", summary: { ...large.summary, printingId: "p:large" } },
        { ...small, key: "small", summary: { ...small.summary, printingId: "p:small" } },
      ]}
      images={largestFiles}
      onOpen={vi.fn()}
    />,
  )

  const [largeImage, smallImage] = document.querySelectorAll("img[srcset]")
  expect(largeImage?.getAttribute("srcset")).toBe("/cdn/images/card_l/synthetic.webp?v=1 459w")
  expect(smallImage?.getAttribute("srcset")).toBe("/cdn/images/card_m/synthetic.webp?v=1 320w")
})

describe("visible current translation quality", () => {
  it.each([
    ["zh-TW", "待校對", "依日文原文翻譯"],
    ["ja", "翻訳要校正", "日本語原文からの訳"],
    ["en", "Translation awaiting proofreading", "Translated from Japanese source"],
  ] as const)(
    "shows independent notices and the original in %s",
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
      expect(screen.getByText(quality)).toBeVisible()
      expect(screen.getByText(source)).toBeVisible()
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
  expect(screen.getByText("尚無譯名")).toBeVisible()
  await act(async () => {
    await i18n.changeLanguage("en")
  })
  expect(screen.getByText("Wording pending")).toBeVisible()
  expect(screen.queryByText("卡文未定")).not.toBeInTheDocument()
  expect(screen.queryByText("尚無譯名")).not.toBeInTheDocument()
  expect(screen.queryByText("卡圖資訊載入中")).not.toBeInTheDocument()
  expect(screen.getByRole("link")).toBe(link)
})
