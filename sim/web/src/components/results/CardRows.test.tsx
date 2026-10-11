import { act, screen } from "@testing-library/react"
import { afterEach, beforeEach, describe, expect, it, vi } from "vitest"

import { renderInRouter, renderRoutes } from "../../test-utils"
import type { GridCell } from "./CardGrid"
import { CardList, CardTable } from "./CardRows"

beforeEach(() => {
  vi.stubGlobal(
    "IntersectionObserver",
    class {
      observe() {}
      unobserve() {}
      disconnect() {}
    },
  )
})
afterEach(() => {
  vi.unstubAllGlobals()
})

const cell: GridCell = {
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
    cost: 15,
    attack: 1,
    defense: 1,
  },
}

describe("row costs", () => {
  it("names the cost with its number in the list's crystal and the table's stat line", async () => {
    const props = { cells: [cell], onOpen: vi.fn() }
    const { unmount } = await renderInRouter(<CardList {...props} />)
    expect(screen.getByText("15")).toBeVisible()
    expect(screen.getByRole("img", { name: "費用 15" })).toBeInTheDocument()
    expect(screen.getByRole("link")).toHaveAttribute("href", "/cards/BP-SYNTHETIC")
    unmount()
    await renderInRouter(<CardTable {...props} images={undefined} artOf={() => undefined} />)
    expect(screen.getByText("費用")).toBeVisible()
    expect(screen.getByText("15")).toBeVisible()
    expect(screen.getByText("攻擊力")).toBeVisible()
    expect(screen.getByRole("link")).toHaveAttribute("href", "/cards/BP-SYNTHETIC")
  })
})

describe("table thumbnails", () => {
  const art = { src: "/cdn/images/art_m/1.webp", srcSet: "", width: 384, height: 288 }
  it("shows the 4:3 illustration crop and falls back to the card thumbnail when it fails", async () => {
    await renderInRouter(
      <CardTable cells={[cell]} onOpen={vi.fn()} images={undefined} artOf={() => art} />,
    )
    const image = document.querySelector("[data-art-thumb] img")
    expect(image).toHaveAttribute("src", art.src)
    expect(image).toHaveAttribute("loading", "lazy")
    act(() => {
      image?.dispatchEvent(new Event("error"))
    })
    expect(document.querySelector("[data-art-thumb]")).toBeNull()
    expect(screen.getAllByText("卡圖資訊載入中").length).toBeGreaterThan(0)
  })
  it("uses the card thumbnail when no crop exists", async () => {
    await renderInRouter(
      <CardTable cells={[cell]} onOpen={vi.fn()} images={undefined} artOf={() => undefined} />,
    )
    expect(document.querySelector("[data-art-thumb]")).toBeNull()
    expect(document.querySelector("[data-image-state]")).toHaveAttribute(
      "data-image-state",
      "loading",
    )
  })
})

it("updates table labels and fallback thumbnails on language changes while retaining every row", async () => {
  const cells = Array.from({ length: 60 }, (_, index) => ({
    ...cell,
    key: `synthetic-${String(index)}`,
    to: `/cards/synthetic-${String(index)}`,
  }))
  const { i18n } = await renderRoutes([
    {
      path: "*",
      element: (
        <CardTable cells={cells} images={undefined} artOf={() => undefined} onOpen={vi.fn()} />
      ),
    },
  ])
  const links = screen.getAllByRole("link")
  expect(links).toHaveLength(60)
  expect(screen.getAllByText("費用")).toHaveLength(60)
  await act(async () => {
    await i18n.changeLanguage("en")
  })
  expect(screen.getAllByText("Cost")).toHaveLength(60)
  expect(screen.queryByText("費用")).not.toBeInTheDocument()
  expect(screen.queryByText("卡圖資訊載入中")).not.toBeInTheDocument()
  expect(screen.queryByText("效果載入中")).not.toBeInTheDocument()
  const updated = screen.getAllByRole("link")
  for (const [index, link] of links.entries()) expect(updated[index]).toBe(link)
})
