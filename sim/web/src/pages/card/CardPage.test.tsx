import { screen, waitFor, within } from "@testing-library/react"
import userEvent from "@testing-library/user-event"
import { afterAll, afterEach, beforeAll, describe, expect, it, vi } from "vitest"

import { buildSnapshot } from "../../../scripts/fixture/build"
import type { CardEntryState } from "../../app/listEntryState"
import { routes } from "../../app/routes"
import { DEFAULT_PREFS, prefsStore, recentStore } from "../../settings"
import { renderRoutes } from "../../test-utils"

const stubImage = ({ width, height, seed }: { width: number; height: number; seed: number }) =>
  Promise.resolve(
    new TextEncoder().encode(`stub:${String(width)}x${String(height)}:${String(seed)}`),
  )
const built = await buildSnapshot({ encodeImage: stubImage })

beforeAll(() => {
  vi.stubGlobal("fetch", (input: RequestInfo | URL) => {
    const url = typeof input === "string" ? input : input instanceof URL ? input.href : input.url
    const bytes = built.files.get(url.replace(/^.*\/cdn\//u, ""))
    return Promise.resolve(
      bytes ? new Response(bytes.slice().buffer) : new Response(null, { status: 404 }),
    )
  })
})
afterAll(() => {
  vi.unstubAllGlobals()
})
afterEach(() => {
  prefsStore.set(DEFAULT_PREFS)
  recentStore.clear()
})

const SLUG_051 = "/cards/BP01-051/%E8%A9%A6%E4%BD%9C%E3%81%AE%E8%81%96%E5%A0%82%E9%A8%8E%E5%A3%AB"
const listState: CardEntryState = {
  background: "",
  source: "results",
  pages: 1,
  resultKey: "c:bp01-051",
}

async function open(path: string, state?: unknown) {
  prefsStore.set({ uiLanguage: "zh-TW" })
  const rendered = await renderRoutes(routes, {
    initialEntries: [
      {
        pathname: path.split("?")[0] ?? path,
        search: path.includes("?") ? `?${path.split("?")[1] ?? ""}` : "",
        state,
      },
    ],
  })
  await screen.findByRole("heading", { level: 1, name: /試作|Prototype|BP01/u }, { timeout: 5000 })
  return rendered
}

describe("CardPage", () => {
  it("opens a direct link as a full page with the effect, icons and a back link", async () => {
    const { router } = await open("/cards/BP01-051")
    // The slug-less URL is replaced with the canonical one.
    expect(router.state.location.pathname).toBe(SLUG_051)
    expect(screen.queryByRole("dialog")).not.toBeInTheDocument()
    expect(screen.getByRole("link", { name: "回查卡" })).toHaveAttribute("href", "/cards")
    expect(screen.queryByRole("button", { name: "下一張" })).not.toBeInTheDocument()
    expect(screen.getByRole("heading", { level: 1 })).toHaveTextContent("試作聖堂騎士")
    expect(screen.getAllByText("BP01-051").length).toBeGreaterThan(0)
    // The icon shows in the original and again in the translation row.
    expect(screen.getAllByRole("img", { name: "入場曲" }).length).toBeGreaterThan(0)
    expect(screen.getAllByRole("img", { name: "費用 2" }).length).toBeGreaterThan(0)
    expect(screen.getByText("本站翻譯・非官方")).toBeInTheDocument()
    await waitFor(() => {
      expect(recentStore.get()).toEqual(["p:bp01-051"])
    })
  })

  it("explains a keyword in place and links to cards with it", async () => {
    await open("/cards/BP01-051")
    const user = userEvent.setup()
    const chip = screen.getAllByRole("button", { name: "守護" })[0]
    if (!chip) throw new Error("no keyword chip")
    await user.click(chip)
    expect(chip).toHaveAttribute("aria-expanded", "true")
    expect(screen.getByRole("link", { name: "找有【守護】的卡" })).toHaveAttribute(
      "href",
      "/cards?mech=kw%3Award",
    )
  })

  it("is a modal overlay with prev/next when opened with a background, and back closes it", async () => {
    prefsStore.set({ uiLanguage: "zh-TW" })
    const { router } = await renderRoutes(routes, { initialEntries: ["/cards"] })
    await screen.findByRole("group", { name: "職業" }, { timeout: 5000 })
    await router.navigate("/cards/BP01-051", { state: listState })
    await screen.findByRole("heading", { level: 1, name: /試作/u }, { timeout: 5000 })
    const user = userEvent.setup()
    const dialog = screen.getByRole("dialog")
    expect(dialog).toHaveAttribute("open")
    const back = within(dialog).getByRole("button", { name: "返回" })
    await waitFor(() => {
      expect(back).toHaveFocus()
    })
    expect(within(dialog).queryByRole("link", { name: "回查卡" })).not.toBeInTheDocument()
    // The list renders underneath, inert; prev/next live inside the dialog.
    expect(screen.getByRole("combobox", { name: "搜尋卡片", hidden: true })).toBeInTheDocument()
    const next = within(dialog).getByRole("button", { name: "下一張" })
    expect(next).toBeEnabled()
    await user.click(next)
    await waitFor(() => {
      expect(router.state.location.pathname).not.toBe(SLUG_051)
    })
    expect(router.state.location.state).toMatchObject({ source: "results", pages: 1 })
    expect((router.state.location.state as CardEntryState).resultKey).not.toBe("c:bp01-051")
    await user.click(within(screen.getByRole("dialog")).getByRole("button", { name: "上一張" }))
    await waitFor(() => {
      expect(router.state.location.pathname).toBe(SLUG_051)
    })
    expect((router.state.location.state as CardEntryState).resultKey).toBe("c:bp01-051")
    await user.keyboard("{Escape}")
    await waitFor(() => {
      expect(router.state.location.pathname).toBe("/cards")
    })
    expect(screen.queryByRole("dialog")).not.toBeInTheDocument()
  })

  it("hides prev/next when the card is not in the sequence and at the ends", async () => {
    await open("/cards/BP01-001", {
      background: "",
      source: "results",
      pages: 1,
      resultKey: "c:bp01-001",
    })
    expect(screen.getByRole("button", { name: "上一張" })).toBeDisabled()
    expect(screen.getByRole("button", { name: "下一張" })).toBeEnabled()
    const { unmount } = await open("/cards/BP01-051", {
      background: "?class=elf",
      source: "results",
      pages: 1,
      resultKey: "c:nope",
    })
    expect(screen.getAllByRole("button", { name: "上一張" }).at(-1)).toBeDisabled()
    expect(screen.getAllByRole("button", { name: "下一張" }).at(-1)).toBeDisabled()
    unmount()
  })

  it("follows a suggest sequence by card id", async () => {
    // "試作" suggests the first eight prototype cards in snapshot order; BP01-001 is followed by 002.
    const { router } = await open("/cards/BP01-001", {
      background: "?q=試作",
      source: "suggest",
      pages: 1,
      resultKey: "c:bp01-001",
    })
    const user = userEvent.setup()
    await user.click(screen.getByRole("button", { name: "下一張" }))
    await waitFor(() => {
      expect(router.state.location.pathname).toMatch(/^\/cards\/BP01-002/u)
    })
    expect(router.state.location.state).toMatchObject({
      source: "suggest",
      background: "?q=試作",
      resultKey: "c:bp01-002",
    })
  })

  it("redirects aliases and provisional ids, and reports a missing card with a search box", async () => {
    const { router } = await open("/cards/BP01-002A")
    expect(router.state.location.pathname).toMatch(/^\/cards\/BP01-002a/u)
    const missing = await renderRoutes(routes, { initialEntries: ["/cards/BP99-999"] })
    expect(
      await screen.findByText("找不到這張卡：BP99-999", {}, { timeout: 5000 }),
    ).toBeInTheDocument()
    expect(screen.getByRole("search")).toBeInTheDocument()
    missing.unmount()
    const provisional = await renderRoutes(routes, { initialEntries: ["/cards/_provisional/1001"] })
    await waitFor(() => {
      expect(provisional.router.state.location.pathname).toBe("/cards/_provisional/1001")
    })
    expect(
      await screen.findByRole("heading", { level: 1, name: "試作見習兵" }, { timeout: 5000 }),
    ).toBeInTheDocument()
    provisional.unmount()
  })

  it("tells an English-only card apart from a Japanese card without an English edition", async () => {
    await open("/cards/BP01EN-090")
    expect(screen.getByText(/尚未發行日文版|尚無已確認對應/u)).toBeInTheDocument()
    expect(screen.queryByText("尚未發行英文版")).not.toBeInTheDocument()
  })

  it("switches edition to the counterpart printing and keeps the entry state", async () => {
    const { router } = await open("/cards/BP01-051", listState)
    const user = userEvent.setup()
    await user.click(screen.getByRole("radio", { name: "英版" }))
    await waitFor(() => {
      expect(router.state.location.pathname).toMatch(/^\/cards\/BP01EN-051/u)
    })
    expect(router.state.location.state).toMatchObject(listState)
    expect(await screen.findByText("Ward", { exact: false })).toBeInTheDocument()
  })

  it("shows the English row for the en UI, and the missing notice for a Japanese-only card", async () => {
    prefsStore.set({ uiLanguage: "en" })
    const en = await renderRoutes(routes, { initialEntries: ["/cards/BP01-051"], language: "en" })
    expect(
      await screen.findByText("Official translation", {}, { timeout: 8000 }),
    ).toBeInTheDocument()
    en.unmount()
    // BP01-011 has no English text at all: the original stays, with the notice.
    const jpOnly = await renderRoutes(routes, {
      initialEntries: ["/cards/BP01-011"],
      language: "en",
    })
    expect(
      await screen.findByText("No English text yet; showing the original", {}, { timeout: 8000 }),
    ).toBeInTheDocument()
    jpOnly.unmount()
  }, 20000)
})
