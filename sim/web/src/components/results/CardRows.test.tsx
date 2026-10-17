import { act, screen, within } from "@testing-library/react"
import userEvent from "@testing-library/user-event"
import { afterEach, beforeEach, describe, expect, it, vi } from "vitest"

import { useAbilityPreview } from "../../app/snapshot"
import { renderInRouter, renderRoutes } from "../../test-utils"
import type { GridCell } from "./CardGrid"
import { CardList, CardTable } from "./CardRows"

vi.mock("../../app/snapshot", async (importOriginal) => ({
  ...(await importOriginal<typeof import("../../app/snapshot")>()),
  useAbilityPreview: vi.fn(() => ({ phase: "pending" })),
}))

beforeEach(() => {
  vi.mocked(useAbilityPreview).mockReset().mockReturnValue({ phase: "pending" })
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
  it("shows a plain numeric list cost and the table stat line", async () => {
    const props = { cells: [cell], onOpen: vi.fn() }
    const { unmount } = await renderInRouter(<CardList {...props} />)
    expect(screen.getByText("15")).toBeVisible()
    expect(screen.queryByRole("img")).not.toBeInTheDocument()
    expect(screen.getByRole("link")).toHaveAttribute("href", "/cards/BP-SYNTHETIC")
    unmount()
    await renderInRouter(<CardTable {...props} images={undefined} artOf={() => undefined} />)
    expect(screen.getByText("消費")).toBeVisible()
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
  expect(screen.getAllByText("消費")).toHaveLength(60)
  await act(async () => {
    await i18n.changeLanguage("en")
  })
  expect(screen.getAllByText("Cost")).toHaveLength(60)
  expect(screen.queryByText("消費")).not.toBeInTheDocument()
  expect(screen.queryByText("卡圖資訊載入中")).not.toBeInTheDocument()
  expect(screen.queryByText("效果載入中")).not.toBeInTheDocument()
  const updated = screen.getAllByRole("link")
  for (const [index, link] of links.entries()) expect(updated[index]).toBe(link)
})

describe("spreadsheet list", () => {
  it("renders six columns, explicit states, full one-line text and absent stats without card images", async () => {
    vi.stubGlobal("IntersectionObserver", undefined)
    const states = ["ready", "pending", "error", "unknown"]
    const cells = states.map((state) => ({
      ...cell,
      key: state,
      summary: {
        ...cell.summary,
        printingId: state,
        attack: null,
        defense: null,
        wordingPending: state === "unknown",
      },
    }))
    vi.mocked(useAbilityPreview).mockImplementation((id) =>
      id === "ready"
        ? {
            phase: "complete",
            queryId: 1,
            generation: 1,
            root: "/cdn",
            manifestHash: "synthetic",
            page: { total: 0, offset: 0, items: [] },
            preview: {
              text: "First synthetic ability.\nSecond synthetic ability.",
              language: "en",
            },
          }
        : id === "error"
          ? { phase: "error", error: { kind: "query-failed", message: "synthetic failure" } }
          : { phase: "pending" },
    )
    await renderInRouter(<CardList cells={cells} onOpen={vi.fn()} />)
    expect(screen.getAllByRole("columnheader").map((column) => column.textContent)).toEqual([
      "卡號 ↑",
      "名稱",
      "消費",
      "攻擊力",
      "生命值",
      "卡文",
    ])
    const rows = screen.getAllByRole("row").slice(1)
    expect(rows[0]).toHaveTextContent("First synthetic ability. Second synthetic ability.")
    expect(within(rows[0] ?? document.body).getAllByText("—")).toHaveLength(2)
    expect(rows[1]).toHaveTextContent("效果載入中")
    expect(rows[2]).toHaveTextContent("效果無法載入")
    expect(rows[3]).toHaveTextContent("卡文未定")
    expect(vi.mocked(useAbilityPreview).mock.calls.some(([id]) => id === "unknown")).toBe(false)
    expect(document.querySelector("[data-image-printing], [data-image-state]")).toBeNull()
    expect(screen.queryByRole("img")).not.toBeInTheDocument()
  })

  it("waits until rows enter the viewport before requesting ability previews", async () => {
    await renderInRouter(<CardList cells={[cell]} onOpen={vi.fn()} />)
    expect(useAbilityPreview).not.toHaveBeenCalled()
    expect(document.querySelector('[data-preview="waiting"]')).not.toBeNull()
  })

  it("moves between rows with arrow keys and opens the selected card with Enter", async () => {
    const onOpen = vi.fn()
    const next: GridCell = {
      ...cell,
      key: "next",
      to: "/cards/next",
      name: { ...cell.name, primary: { lang: "en", text: "Next synthetic" } },
    }
    await renderInRouter(<CardList cells={[cell, next]} onOpen={onOpen} />)
    const user = userEvent.setup()
    const links = screen.getAllByRole("link")
    links[0]?.focus()
    await user.keyboard("{ArrowDown}")
    expect(links[1]).toHaveFocus()
    await user.keyboard("{ArrowUp}")
    expect(links[0]).toHaveFocus()
    await user.keyboard("{ArrowDown}{Enter}")
    expect(onOpen).toHaveBeenCalledWith(next, expect.anything())
  })
})
