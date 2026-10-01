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
    ["zh-TW", "表記未定"],
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
    expect(screen.queryByText("表記未定")).not.toBeInTheDocument()
  })
})
