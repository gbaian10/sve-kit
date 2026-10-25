import { act, fireEvent, screen, waitFor, within } from "@testing-library/react"
import userEvent from "@testing-library/user-event"
import { useTranslation } from "react-i18next"
import { afterEach, beforeEach, describe, expect, it, vi } from "vitest"

import { useAbilityPreview } from "../../app/snapshot"
import { displayName } from "../../domain/nameDisplay"
import { DEFAULT_PREFS, prefsStore, usePrefs } from "../../settings"
import { renderInRouter, renderRoutes } from "../../test-utils"
import { CardGrid, type GridCell } from "./CardGrid"
import { CardList, CardTable } from "./CardRows"
import { ListColumnPicker } from "./ListColumnPicker"

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

const classLabel = () => "中立"
const typeNames: Readonly<Record<string, string>> = {
  follower: "從者",
  spell: "法術",
  amulet: "護符",
  crest: "紋章",
  equipment: "裝備",
  ep: "Evolution Point",
  sep: "Super-Evolution Point",
}
const typeLabel = (code: string) => typeNames[code] ?? code

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
    const { unmount } = await renderInRouter(
      <CardList typeLabel={typeLabel} classLabel={classLabel} {...props} />,
    )
    expect(screen.getByText("15")).toBeVisible()
    expect(document.querySelector("[data-image-printing], [data-image-state]")).toBeNull()
    expect(screen.getByRole("link")).toHaveAttribute("href", "/cards/BP-SYNTHETIC")
    unmount()
    await renderInRouter(
      <CardTable typeLabel={typeLabel} {...props} images={undefined} artOf={() => undefined} />,
    )
    expect(screen.getByText("消費")).toBeVisible()
    expect(screen.getByText("15")).toBeVisible()
    expect(screen.getByText("攻擊力")).toBeVisible()
    expect(screen.getByRole("link")).toHaveAttribute("href", "/cards/BP-SYNTHETIC")
  })
})

describe("table thumbnails", () => {
  const art = { src: "/cdn/images/art_s/1.webp", width: 160, height: 120 }
  it("shows the 4:3 illustration crop and falls back to the card thumbnail when it fails", async () => {
    await renderInRouter(
      <CardTable
        typeLabel={typeLabel}
        cells={[cell]}
        onOpen={vi.fn()}
        images={undefined}
        artOf={() => art}
      />,
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
      <CardTable
        typeLabel={typeLabel}
        cells={[cell]}
        onOpen={vi.fn()}
        images={undefined}
        artOf={() => undefined}
      />,
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
        <CardTable
          typeLabel={typeLabel}
          cells={cells}
          images={undefined}
          artOf={() => undefined}
          onOpen={vi.fn()}
        />
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
  it("renders ten columns, explicit states, full one-line text and absent stats without card images", async () => {
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
    await renderInRouter(
      <CardList typeLabel={typeLabel} classLabel={classLabel} cells={cells} onOpen={vi.fn()} />,
    )
    expect(
      screen.getAllByRole("columnheader").map((column) => column.getAttribute("aria-label")),
    ).toEqual([
      "卡號",
      "名稱",
      "稀有度",
      "位置",
      "種類",
      "職業",
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
    expect(vi.mocked(useAbilityPreview).mock.calls.some(([id]) => id === "unknown")).toBe(true)
    expect(document.querySelector("[data-image-printing], [data-image-state]")).toBeNull()
  })

  it("waits until rows enter the viewport before requesting ability previews", async () => {
    await renderInRouter(
      <CardList typeLabel={typeLabel} classLabel={classLabel} cells={[cell]} onOpen={vi.fn()} />,
    )
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
    await renderInRouter(
      <CardList
        typeLabel={typeLabel}
        classLabel={classLabel}
        cells={[cell, next]}
        onOpen={onOpen}
      />,
    )
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
        <CardList typeLabel={typeLabel} classLabel={classLabel} cells={[named]} onOpen={vi.fn()} />
      ) : (
        <CardTable
          typeLabel={typeLabel}
          cells={[named]}
          onOpen={vi.fn()}
          images={undefined}
          artOf={() => undefined}
        />
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
  await renderInRouter(
    <CardList typeLabel={typeLabel} classLabel={classLabel} cells={cells} onOpen={vi.fn()} />,
  )
  const table = screen.getByRole("table")
  expect(table).toHaveAttribute("data-loaded-count", "600")
  expect(table).toHaveAttribute("aria-rowcount", "601")
  expect(table.querySelector("thead tr")).toHaveAttribute("aria-rowindex", "1")
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

it("preserves a text selection instead of opening a row without translation notices", async () => {
  const onOpen = vi.fn()
  await renderInRouter(
    <CardList
      typeLabel={typeLabel}
      classLabel={classLabel}
      cells={[{ ...cell, name: { ...cell.name, missingTranslation: true } }]}
      onOpen={onOpen}
    />,
  )
  expect(screen.queryByRole("img", { name: "尚無譯名" })).not.toBeInTheDocument()
  const selection = window.getSelection()
  const name = screen.getByRole("link")
  const range = document.createRange()
  range.selectNodeContents(name)
  selection?.removeAllRanges()
  selection?.addRange(range)
  fireEvent.click(screen.getByText("BP-SYNTHETIC"), { detail: 1 })
  expect(onOpen).not.toHaveBeenCalled()
  selection?.removeAllRanges()
})

it.each(["list", "table"] as const)(
  "shows a printing badge in %s without loading a card image for the list",
  async (view) => {
    const premium = { ...cell, summary: { ...cell.summary, rarityCode: "sr", premium: true } }
    await renderInRouter(
      view === "list" ? (
        <CardList
          typeLabel={typeLabel}
          classLabel={classLabel}
          cells={[premium]}
          onOpen={vi.fn()}
        />
      ) : (
        <CardTable
          typeLabel={typeLabel}
          cells={[premium]}
          onOpen={vi.fn()}
          images={undefined}
          artOf={() => undefined}
        />
      ),
    )
    expect(screen.getByRole("img", { name: "官方稀有度：SR" })).toBeVisible()
    expect(document.querySelectorAll("[data-rarity-badges] img")).toHaveLength(1)
    if (view === "list") {
      const headers = screen.getAllByRole("columnheader")
      expect(headers[2]).toHaveTextContent("稀有度")
      expect(headers[2]?.querySelector("button:not([data-column-resize])")).toBeNull()
      expect(document.querySelector("[data-list-rarity] img")).not.toBeNull()
      expect(document.querySelector("[data-image-state], [data-image-printing]")).toBeNull()
    }
  },
)

it("hides matching headers and cells, keeps an identity column, and preserves row navigation", async () => {
  await renderInRouter(
    <>
      <ListColumnPicker />
      <CardList typeLabel={typeLabel} classLabel={classLabel} cells={[cell]} onOpen={vi.fn()} />
    </>,
  )
  const user = userEvent.setup()
  await user.click(screen.getByText("顯示欄位", { selector: "summary" }))
  for (const name of ["稀有度", "位置", "種類", "職業", "消費", "攻擊力", "生命值"]) {
    await user.click(screen.getByRole("checkbox", { name }))
  }
  expect(screen.getAllByRole("columnheader")).toHaveLength(3)
  expect(screen.getAllByRole("cell")).toHaveLength(3)
  expect(screen.getByRole("columnheader", { name: "卡文" })).toBeVisible()
  await user.click(screen.getByRole("checkbox", { name: "名稱" }))
  expect(screen.getByRole("checkbox", { name: "卡號" })).toBeDisabled()
  expect(screen.getByRole("link", { name: "BP-SYNTHETIC" })).toHaveAttribute("href", cell.to)
  expect(screen.getByRole("table")).toHaveAttribute("data-frozen-column", "number")
  await user.click(screen.getByRole("checkbox", { name: "名稱" }))
  await user.click(screen.getByRole("checkbox", { name: "卡號" }))
  expect(screen.getByRole("checkbox", { name: "名稱" })).toBeDisabled()
  expect(screen.getByRole("table")).toHaveAttribute("data-frozen-column", "name")
})

it("resizes a column by keyboard, persists it on reload, and restores the default", async () => {
  const { unmount } = await renderInRouter(
    <CardList typeLabel={typeLabel} classLabel={classLabel} cells={[cell]} onOpen={vi.fn()} />,
  )
  const handle = screen.getByRole("separator", { name: "調整「名稱」欄寬" })
  const heading = handle.parentElement
  if (!heading) throw new Error("missing heading")
  vi.spyOn(heading, "getBoundingClientRect").mockReturnValue({ width: 192 } as DOMRect)
  handle.focus()
  await userEvent.keyboard("{ArrowRight}")
  expect(prefsStore.get().listWidths.name).toBe(202)
  unmount()
  prefsStore.reload()
  await renderInRouter(
    <CardList typeLabel={typeLabel} classLabel={classLabel} cells={[cell]} onOpen={vi.fn()} />,
  )
  expect(screen.getByRole("table").style.getPropertyValue("--list-user-name")).toBe("202px")
  act(() => {
    prefsStore.set(DEFAULT_PREFS)
  })
  expect(screen.getByRole("table").style.getPropertyValue("--list-user-name")).toBe("")
})

it("previews a drag locally and persists once when released", async () => {
  await renderInRouter(
    <CardList typeLabel={typeLabel} classLabel={classLabel} cells={[cell]} onOpen={vi.fn()} />,
  )
  const handle = screen.getByRole("separator", { name: "調整「名稱」欄寬" })
  const heading = handle.parentElement
  if (!heading) throw new Error("missing heading")
  vi.spyOn(heading, "getBoundingClientRect").mockReturnValue({ width: 192 } as DOMRect)
  Object.defineProperty(handle, "setPointerCapture", { value: vi.fn() })
  Object.defineProperty(handle, "releasePointerCapture", { value: vi.fn() })
  const writes = vi.spyOn(Storage.prototype, "setItem")
  const notified = vi.fn()
  const unsubscribe = prefsStore.subscribe(notified)
  try {
    fireEvent.pointerDown(handle, { pointerId: 1, button: 0, clientX: 100 })
    for (let move = 1; move <= 100; move += 1) {
      fireEvent.pointerMove(handle, { pointerId: 1, clientX: 100 + move })
    }
    expect(screen.getByRole("table").style.getPropertyValue("--list-user-name")).toBe("292px")
    expect(writes).not.toHaveBeenCalled()
    expect(notified).not.toHaveBeenCalled()
    fireEvent.pointerUp(handle, { pointerId: 1, clientX: 200 })
    expect(writes).toHaveBeenCalledTimes(1)
    expect(notified).toHaveBeenCalledTimes(1)
    expect(prefsStore.get().listWidths.name).toBe(292)
  } finally {
    unsubscribe()
    writes.mockRestore()
  }
})

it("keeps touch resize drafts separate from desktop widths and restores both defaults", async () => {
  vi.stubGlobal("matchMedia", () => ({ matches: true }))
  act(() => {
    prefsStore.set({ listWidths: { name: 280 } })
  })
  const { unmount } = await renderInRouter(
    <CardList typeLabel={typeLabel} classLabel={classLabel} cells={[cell]} onOpen={vi.fn()} />,
  )
  const handle = screen.getByRole("separator", { name: "調整「名稱」欄寬" })
  const heading = handle.parentElement
  if (!heading) throw new Error("missing heading")
  vi.spyOn(heading, "getBoundingClientRect").mockReturnValue({ width: 124 } as DOMRect)
  Object.defineProperty(handle, "setPointerCapture", { value: vi.fn() })
  Object.defineProperty(handle, "releasePointerCapture", { value: vi.fn() })
  const writes = vi.spyOn(Storage.prototype, "setItem")
  try {
    fireEvent.pointerDown(handle, { pointerId: 1, pointerType: "touch", button: 0, clientX: 100 })
    for (let x = 110; x <= 180; x += 10) {
      fireEvent.pointerMove(handle, { pointerId: 1, pointerType: "touch", clientX: x })
    }
    expect(screen.getByRole("table").style.getPropertyValue("--list-mobile-user-name")).toBe(
      "204px",
    )
    expect(writes).not.toHaveBeenCalled()
    fireEvent.pointerUp(handle, { pointerId: 1, pointerType: "touch" })
    expect(writes).toHaveBeenCalledTimes(1)
    expect(prefsStore.get().listMobileWidths.name).toBe(204)
    expect(prefsStore.get().listWidths.name).toBe(280)
    expect(prefsStore.get().listOrder).toEqual(DEFAULT_PREFS.listOrder)
  } finally {
    writes.mockRestore()
  }
  unmount()
  prefsStore.reload()
  await renderInRouter(
    <CardList typeLabel={typeLabel} classLabel={classLabel} cells={[cell]} onOpen={vi.fn()} />,
  )
  expect(screen.getByRole("table").style.getPropertyValue("--list-mobile-user-name")).toBe("204px")
  act(() => {
    prefsStore.set(DEFAULT_PREFS)
  })
  expect(screen.getByRole("table").style.getPropertyValue("--list-mobile-user-name")).toBe("")
  expect(screen.getByRole("table").style.getPropertyValue("--list-user-name")).toBe("")
})

it("shows only class icons with localized accessible names including neutral, and hides their column", async () => {
  function Example() {
    const { t } = useTranslation()
    return (
      <>
        <ListColumnPicker />
        <CardList
          typeLabel={typeLabel}
          classLabel={() => t("search.neutral")}
          cells={[cell]}
          onOpen={vi.fn()}
        />
      </>
    )
  }
  const { i18n } = await renderRoutes([{ path: "*", element: <Example /> }])
  expect(screen.getByRole("img", { name: "中立" })).toHaveAttribute("title", "中立")
  expect(document.querySelector("[data-list-class]")).toHaveTextContent(/^$/u)
  expect(document.querySelector("[data-list-class] img")).toHaveAttribute(
    "src",
    expect.stringContaining("neutral"),
  )
  await act(async () => {
    await i18n.changeLanguage("ja")
  })
  expect(screen.getByRole("img", { name: "ニュートラル" })).toHaveAttribute("title", "ニュートラル")
  await act(async () => {
    await i18n.changeLanguage("en")
  })
  expect(screen.getByRole("img", { name: "Neutral" })).toHaveAttribute("title", "Neutral")
  expect(
    screen
      .getByRole("columnheader", { name: "Class" })
      .querySelector("button:not([data-column-resize])"),
  ).toBeNull()
  await userEvent.click(screen.getByText("Visible columns", { selector: "summary" }))
  await userEvent.click(screen.getByRole("checkbox", { name: "Class" }))
  expect(document.querySelector("[data-list-class]")).toBeNull()
  expect(screen.queryByRole("columnheader", { name: "Class" })).not.toBeInTheDocument()
})

it("reorders from the picker keyboard and keeps focus and cells in the same order", async () => {
  await renderInRouter(
    <>
      <ListColumnPicker />
      <CardList typeLabel={typeLabel} classLabel={classLabel} cells={[cell]} onOpen={vi.fn()} />
    </>,
  )
  await userEvent.click(screen.getByText("顯示欄位", { selector: "summary" }))
  const handle = screen.getByRole("button", { name: "調整「職業」順序（↑／↓ 移動）" })
  handle.focus()
  await userEvent.keyboard("{ArrowUp}")
  expect(handle).toHaveFocus()
  expect(
    screen.getAllByRole("columnheader").map((header) => header.getAttribute("aria-label")),
  ).toEqual(["卡號", "名稱", "稀有度", "位置", "職業", "種類", "消費", "攻擊力", "生命值", "卡文"])
  expect(
    within(screen.getAllByRole("cell")[4] ?? document.body).getByRole("img", { name: "中立" }),
  ).toBeVisible()
  expect(
    screen.getAllByRole("checkbox").map((checkbox) => checkbox.closest("label")?.textContent),
  ).toEqual(["卡號", "名稱", "稀有度", "位置", "職業", "種類", "消費", "攻擊力", "生命值", "卡文"])
  await userEvent.keyboard("{ArrowDown}")
  expect(prefsStore.get().listOrder).toEqual(DEFAULT_PREFS.listOrder)
})

it("keeps short header clicks sortable and commits a header drag only on release", async () => {
  const onSort = vi.fn()
  await renderInRouter(
    <CardList
      typeLabel={typeLabel}
      classLabel={classLabel}
      cells={[cell]}
      onOpen={vi.fn()}
      onSort={onSort}
    />,
  )
  vi.stubGlobal("matchMedia", () => ({ matches: true }))
  const headings = screen.getAllByRole("columnheader")
  headings.forEach((heading, index) => {
    vi.spyOn(heading, "getBoundingClientRect").mockReturnValue({
      left: index * 100,
      right: (index + 1) * 100,
    } as DOMRect)
  })
  const button = screen.getByRole("button", { name: "名稱" })
  Object.defineProperty(button, "setPointerCapture", { value: vi.fn() })
  Object.defineProperty(button, "releasePointerCapture", { value: vi.fn() })
  const writes = vi.spyOn(Storage.prototype, "setItem")
  try {
    fireEvent.pointerDown(button, { pointerId: 1, isPrimary: true, button: 0, clientX: 120 })
    fireEvent.pointerMove(button, { pointerId: 1, clientX: 122 })
    fireEvent.pointerUp(button, { pointerId: 1 })
    fireEvent.click(button, { detail: 1 })
    expect(onSort).toHaveBeenCalledWith("name")
    onSort.mockClear()
    fireEvent.pointerDown(button, { pointerId: 2, isPrimary: true, button: 0, clientX: 120 })
    for (let x = 130; x < 700; x += 10) fireEvent.pointerMove(button, { pointerId: 2, clientX: x })
    expect(writes).not.toHaveBeenCalled()
    expect(document.querySelector('[data-list-heading="cost"]')).toHaveAttribute(
      "data-drop",
      "after",
    )
    fireEvent.pointerUp(button, { pointerId: 2 })
    fireEvent.click(button, { detail: 1 })
    expect(onSort).not.toHaveBeenCalled()
    expect(writes).toHaveBeenCalledTimes(1)
    expect(prefsStore.get().listOrder).toEqual([
      "number",
      "rarity",
      "location",
      "type",
      "class",
      "cost",
      "name",
      "attack",
      "defense",
      "effect",
    ])
    expect(screen.getAllByRole("cell")[6]).toHaveTextContent("Synthetic name")
  } finally {
    writes.mockRestore()
    vi.unstubAllGlobals()
  }
})

it.each(["list", "table"] as const)("shows EVOLVE only for evolve cards in %s", async (view) => {
  const evolved = {
    ...cell,
    key: "evolved",
    summary: { ...cell.summary, deckLocations: ["evolve"] as const, rarityCode: "lg" },
  }
  const props = { cells: [cell, evolved], onOpen: vi.fn() }
  await renderInRouter(
    view === "list" ? (
      <CardList typeLabel={typeLabel} classLabel={classLabel} {...props} />
    ) : (
      <CardTable typeLabel={typeLabel} {...props} images={undefined} artOf={() => undefined} />
    ),
  )
  expect(screen.getAllByRole("img", { name: "進化" })).toHaveLength(1)
  if (view === "list") {
    const cells = document.querySelectorAll("[data-list-location]")
    expect(cells[0]).toBeEmptyDOMElement()
    expect(within(cells[1] as HTMLElement).getByRole("img", { name: "進化" })).toBeVisible()
    expect(screen.getAllByRole("columnheader")[3]).toHaveAttribute("aria-label", "位置")
  } else {
    const row = screen.getAllByRole("listitem")[1] as HTMLElement
    expect(
      within(row)
        .getAllByRole("img")
        .map((image) => image.getAttribute("alt")),
    ).toEqual([expect.stringContaining("LG"), "進化"])
  }
})

it.each(["list", "table"] as const)(
  "shows vocabulary labels for crest and equipment types in %s",
  async (view) => {
    const props = {
      cells: ["crest", "equipment", "leader"].map((typeCode) => ({
        ...cell,
        key: typeCode,
        summary: {
          ...cell.summary,
          typeCode,
          deckLocations: typeCode === "leader" ? (["leader"] as const) : (["token"] as const),
        },
      })),
      onOpen: vi.fn(),
    }
    await renderInRouter(
      view === "list" ? (
        <CardList typeLabel={typeLabel} classLabel={classLabel} {...props} />
      ) : (
        <CardTable typeLabel={typeLabel} {...props} images={undefined} artOf={() => undefined} />
      ),
    )
    expect(screen.getByText("紋章")).toBeVisible()
    expect(screen.getByText("裝備")).toBeVisible()
    expect(screen.queryByText("crest")).not.toBeInTheDocument()
    expect(screen.queryByText("equipment")).not.toBeInTheDocument()
    expect(screen.queryByText("leader")).not.toBeInTheDocument()
    expect(screen.getAllByRole("img", { name: "Token" })).toHaveLength(2)
  },
)

it("keeps EP and SEP text available with full-label tooltips in the list", async () => {
  await renderInRouter(
    <CardList
      typeLabel={typeLabel}
      classLabel={classLabel}
      cells={["ep", "sep"].map((typeCode) => ({
        ...cell,
        key: typeCode,
        summary: { ...cell.summary, typeCode, deckLocations: [] },
      }))}
      onOpen={vi.fn()}
    />,
    { language: "en" },
  )
  for (const name of ["Evolution Point", "Super-Evolution Point"]) {
    expect(screen.getByText(name)).toBeVisible()
    expect(screen.getByText(name)).toHaveAttribute("title", name)
  }
  for (const location of document.querySelectorAll("[data-list-location]")) {
    expect(location).toBeEmptyDOMElement()
  }
})

it.each(["list", "table"] as const)(
  "uses the supplied vocabulary label for the type icon in %s regardless of UI language",
  async (view) => {
    const props = {
      cells: [{ ...cell, summary: { ...cell.summary, typeCode: "follower" } }],
      onOpen: vi.fn(),
      typeLabel: () => "詞彙從者",
    }
    await renderInRouter(
      view === "list" ? (
        <CardList classLabel={classLabel} {...props} />
      ) : (
        <CardTable {...props} images={undefined} artOf={() => undefined} />
      ),
      { language: "en" },
    )
    expect(screen.getByRole("img", { name: "詞彙從者" })).toHaveAttribute("title", "詞彙從者")
    expect(screen.queryByRole("img", { name: "Follower" })).not.toBeInTheDocument()
  },
)

it("toggles type and location columns independently of rarity", async () => {
  await renderInRouter(
    <>
      <ListColumnPicker />
      <CardList
        typeLabel={typeLabel}
        classLabel={classLabel}
        cells={[
          {
            ...cell,
            summary: { ...cell.summary, typeCode: "follower", deckLocations: ["evolve"] as const },
          },
        ]}
        onOpen={vi.fn()}
      />
    </>,
  )
  await userEvent.click(screen.getByText("顯示欄位", { selector: "summary" }))
  await userEvent.click(screen.getByRole("checkbox", { name: "位置" }))
  expect(screen.queryByRole("columnheader", { name: "位置" })).not.toBeInTheDocument()
  expect(screen.queryByRole("img", { name: "進化" })).not.toBeInTheDocument()
  expect(screen.getByRole("columnheader", { name: "稀有度" })).toBeVisible()
  expect(screen.getByRole("img", { name: "從者" })).toBeVisible()
  await userEvent.click(screen.getByRole("checkbox", { name: "種類" }))
  expect(screen.queryByRole("columnheader", { name: "種類" })).not.toBeInTheDocument()
  expect(screen.queryByRole("img", { name: "從者" })).not.toBeInTheDocument()
  await userEvent.click(screen.getByRole("checkbox", { name: "位置" }))
  expect(screen.getByRole("img", { name: "進化" })).toBeVisible()
})

it.each(["zh-TW", "ja", "en"] as const)(
  "shows only effect text without an incomplete-preview notice in %s",
  async (language) => {
    act(() => {
      prefsStore.set({ effectLanguage: "original" })
    })
    vi.stubGlobal("IntersectionObserver", undefined)
    vi.mocked(useAbilityPreview).mockReturnValue({
      phase: "complete",
      queryId: 1,
      generation: 1,
      root: "/cdn",
      manifestHash: "synthetic",
      page: { total: 0, offset: 0, items: [] },
      preview: { text: "Synthetic ability", language: "en" },
    })
    await renderRoutes(
      [
        {
          path: "*",
          element: (
            <CardTable
              typeLabel={typeLabel}
              cells={[cell]}
              onOpen={vi.fn()}
              images={undefined}
              artOf={() => undefined}
            />
          ),
        },
      ],
      { language },
    )
    expect(screen.getByText("Synthetic ability").closest("[data-preview]")).toHaveTextContent(
      /^Synthetic ability$/u,
    )
  },
)

it.each(["list", "table"] as const)(
  "shows all positions and the type badge in %s",
  async (view) => {
    const classified = {
      ...cell,
      summary: {
        ...cell.summary,
        typeCode: "follower",
        deckLocations: ["evolve", "token"] as const,
      },
    }
    const props = { cells: [classified], onOpen: vi.fn() }
    await renderInRouter(
      view === "list" ? (
        <CardList typeLabel={typeLabel} classLabel={classLabel} {...props} />
      ) : (
        <CardTable typeLabel={typeLabel} {...props} images={undefined} artOf={() => undefined} />
      ),
    )
    const positions = document.querySelector("[data-location-badges]") as HTMLElement
    expect(
      within(positions)
        .getAllByRole("img")
        .map((img) => img.getAttribute("alt")),
    ).toEqual(["進化", "Token"])
    expect(screen.getByRole("img", { name: "從者" })).toBeVisible()
  },
)

it.each(["grid", "table", "list"])(
  "shows bans and copy limits next to existing labels in %s view",
  async (view) => {
    const cells = [0, 1].map((maxCopies) => ({
      ...cell,
      key: `restriction-${String(maxCopies)}`,
      summary: {
        ...cell.summary,
        restrictions: [
          {
            id: `r-${String(maxCopies)}`,
            formatCode: "standard" as const,
            maxCopies,
            effectiveFrom: "2026-01-01",
            sourceUrl: "https://example.invalid/list",
            asOf: "2026-09-25",
          },
        ],
      },
    }))
    const props = { cells, onOpen: vi.fn(), images: undefined }
    await renderInRouter(
      view === "grid" ? (
        <CardGrid {...props} />
      ) : view === "table" ? (
        <CardTable {...props} typeLabel={typeLabel} artOf={() => undefined} />
      ) : (
        <CardList {...props} typeLabel={typeLabel} classLabel={classLabel} />
      ),
    )
    expect(screen.getByText("禁止")).toBeVisible()
    expect(screen.getByText("限1")).toBeVisible()
  },
)
