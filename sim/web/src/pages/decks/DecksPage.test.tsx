import "fake-indexeddb/auto"

import { act, screen, waitFor, within } from "@testing-library/react"
import userEvent from "@testing-library/user-event"
import { afterAll, afterEach, beforeAll, beforeEach, describe, expect, it, vi } from "vitest"

import { buildSnapshot } from "../../../scripts/fixture/build"
import { routes } from "../../app/routes"
import { decksStore, DEFAULT_PREFS, prefsStore } from "../../settings"
import { renderRoutes } from "../../test-utils"
import { TestSearchWorker } from "../../test-utils/search-worker"

beforeEach(() => vi.stubGlobal("Worker", TestSearchWorker))

const built = await buildSnapshot({
  encodeImage: () => Promise.resolve(new TextEncoder().encode("stub")),
})

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
afterEach(async () => {
  prefsStore.set(DEFAULT_PREFS)
  await decksStore.reload()
  const state = decksStore.get()
  if (state.status === "ready") for (const deck of state.decks) await decksStore.remove(deck.id)
})

async function openList(initialEntries = ["/decks"]) {
  prefsStore.set({ uiLanguage: "zh-TW" })
  const rendered = await renderRoutes(routes, { initialEntries })
  await screen.findByText("還沒有牌組", {}, { timeout: 4000 })
  return rendered
}

/** The saved decks' names, in list order. */
function deckNames(): string[] {
  const list = screen.queryByRole("list", { name: "我的牌組" })
  return list
    ? within(list)
        .getAllByRole("heading", { level: 2 })
        .map((heading) => heading.textContent)
    : []
}

describe("DecksPage", () => {
  it("creates, duplicates, renames and deletes decks kept in the browser", async () => {
    await openList()
    const user = userEvent.setup()
    await user.click(screen.getAllByRole("button", { name: "新牌組" })[0] as HTMLElement)
    const dialog = await screen.findByRole("dialog", { name: "新牌組" })
    await user.type(within(dialog).getByLabelText("牌組名稱"), "精靈快攻")
    // Class choices come from the snapshot; the first one is preselected once they arrive.
    await waitFor(() => {
      expect(within(dialog).getByRole("button", { name: "建立" })).toBeEnabled()
    })
    await user.click(within(dialog).getByRole("button", { name: "建立" }))
    await waitFor(() => {
      expect(deckNames()).toEqual(["精靈快攻"])
    })
    const stored = decksStore.get()
    expect(stored.status === "ready" && stored.decks[0]).toMatchObject({
      name: "精靈快攻",
      edition: "jp",
      basis: { kind: "class" },
      revision: 1,
    })

    await user.click(screen.getByRole("button", { name: "複製 精靈快攻" }))
    await waitFor(() => {
      expect(deckNames().toSorted()).toEqual(["精靈快攻", "精靈快攻（複製）"])
    })

    await user.click(screen.getByRole("button", { name: "幫 精靈快攻 改名" }))
    const rename = await screen.findByRole("dialog", { name: "改名" })
    const input = within(rename).getByLabelText("牌組名稱")
    await user.clear(input)
    await user.type(input, "精靈中速")
    await user.click(within(rename).getByRole("button", { name: "改名" }))
    await waitFor(() => {
      expect(deckNames().toSorted()).toEqual(["精靈中速", "精靈快攻（複製）"])
    })

    await user.click(screen.getByRole("button", { name: "刪除 精靈快攻（複製）" }))
    const confirm = await screen.findByRole("dialog", { name: "刪除牌組？" })
    await user.click(within(confirm).getByRole("button", { name: "刪除" }))
    await waitFor(() => {
      expect(deckNames()).toEqual(["精靈中速"])
    })
  }, 20_000)

  it("names an unnamed deck after its position", async () => {
    await openList()
    const user = userEvent.setup()
    await user.click(screen.getAllByRole("button", { name: "新牌組" })[0] as HTMLElement)
    const dialog = await screen.findByRole("dialog", { name: "新牌組" })
    await waitFor(() => {
      expect(within(dialog).getByRole("button", { name: "建立" })).toBeEnabled()
    })
    await user.click(within(dialog).getByRole("button", { name: "建立" }))
    await waitFor(() => {
      expect(deckNames()).toEqual(["新牌組 1"])
    })
  }, 20_000)

  it("closes the dialog on back and stays on the list", async () => {
    const { router } = await openList(["/cards", "/decks"])
    const user = userEvent.setup()
    await user.click(screen.getAllByRole("button", { name: "新牌組" })[0] as HTMLElement)
    await screen.findByRole("dialog", { name: "新牌組" })
    await act(async () => {
      await router.navigate(-1)
    })
    await waitFor(() => {
      expect(screen.queryByRole("dialog", { name: "新牌組" })).not.toBeInTheDocument()
    })
    expect(router.state.location.pathname).toBe("/decks")
  }, 20_000)

  it("creates the deck in the edition chosen in the dialog, which becomes the card edition", async () => {
    await openList()
    const user = userEvent.setup()
    await user.click(screen.getAllByRole("button", { name: "新牌組" })[0] as HTMLElement)
    const dialog = await screen.findByRole("dialog", { name: "新牌組" })
    await user.click(within(dialog).getByRole("radio", { name: "英版" }))
    expect(prefsStore.get().cardEdition).toBe("en")
    await waitFor(() => {
      expect(within(dialog).getByRole("button", { name: "建立" })).toBeEnabled()
    })
    await user.click(within(dialog).getByRole("button", { name: "建立" }))
    await waitFor(() => {
      const state = decksStore.get()
      expect(state.status === "ready" && state.decks[0]?.edition).toBe("en")
    })
  }, 20_000)
})
