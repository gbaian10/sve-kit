import { act, fireEvent, screen, waitFor, within } from "@testing-library/react"
import userEvent from "@testing-library/user-event"
import { afterEach, beforeEach, expect, it, vi } from "vitest"

import { routes } from "../../app/routes"
import type { RuleNode, RulesBundle } from "../../data"
import { DEFAULT_PREFS, prefsStore } from "../../settings"
import { renderRoutes } from "../../test-utils"
import { TestSearchWorker } from "../../test-utils/search-worker"

// Synthetic rules only; the real data is checked by src/data/rules/data.test.ts.
const nodes: RuleNode[] = [
  { key: "1", context: "main", number: "1", kind: "chapter", depth: 1, title: "章一", text: "" },
  {
    key: "1.1",
    context: "main",
    number: "1.1",
    kind: "section",
    depth: 2,
    title: "節一",
    text: "",
  },
  {
    key: "1.1.1",
    context: "main",
    number: "1.1.1",
    kind: "rule",
    depth: 3,
    text: "本文一（1.1.2）参照。{起動}と{PP}。",
    examples: ["例：例一"],
  },
  {
    key: "1.1.2",
    context: "main",
    number: "1.1.2",
    kind: "rule",
    depth: 3,
    text: "本文二。\n* 項目甲\n* 項目乙",
  },
  { key: "B-1", context: "B-1", kind: "section", depth: 2, title: "B-1：附錄", text: "附錄說明。" },
  {
    key: "B-1:1.1.2",
    context: "B-1",
    number: "1.1.2",
    kind: "rule",
    depth: 5,
    text: "附錄本文（1.1.2）。",
  },
  {
    key: "A:1",
    context: "A",
    kind: "block",
    depth: 2,
    title: "表名",
    text: "",
    table: { header: ["名", "職"], rows: [["甲", "乙"]] },
  },
]
const bundle = (translations: RulesBundle["translations"] = new Map()): RulesBundle => ({
  index: {
    format: 1,
    kind: "comprehensive-rules-index",
    version: "9.9.9",
    region: "jp",
    last_updated: "2099-01-02",
    page_url: "https://example.invalid/rules/",
    source_url: "https://example.invalid/rules.pdf",
    raw_sha256: "0".repeat(64),
    pages: 1,
    contents: [],
  },
  nodes,
  byKey: new Map(nodes.map((node) => [node.key, node])),
  translations,
})
const current = vi.hoisted((): { value: unknown } => ({ value: undefined }))
vi.mock("../../data/rules/bundle", () => ({
  get RULES() {
    return current.value
  },
}))

const scrollIntoView = vi.fn()
beforeEach(() => {
  scrollIntoView.mockClear()
  vi.stubGlobal("Worker", TestSearchWorker)
  Element.prototype.scrollIntoView = scrollIntoView
  current.value = bundle()
  prefsStore.set({ uiLanguage: "zh-TW" })
})
afterEach(() => {
  prefsStore.set(DEFAULT_PREFS)
  vi.unstubAllGlobals()
})

const rule = (key: string) => document.getElementById(`r-${key.replace(":", "-")}`)

it("shows the version facts and the Japanese original while no translation exists", async () => {
  await renderRoutes(routes, { initialEntries: ["/rules"] })
  expect(await screen.findByText("ver. 9.9.9")).toBeInTheDocument()
  expect(screen.getByText("官方最終更新：2099-01-02")).toBeInTheDocument()
  expect(screen.getByRole("link", { name: "官方規則書 PDF" })).toHaveAttribute(
    "href",
    "https://example.invalid/rules.pdf",
  )
  expect(screen.getByText("繁中譯文準備中，目前先顯示日文原文。")).toBeInTheDocument()
  const chapter = within(screen.getByRole("article")).getByRole("heading", { level: 2 })
  expect(chapter).toHaveTextContent("1章一")
  expect(within(chapter).getByText("章一").closest("[lang]")).toHaveAttribute("lang", "ja")
  // No per-node tags while nothing at all is translated.
  expect(within(screen.getByRole("article")).queryByText("原文")).not.toBeInTheDocument()
})

it("renders icons, unknown markers, bullets, examples and resolved references", async () => {
  await renderRoutes(routes, { initialEntries: ["/rules"] })
  const first = await waitFor(() => {
    const element = rule("1.1.1")
    expect(element).not.toBeNull()
    return element as HTMLElement
  })
  expect(within(first).getByRole("img", { name: "起動" })).toBeInTheDocument()
  expect(within(first).getByText("PP")).toBeInTheDocument()
  expect(within(first).getByText("例：例一")).toBeInTheDocument()
  expect(within(first).getByRole("link", { name: "1.1.2" })).toHaveAttribute(
    "href",
    "/rules#r-1.1.2",
  )
  expect(within(rule("1.1.2") as HTMLElement).getAllByRole("listitem")).toHaveLength(2)
  // Inside appendix B-1 a citation means the appendix's own restated clause first.
  expect(
    within(rule("B-1:1.1.2") as HTMLElement).getByRole("link", { name: "1.1.2" }),
  ).toHaveAttribute("href", "/rules#r-B-1-1.1.2")
})

it("uses translations where present and tags the pieces still in Japanese", async () => {
  current.value = bundle(
    new Map([
      ["1", { title: "第一章" }],
      ["1.1.1", { text: "譯文一（1.1.2）。" }],
    ]),
  )
  await renderRoutes(routes, { initialEntries: ["/rules"] })
  expect(
    await screen.findByText("譯文準備中：已翻譯 2／7 條，其餘先顯示日文原文。"),
  ).toBeInTheDocument()
  const chapter = within(screen.getByRole("article")).getByRole("heading", { level: 2 })
  expect(within(chapter).getByText("第一章").closest("span[lang]")).toBeNull()
  const translated = rule("1.1.1") as HTMLElement
  expect(within(translated).getByRole("link", { name: "1.1.2" })).toBeInTheDocument()
  // Its example is still untranslated, so the node carries the tag.
  expect(within(translated).getByText("原文")).toBeInTheDocument()
  expect(within(rule("1.1.2") as HTMLElement).getByText("原文")).toBeInTheDocument()
})

it("shows the original in the Japanese UI and offers it on request in the English UI", async () => {
  prefsStore.set({ uiLanguage: "ja" })
  const first = await renderRoutes(routes, { initialEntries: ["/rules"], language: "ja" })
  expect(await screen.findByText("ver. 9.9.9")).toBeInTheDocument()
  expect(screen.queryByText(/準備中/u)).not.toBeInTheDocument()
  expect(rule("1.1.1")).not.toBeNull()
  first.unmount()

  prefsStore.set({ uiLanguage: "en" })
  await renderRoutes(routes, { initialEntries: ["/rules"], language: "en" })
  expect(
    await screen.findByText("The English comprehensive rules are not available yet."),
  ).toBeInTheDocument()
  expect(rule("1.1.1")).toBeNull()
  fireEvent.click(screen.getByRole("button", { name: "Show the Japanese original" }))
  expect(rule("1.1.1")).not.toBeNull()
  expect(screen.getByRole("button", { name: "Hide the Japanese original" })).toHaveAttribute(
    "aria-pressed",
    "true",
  )
})

it("scrolls to the hash target on entry and marks it briefly", async () => {
  vi.useFakeTimers({ shouldAdvanceTime: true })
  try {
    await renderRoutes(routes, { initialEntries: ["/rules#r-1.1.2"] })
    await waitFor(() => {
      expect(rule("1.1.2")).toHaveAttribute("data-flash")
    })
    expect(scrollIntoView).toHaveBeenCalled()
    act(() => {
      vi.advanceTimersByTime(1600)
    })
    expect(rule("1.1.2")).not.toHaveAttribute("data-flash")
  } finally {
    vi.useRealTimers()
  }
})

it("copies a clause link from its number and moves the hash there", async () => {
  // user-event installs a clipboard stub the test reads back.
  const user = userEvent.setup()
  const { router } = await renderRoutes(routes, { initialEntries: ["/rules"] })
  await user.click(await screen.findByRole("button", { name: "複製 1.1.1 的連結" }))
  await waitFor(() => {
    expect(router.state.location.hash).toBe("#r-1.1.1")
  })
  expect(await screen.findByText("已複製 1.1.1 的連結")).toBeInTheDocument()
  expect(await navigator.clipboard.readText()).toBe("http://localhost:3000/rules#r-1.1.1")
})

it("lists chapters and sections in the table of contents and jumps from it", async () => {
  const { router } = await renderRoutes(routes, { initialEntries: ["/rules"] })
  const toc = await screen.findByRole("navigation", { name: "目錄" })
  const links = within(toc).getAllByRole("link")
  expect(links.map((link) => link.textContent)).toEqual(["1章一", "1.1節一", "B-1：附錄"])
  fireEvent.click(within(toc).getByRole("link", { name: /節一/u }))
  await waitFor(() => {
    expect(router.state.location.hash).toBe("#r-1.1")
  })
})

it("marks the section the reader has scrolled to", async () => {
  await renderRoutes(routes, { initialEntries: ["/rules"] })
  const toc = await screen.findByRole("navigation", { name: "目錄" })
  const tops = new Map([
    ["r-1", -400],
    ["r-1.1", 300],
    ["r-B-1", 900],
  ])
  const rect = vi.spyOn(Element.prototype, "getBoundingClientRect").mockImplementation(function (
    this: Element,
  ) {
    return { top: tops.get(this.id) ?? 0 } as DOMRect
  })
  try {
    fireEvent.scroll(window)
    await waitFor(() => {
      expect(within(toc).getByRole("link", { name: /章一/u })).toHaveAttribute(
        "aria-current",
        "location",
      )
    })
    tops.set("r-1.1", 40)
    fireEvent.scroll(window)
    await waitFor(() => {
      expect(within(toc).getByRole("link", { name: /節一/u })).toHaveAttribute(
        "aria-current",
        "location",
      )
    })
  } finally {
    rect.mockRestore()
  }
})

it("opens the contents as a sheet on small screens and jumps after closing it", async () => {
  const { router } = await renderRoutes(routes, { initialEntries: ["/rules"] })
  fireEvent.click(await screen.findByRole("button", { name: "目錄" }))
  const sheet = await screen.findByRole("dialog", { name: "目錄" })
  fireEvent.click(within(sheet).getByRole("link", { name: /B-1：附錄/u }))
  await waitFor(() => {
    expect(router.state.location.hash).toBe("#r-B-1")
  })
  expect(screen.queryByRole("dialog", { name: "目錄" })).not.toBeInTheDocument()
})

it("previews a cited clause on hover and focus", async () => {
  await renderRoutes(routes, { initialEntries: ["/rules"] })
  await waitFor(() => {
    expect(rule("1.1.1")).not.toBeNull()
  })
  const link = within(rule("1.1.1") as HTMLElement).getByRole("link", { name: "1.1.2" })
  fireEvent.mouseEnter(link)
  const tip = await screen.findByRole("tooltip")
  expect(tip).toHaveTextContent("1.1.2本文二。")
  // Text inside the preview never links further.
  expect(within(tip).queryByRole("link")).not.toBeInTheDocument()
  expect(link).toHaveAttribute("aria-describedby", tip.id)
  fireEvent.mouseLeave(link)
  expect(screen.queryByRole("tooltip")).not.toBeInTheDocument()
  fireEvent.focus(link)
  expect(await screen.findByRole("tooltip")).toBeInTheDocument()
})

it("gives untranslated section bodies lang=ja in the Chinese and English views", async () => {
  await renderRoutes(routes, { initialEntries: ["/rules"] })
  const section = await waitFor(() => {
    const element = rule("B-1")
    expect(element).not.toBeNull()
    return element as HTMLElement
  })
  expect(within(section).getByText("附錄說明。").closest("[lang]")).toHaveAttribute("lang", "ja")
})

it("marks each untranslated table cell only while the rules are partly translated", async () => {
  current.value = bundle(
    new Map([["A:1", { title: "表", table: { header: ["名稱", null], rows: [["甲譯", null]] } }]]),
  )
  const view = await renderRoutes(routes, { initialEntries: ["/rules"] })
  const table = await waitFor(() => {
    const element = rule("A:1")
    expect(element).not.toBeNull()
    return element as HTMLElement
  })
  const desktop = within(table).getByRole("table")
  const cell = within(desktop).getByText("乙").closest("td") as HTMLElement
  expect(within(cell).getByText("原文")).toBeInTheDocument()
  expect(within(cell).getByText("乙").closest("[lang]")).toHaveAttribute("lang", "ja")
  expect(
    within(within(desktop).getByText("職").closest("th") as HTMLElement).getByText("原文"),
  ).toBeInTheDocument()
  expect(
    within(within(desktop).getByText("甲譯").closest("td") as HTMLElement).queryByText("原文"),
  ).toBeNull()
  // The phone card shows the text column without its header, so only the cell is marked.
  const card = within(table).getByRole("listitem")
  expect(within(card).getAllByText("原文")).toHaveLength(1)
  view.unmount()

  prefsStore.set({ uiLanguage: "ja" })
  await renderRoutes(routes, { initialEntries: ["/rules"], language: "ja" })
  const original = await waitFor(() => {
    const element = rule("A:1")
    expect(element).not.toBeNull()
    return element as HTMLElement
  })
  expect(within(original).queryByText("原文")).toBeNull()
})

it("ignores a malformed hash instead of failing the page", async () => {
  await renderRoutes(routes, { initialEntries: ["/rules#%"] })
  await waitFor(() => {
    expect(rule("1.1.1")).not.toBeNull()
  })
  expect(screen.getByRole("article")).toBeInTheDocument()
  expect(scrollIntoView).not.toHaveBeenCalled()
})
