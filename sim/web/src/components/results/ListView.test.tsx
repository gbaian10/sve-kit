import { screen } from "@testing-library/react"
import { describe, expect, it, vi } from "vitest"

import { renderInRouter } from "../../test-utils"
import type { GridCell } from "./CardGrid"
import { ListView } from "./ListView"

const cell = (cost: number | null, key: string): GridCell => ({
  key,
  to: `/cards/${key}`,
  state: undefined,
  name: { primary: { lang: "ja", text: `試作 ${key}` }, missingTranslation: false },
  summary: {
    cardId: `c:${key}`,
    printingId: `p:${key}`,
    faceId: `f:${key}`,
    cardNo: key.toUpperCase(),
    region: "jp",
    classCode: "elf",
    name: { original: { lang: "ja", text: `試作 ${key}` }, translations: {} },
    cost,
    attack: 2,
    defense: 2,
  },
})

describe("ListView", () => {
  it("draws the official cost icon when there is one, else the number in a bubble", async () => {
    await renderInRouter(
      <ListView cells={[cell(3, "a"), cell(12, "b"), cell(null, "c")]} onOpen={vi.fn()} />,
    )
    expect(screen.getByRole("img", { name: "3" })).toBeInTheDocument()
    expect(screen.queryByRole("img", { name: "12" })).not.toBeInTheDocument()
    expect(screen.getByText("12")).toBeInTheDocument()
    expect(screen.getAllByRole("link")).toHaveLength(3)
  })
})
