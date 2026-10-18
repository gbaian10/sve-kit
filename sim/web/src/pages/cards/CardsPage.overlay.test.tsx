import { act, fireEvent, screen, waitFor, within } from "@testing-library/react"
import userEvent from "@testing-library/user-event"
import { afterAll, afterEach, beforeAll, beforeEach, describe, expect, it, vi } from "vitest"

import { buildSnapshot } from "../../../scripts/fixture/build"
import { routes } from "../../app/routes"
import {
  type CardOpenMode,
  DEFAULT_PREFS,
  prefsStore,
  readPrefs,
  recentStore,
} from "../../settings"
import { renderRoutes } from "../../test-utils"
import { TestAppWorker } from "../../test-utils/app-workers"

beforeEach(() => {
  vi.stubGlobal("Worker", TestAppWorker)
  // jsdom never decodes images; the card content waits on decode before it commits.
  Object.defineProperty(HTMLImageElement.prototype, "decode", {
    configurable: true,
    value: () => Promise.resolve(),
  })
})

const stubImage = ({ width, height, seed }: { width: number; height: number; seed: number }) =>
  Promise.resolve(
    new TextEncoder().encode(`stub:${String(width)}x${String(height)}:${String(seed)}`),
  )
const built = await buildSnapshot({ encodeImage: stubImage })

// jsdom has no layout: the window width the drawer fits itself to comes from this switch.
let desktop = true
const matchMedia = (query: string): MediaQueryList =>
  ({
    matches: false,
    media: query,
    onchange: null,
    addEventListener: () => undefined,
    removeEventListener: () => undefined,
    addListener: () => undefined,
    removeListener: () => undefined,
    dispatchEvent: () => false,
  }) as MediaQueryList

beforeAll(() => {
  vi.stubGlobal("fetch", (input: RequestInfo | URL) => {
    const url = typeof input === "string" ? input : input instanceof URL ? input.href : input.url
    const bytes = built.files.get(url.replace(/^.*\/cdn\//u, ""))
    return Promise.resolve(
      bytes ? new Response(bytes.slice().buffer) : new Response(null, { status: 404 }),
    )
  })
  vi.stubGlobal("matchMedia", matchMedia)
})
afterAll(() => {
  vi.unstubAllGlobals()
})
afterEach(() => {
  desktop = true
  prefsStore.set(DEFAULT_PREFS)
  recentStore.clear()
})

function resizeWindow(width: number) {
  vi.stubGlobal("innerWidth", width)
  window.dispatchEvent(new Event("resize"))
}

async function open(path: string, mode: CardOpenMode) {
  prefsStore.set({ uiLanguage: "zh-TW", cardOpenMode: mode })
  vi.stubGlobal("innerWidth", desktop ? 1440 : 390)
  const rendered = await renderRoutes(routes, { initialEntries: [path] })
  // Scoping avoids computing accessible names for the collapsed filter panel on every retry.
  const classes = await screen.findByRole("group", { name: "職業" })
  await within(classes).findByRole("button", { name: "精靈" }, { timeout: 4000 })
  return rendered
}

function cellLink(key: string): HTMLElement {
  const link = document.querySelector<HTMLElement>(`a[data-result-key="${key}"]`)
  if (!link) throw new Error(`missing result cell ${key}`)
  return link
}

async function cells(): Promise<HTMLElement[]> {
  return waitFor(() => {
    const links = [...document.querySelectorAll<HTMLElement>("a[data-result-key]")]
    expect(links.length).toBeGreaterThan(1)
    return links
  })
}

const panel = () => screen.getByRole("complementary", { name: "單卡面板" })
const noPanel = () => screen.queryByRole("complementary", { name: "單卡面板" })

describe("opening a card beside the results", () => {
  it.each(["grid", "table", "list"])(
    "opens the clicked %s cell in the side panel, replaces it with the next click and marks the cell",
    async (view) => {
      const { router } = await open(`/cards?q=bp01&view=${view}`, "panel")
      const user = userEvent.setup()
      const [first, second] = await cells()
      if (!first || !second) throw new Error("missing cells")
      const firstKey = first.getAttribute("data-result-key") ?? ""
      const secondKey = second.getAttribute("data-result-key") ?? ""
      await user.click(first)
      expect(router.state.location.pathname).toBe("/cards")
      expect(new URLSearchParams(router.state.location.search).get("card")).toBe("BP01-001")
      expect(router.state.location.state).toMatchObject({
        overlay: { resultKey: firstKey, pushed: true },
      })
      const aside = panel()
      expect(aside).toHaveFocus()
      await within(aside).findByRole("heading", { name: "試作見習兵" }, { timeout: 5000 })
      expect(cellLink(firstKey)).toHaveAttribute("aria-current", "true")

      // The results stay: another click swaps the card without a second history entry.
      const entries = router.state.location.key
      await user.click(cellLink(secondKey))
      expect(router.state.historyAction).toBe("REPLACE")
      expect(router.state.location.key).not.toBe(entries)
      expect(new URLSearchParams(router.state.location.search).get("card")).not.toBe("BP01-001")
      expect(cellLink(secondKey)).toHaveAttribute("aria-current", "true")
      expect(cellLink(firstKey)).not.toHaveAttribute("aria-current")
      await waitFor(() => {
        expect(within(panel()).queryByRole("heading", { name: "試作見習兵" })).toBeNull()
      })
    },
  )

  it("also opens the panel on a set page", async () => {
    const { router } = await open("/sets/BP01", "panel")
    const user = userEvent.setup()
    const [first] = await cells()
    if (!first) throw new Error("missing cell")
    await user.click(first)
    expect(router.state.location.pathname).toBe("/sets/BP01")
    expect(router.state.location.search).toBe("?card=BP01-001")
    await within(panel()).findByRole("heading", { name: "試作見習兵" }, { timeout: 5000 })
  })

  it("closes with Escape and gives focus back to the cell, popping the entry it pushed", async () => {
    const { router } = await open("/cards?q=bp01", "panel")
    const user = userEvent.setup()
    const [first] = await cells()
    if (!first) throw new Error("missing cell")
    const key = first.getAttribute("data-result-key") ?? ""
    await user.click(first)
    await within(panel()).findByRole("heading", { name: "試作見習兵" }, { timeout: 5000 })
    await user.keyboard("{Escape}")
    await waitFor(() => {
      expect(noPanel()).toBeNull()
    })
    expect(router.state.location.search).toBe("?q=bp01")
    expect(router.state.historyAction).toBe("POP")
    expect(cellLink(key)).toHaveFocus()
  })

  it("closes when the browser goes back, and the close button replaces a shared link's URL", async () => {
    const { router } = await open("/cards?q=bp01&card=BP01-002", "panel")
    const user = userEvent.setup()
    await within(panel()).findByRole("heading", { name: "試作隊長" }, { timeout: 5000 })
    // No entry was pushed for a URL that arrived open: closing rewrites it in place.
    await user.click(within(panel()).getByRole("button", { name: "關閉面板" }))
    await waitFor(() => {
      expect(noPanel()).toBeNull()
    })
    expect(router.state.location.search).toBe("?q=bp01")
    expect(router.state.historyAction).toBe("REPLACE")

    const [first] = await cells()
    if (!first) throw new Error("missing cell")
    await user.click(first)
    await within(panel()).findByRole("heading", { name: "試作見習兵" }, { timeout: 5000 })
    await act(() => router.navigate(-1))
    await waitFor(() => {
      expect(noPanel()).toBeNull()
    })
    expect(router.state.location.search).toBe("?q=bp01")
  })

  it("copies the card page's own address and opens that page in full", async () => {
    const { router } = await open("/cards?q=bp01", "panel")
    // user-event installs a clipboard stub the test reads back.
    const user = userEvent.setup()
    const [first] = await cells()
    if (!first) throw new Error("missing cell")
    await user.click(first)
    const aside = panel()
    await within(aside).findByRole("heading", { name: "試作見習兵" }, { timeout: 5000 })
    await user.click(within(aside).getByRole("button", { name: "複製連結" }))
    expect(await screen.findByText("已複製這張卡的連結")).toBeInTheDocument()
    expect(await navigator.clipboard.readText()).toBe("http://localhost:3000/cards/BP01-001")

    await user.click(within(aside).getByRole("button", { name: "全螢幕顯示" }))
    await waitFor(() => {
      expect(router.state.location.pathname).toBe("/cards/BP01-001")
    })
    expect(router.state.location.state).toMatchObject({
      background: "?q=bp01",
      source: "results",
      resultKey: "p:bp01-001",
    })
    await waitFor(() => {
      expect(noPanel()).toBeNull()
    })
  })

  // What width the drawer and the frame share at each window width is `effectiveDrawerWidth`'s
  // contract (card-overlay.test.ts); the real geometry is checked in a browser by
  // `bun run check:drawer`. Here: the preference the keyboard writes, and the frame that a saved
  // width no longer fits.
  it("saves the width the keyboard sets and falls back to the modal when the window is too narrow", async () => {
    await open("/cards?q=bp01", "panel")
    const user = userEvent.setup()
    const [first] = await cells()
    if (!first) throw new Error("missing cell")
    await user.click(first)
    const handle = within(panel()).getByRole("separator", { name: "調整抽屜寬度" })
    handle.focus()
    await user.keyboard("{ArrowLeft}")
    expect(readPrefs().drawerWidth).toBe(640)
    await user.keyboard("{ArrowRight}{ArrowRight}")
    expect(readPrefs().drawerWidth).toBe(600)

    // A saved width wider than the window allows still shows the drawer, not a broken frame.
    prefsStore.set({ drawerWidth: 1200 })
    act(() => {
      resizeWindow(1180)
    })
    expect(panel()).toBeInTheDocument()
    expect(screen.queryByRole("dialog", { name: "單卡面板" })).toBeNull()

    // Below the room for a drawer plus usable results the card becomes a modal instead.
    act(() => {
      resizeWindow(900)
    })
    await waitFor(() => {
      expect(noPanel()).toBeNull()
    })
    expect(screen.getByRole("dialog", { name: "單卡面板" })).toHaveAttribute("open")
    expect(readPrefs().drawerWidth).toBe(1200)
  })

  it("keeps the card's result cell and the return focus when another printing is chosen", async () => {
    const { router } = await open("/cards?q=bp01-002&unit=card", "panel")
    const user = userEvent.setup()
    const [first] = await cells()
    if (!first) throw new Error("missing cell")
    const key = first.getAttribute("data-result-key") ?? ""
    expect(key).toBe("c:bp01-002")
    await user.click(first)
    const aside = panel()
    await within(aside).findByRole("heading", { name: "試作隊長" }, { timeout: 5000 })
    const pill = await within(aside).findByRole("link", { name: /^BP01-002a/u }, { timeout: 5000 })
    await user.click(pill)
    await waitFor(() => {
      expect(new URLSearchParams(router.state.location.search).get("card")).toBe("BP01-002a")
    })
    expect(router.state.historyAction).toBe("REPLACE")
    expect(router.state.location.state).toMatchObject({
      overlay: { resultKey: key, opener: key, pushed: true },
    })
    expect(cellLink(key)).toHaveAttribute("aria-current", "true")
    await within(aside).findByRole("button", { name: "下一張" }, { timeout: 5000 })
    await waitFor(() => {
      expect(within(aside).getByRole("button", { name: "下一張" })).toBeEnabled()
    })
    await user.keyboard("{Escape}")
    await waitFor(() => {
      expect(noPanel()).toBeNull()
    })
    expect(cellLink(key)).toHaveFocus()
  })

  it.each([
    ["card", "printing", "c:bp01-001", "p:bp01-001"],
    ["printing", "card", "p:bp01-001", "c:bp01-001"],
  ] as const)(
    "follows the open card to its %s → %s cell when the row unit changes",
    async (from, to, before, after) => {
      prefsStore.set({ filterPanelOpen: true })
      const { router } = await open(`/cards?q=bp01${from === "card" ? "&unit=card" : ""}`, "panel")
      const user = userEvent.setup()
      const [first] = await cells()
      if (!first) throw new Error("missing cell")
      expect(first.getAttribute("data-result-key")).toBe(before)
      await user.click(first)
      const aside = panel()
      await within(aside).findByRole("heading", { name: "試作見習兵" }, { timeout: 5000 })
      await user.selectOptions(screen.getByRole("combobox", { name: "顯示方式" }), to)
      await waitFor(() => {
        expect(new URLSearchParams(router.state.location.search).get("unit")).toBe(
          to === "card" ? "card" : null,
        )
      })
      await waitFor(() => {
        expect(cellLink(after)).toHaveAttribute("aria-current", "true")
      })
      expect(document.querySelectorAll("a[data-result-key][aria-current]")).toHaveLength(1)
      await waitFor(() => {
        expect(within(panel()).getByRole("button", { name: "下一張" })).toBeEnabled()
      })
      await user.keyboard("{Escape}")
      await waitFor(() => {
        expect(noPanel()).toBeNull()
      })
      expect(cellLink(after)).toHaveFocus()
    },
  )

  it.each([
    ["card", "printing", "c:bp01-001", "p:bp01-001", "p:bp01-002"],
    ["printing", "card", "p:bp01-001", "c:bp01-001", "c:bp01-002"],
  ] as const)(
    "returns to the opening cell, not the card moved to, after a %s → %s unit change",
    async (from, to, opened, openerAfter, shownAfter) => {
      prefsStore.set({ filterPanelOpen: true })
      const { router } = await open(`/cards?q=bp01${from === "card" ? "&unit=card" : ""}`, "panel")
      const user = userEvent.setup()
      const [first] = await cells()
      if (!first) throw new Error("missing cell")
      expect(first.getAttribute("data-result-key")).toBe(opened)
      await user.click(first)
      const aside = panel()
      await within(aside).findByRole("heading", { name: "試作見習兵" }, { timeout: 5000 })
      const next = await within(aside).findByRole("button", { name: "下一張" }, { timeout: 5000 })
      await waitFor(() => expect(next).toBeEnabled())
      await user.click(next)
      await within(aside).findByRole("heading", { name: "試作隊長" }, { timeout: 5000 })
      await user.selectOptions(screen.getByRole("combobox", { name: "顯示方式" }), to)
      await waitFor(() => {
        expect(new URLSearchParams(router.state.location.search).get("unit")).toBe(
          to === "card" ? "card" : null,
        )
      })
      // The shown card is marked in the new unit; focus will go back to where it started.
      await waitFor(() => {
        expect(cellLink(shownAfter)).toHaveAttribute("aria-current", "true")
      })
      await user.keyboard("{Escape}")
      await waitFor(() => {
        expect(noPanel()).toBeNull()
      })
      expect(cellLink(openerAfter)).toHaveFocus()
    },
  )

  it.each([
    ["sort", "sort=cost"],
    ["view", "view=table"],
  ] as const)(
    "keeps the card through a %s change, yet back closes it and back again is the old list",
    async (change, expected) => {
      const { router } = await open("/cards?q=bp01", "panel")
      const user = userEvent.setup()
      const [first] = await cells()
      if (!first) throw new Error("missing cell")
      await user.click(first)
      await within(panel()).findByRole("heading", { name: "試作見習兵" }, { timeout: 5000 })
      if (change === "sort")
        await user.selectOptions(screen.getByRole("combobox", { name: "排序" }), "cost")
      else await user.click(screen.getByRole("radio", { name: "表格" }))
      await waitFor(() => {
        expect(router.state.location.search).toBe(`?q=bp01&${expected}&card=BP01-001`)
      })
      expect(panel()).toBeInTheDocument()
      expect(router.state.location.state).toMatchObject({ overlay: { pushed: true } })

      await act(() => router.navigate(-1))
      await waitFor(() => {
        expect(noPanel()).toBeNull()
      })
      expect(router.state.location.search).toBe(`?q=bp01&${expected}`)
      await act(() => router.navigate(-1))
      expect(router.state.location.search).toBe("?q=bp01")
      expect(noPanel()).toBeNull()
    },
  )

  it("names the English edition in a copied link", async () => {
    await open("/cards?q=bp01&ed=en", "panel")
    const user = userEvent.setup()
    const [first] = await cells()
    if (!first) throw new Error("missing cell")
    await user.click(first)
    const aside = panel()
    await within(aside).findByRole("heading", { name: "試作見習兵" }, { timeout: 5000 })
    await user.click(within(aside).getByRole("button", { name: "複製連結" }))
    expect(await screen.findByText("已複製這張卡的連結")).toBeInTheDocument()
    expect(await navigator.clipboard.readText()).toBe(
      "http://localhost:3000/cards/BP01EN-001?ed=en",
    )
  })

  it("walks the results with previous and next inside the panel", async () => {
    const { router } = await open("/cards?q=bp01", "panel")
    const user = userEvent.setup()
    const [first] = await cells()
    if (!first) throw new Error("missing cell")
    await user.click(first)
    const aside = panel()
    await within(aside).findByRole("heading", { name: "試作見習兵" }, { timeout: 5000 })
    const next = await within(aside).findByRole("button", { name: "下一張" })
    await waitFor(() => expect(next).toBeEnabled())
    await user.click(next)
    expect(router.state.historyAction).toBe("REPLACE")
    expect(router.state.location.pathname).toBe("/cards")
    await within(aside).findByRole("heading", { name: "試作隊長" }, { timeout: 5000 })
    expect(new URLSearchParams(router.state.location.search).get("card")).toBe("BP01-002")
    expect(cellLink("p:bp01-002")).toHaveAttribute("aria-current", "true")
  })
})

describe("opening a card over the results", () => {
  it("opens a modal that Escape closes", async () => {
    const { router } = await open("/cards?q=bp01", "modal")
    const user = userEvent.setup()
    const [first] = await cells()
    if (!first) throw new Error("missing cell")
    await user.click(first)
    const dialog = screen.getByRole("dialog", { name: "單卡面板" })
    expect(dialog).toHaveAttribute("open")
    expect(noPanel()).toBeNull()
    await within(dialog).findByRole("heading", { name: "試作見習兵" }, { timeout: 5000 })
    expect(new URLSearchParams(router.state.location.search).get("card")).toBe("BP01-001")
    fireEvent(dialog, new Event("cancel", { cancelable: true }))
    await waitFor(() => {
      expect(dialog).not.toHaveAttribute("open")
    })
    expect(router.state.location.search).toBe("?q=bp01")
  })

  it("falls back to the modal on a narrow screen, even when the panel is chosen", async () => {
    desktop = false
    await open("/cards?q=bp01", "panel")
    const user = userEvent.setup()
    const [first] = await cells()
    if (!first) throw new Error("missing cell")
    await user.click(first)
    expect(screen.getByRole("dialog", { name: "單卡面板" })).toHaveAttribute("open")
    expect(noPanel()).toBeNull()
  })

  it.each([
    [true, "complementary"],
    [false, "dialog"],
  ] as const)(
    "picks the frame by screen width when the preference is auto (desktop: %s)",
    async (wide, role) => {
      desktop = wide
      await open("/cards?q=bp01", "auto")
      const user = userEvent.setup()
      const [first] = await cells()
      if (!first) throw new Error("missing cell")
      await user.click(first)
      expect(screen.getByRole(role, { name: "單卡面板" })).toBeInTheDocument()
    },
  )
})

describe("opening a card as its page", () => {
  it("navigates to the card page when the preference says so", async () => {
    const { router } = await open("/cards?q=bp01", "page")
    const user = userEvent.setup()
    const [first] = await cells()
    if (!first) throw new Error("missing cell")
    await user.click(first)
    expect(router.state.location.pathname).toBe("/cards/BP01-001")
    expect(router.state.location.state).toMatchObject({
      source: "results",
      resultKey: "p:bp01-001",
    })
  })

  it("turns a shared panel URL into the card page", async () => {
    prefsStore.set({ uiLanguage: "zh-TW", cardOpenMode: "page" })
    const { router } = await renderRoutes(routes, {
      initialEntries: ["/cards?q=bp01&card=BP01-002"],
    })
    await waitFor(() => {
      expect(router.state.location.pathname).toBe("/cards/BP01-002")
    })
    expect(router.state.historyAction).toBe("REPLACE")
  })
})
