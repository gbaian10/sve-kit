import { act, screen, waitFor, within } from "@testing-library/react"
import userEvent from "@testing-library/user-event"
import { afterEach, beforeEach, describe, expect, it, vi } from "vitest"

import { useAbilityPreview } from "../../app/snapshot"
import { displayName } from "../../domain/nameDisplay"
import { DEFAULT_PREFS, prefsStore, usePrefs } from "../../settings"
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
  act(() => {
    prefsStore.set(DEFAULT_PREFS)
  })
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
    const states = ["ready", "pending", "error", "unknown", "blank"]
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
      id === "ready" || id === "blank"
        ? {
            phase: "complete",
            queryId: 1,
            generation: 1,
            root: "/cdn",
            manifestHash: "synthetic",
            page: { total: 0, offset: 0, items: [] },
            preview:
              id === "blank"
                ? null
                : {
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
    expect(rows[4]?.querySelector("[data-list-effect-text]")).toHaveTextContent(/^$/u)
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

it.each(["list", "table"] as const)(
  "updates %s names and ability text immediately with the display settings",
  async (view) => {
    vi.stubGlobal("IntersectionObserver", undefined)
    const source = {
      original: { lang: "ja" as const, text: "Original synthetic name" },
      translations: { "zh-Hant": "Translated synthetic name" },
    }
    vi.mocked(useAbilityPreview).mockImplementation((_printing, _face, language) => ({
      phase: "complete",
      queryId: 1,
      generation: 1,
      root: "/cdn",
      manifestHash: "synthetic",
      page: { total: 0, offset: 0, items: [] },
      preview: {
        text: language === "ja" ? "Original synthetic effect" : "Translated synthetic effect",
        language,
      },
    }))
    function Example() {
      const prefs = usePrefs()
      const named = { ...cell, name: displayName(source, "zh-TW", prefs.nameDisplay) }
      return view === "list" ? (
        <CardList cells={[named]} onOpen={vi.fn()} />
      ) : (
        <CardTable cells={[named]} onOpen={vi.fn()} images={undefined} artOf={() => undefined} />
      )
    }
    prefsStore.set({ effectLanguage: "both", nameDisplay: "both" })
    await renderInRouter(<Example />)
    const name = () => document.querySelector("[data-row-name]")
    const effect = () => document.querySelector('[data-preview="ready"]')
    expect(name()).toHaveTextContent("Translated synthetic name")
    expect(name()).toHaveTextContent("Original synthetic name")
    expect(effect()).toHaveTextContent("Translated synthetic effect")
    expect(effect()).toHaveTextContent("Original synthetic effect")
    act(() => {
      prefsStore.set({ effectLanguage: "original", nameDisplay: "original" })
    })
    expect(name()).toHaveTextContent("Original synthetic name")
    expect(name()).not.toHaveTextContent("Translated synthetic name")
    expect(effect()).toHaveTextContent("Original synthetic effect")
    expect(effect()).not.toHaveTextContent("Translated synthetic effect")
    act(() => {
      prefsStore.set({ effectLanguage: "translated", nameDisplay: "translated" })
    })
    expect(name()).toHaveTextContent("Translated synthetic name")
    expect(name()).not.toHaveTextContent("Original synthetic name")
    expect(effect()).toHaveTextContent("Translated synthetic effect")
    expect(effect()).not.toHaveTextContent("Original synthetic effect")
  },
)

it("keeps a large loaded list bounded and replaces its visible rows when the page scrolls", async () => {
  const cells = Array.from({ length: 600 }, (_, index) => ({
    ...cell,
    key: `window-${String(index)}`,
    to: `/cards/window-${String(index)}`,
  }))
  await renderInRouter(<CardList cells={cells} onOpen={vi.fn()} />)
  const table = screen.getByRole("table")
  expect(table).toHaveAttribute("data-loaded-count", "600")
  expect(table).toHaveAttribute("aria-rowcount", "601")
  expect(screen.getAllByRole("link").length).toBeLessThan(100)
  const body = table.querySelector("tbody")
  if (!body) throw new Error("missing result body")
  vi.spyOn(body, "getBoundingClientRect").mockReturnValue({
    top: -8000,
    bottom: 16000,
    left: 0,
    right: 1000,
    width: 1000,
    height: 24000,
    x: 0,
    y: -8000,
    toJSON: () => ({}),
  })
  act(() => {
    window.dispatchEvent(new Event("scroll"))
  })
  await waitFor(() => {
    expect(document.querySelector('a[href="/cards/window-200"]')).not.toBeNull()
  })
  expect(document.querySelector('a[href="/cards/window-0"]')).toBeNull()
  expect(screen.getAllByRole("link").length).toBeLessThan(100)
  expect(document.querySelector('a[href="/cards/window-200"]')?.closest("tr")).toHaveAttribute(
    "aria-rowindex",
    "202",
  )
})
