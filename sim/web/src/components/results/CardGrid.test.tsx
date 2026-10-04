import { screen } from "@testing-library/react"
import { describe, expect, it, vi } from "vitest"

import { renderInRouter } from "../../test-utils"
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

describe("visible current translation quality", () => {
  it.each([
    ["zh-TW", "待校對", "日英文字尚未核對"],
    ["ja", "翻訳要校正", "日英の原文は未照合"],
    ["en", "Translation awaiting proofreading", "Japanese and English wording unchecked"],
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
          sourceUnchecked: true,
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
        cells={[
          { ...value, name: { ...value.name, lowConfidence: false, sourceUnchecked: false } },
        ]}
        images={undefined}
        onOpen={vi.fn()}
      />,
    )
    expect(screen.queryByText("待校對")).not.toBeInTheDocument()
    expect(screen.queryByText("日英文字尚未核對")).not.toBeInTheDocument()
  })
})
