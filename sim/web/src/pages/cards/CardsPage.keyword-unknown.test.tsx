import { act, screen, waitFor, within } from "@testing-library/react"
import { afterAll, beforeAll, describe, expect, it, vi } from "vitest"

import { buildSnapshot } from "../../../scripts/fixture/build"
import { CARDS } from "../../../scripts/fixture/cards"
import { routes } from "../../app/routes"
import { DEFAULT_PREFS, prefsStore } from "../../settings"
import { renderRoutes } from "../../test-utils"
import { TestSearchWorker } from "../../test-utils/search-worker"

// A separate root lifetime keeps this alternate snapshot isolated from the default page fixture.
const built = await buildSnapshot({
  encodeImage: () => Promise.resolve(new Uint8Array()),
  cards: CARDS.map((card) => ({ ...card, enBlock: ["unconfirmed_mapping"] })),
})

let indexRequests = 0

beforeAll(() => {
  vi.stubGlobal("Worker", TestSearchWorker)
  vi.stubGlobal("fetch", (input: RequestInfo | URL) => {
    const url = typeof input === "string" ? input : input instanceof URL ? input.href : input.url
    if (url.endsWith("/snapshots/versions/index.json")) indexRequests++
    const bytes = built.files.get(url.replace(/^.*\/cdn\//u, ""))
    return Promise.resolve(
      bytes ? new Response(bytes.slice().buffer) : new Response(null, { status: 404 }),
    )
  })
})
afterAll(() => {
  prefsStore.set(DEFAULT_PREFS)
  vi.unstubAllGlobals()
})

describe("filter panel", () => {
  it("explains unsupported English keyword links in every UI language with the panel collapsed", async () => {
    prefsStore.set({ uiLanguage: "zh-TW", filterPanelOpen: false })
    await renderRoutes(routes, { initialEntries: ["/cards?ed=en&mech=kw%3Award"] })
    // The unsupported result waits for mechanics, which loads after this catalog is ready.
    const classes = await screen.findByRole("group", { name: "職業" })
    await within(classes).findByRole("button", { name: "精靈" })
    const alert = await screen.findByRole("alert")
    expect(screen.queryByText("0 個版次")).not.toBeInTheDocument()
    const initialIndexRequests = indexRequests
    for (const [language, notice] of [
      ["zh-TW", "這些條件還不能套用：關鍵字"],
      ["ja", "まだ適用できない条件：キーワード"],
      ["en", "These conditions cannot be applied yet: Keywords"],
    ] as const) {
      act(() => {
        prefsStore.set({ uiLanguage: language })
      })
      await waitFor(() => expect(alert).toHaveTextContent(notice))
      expect(screen.getByRole("alert")).toBe(alert)
    }
    expect(indexRequests).toBe(initialIndexRequests)
  })
})
