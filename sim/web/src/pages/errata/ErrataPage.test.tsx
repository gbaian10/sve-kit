import { screen, waitFor } from "@testing-library/react"
import { afterAll, afterEach, beforeEach, expect, it, vi } from "vitest"

import { routes } from "../../app/routes"
import { DEFAULT_PREFS, prefsStore } from "../../settings"
import { renderRoutes } from "../../test-utils"
import { stubFixtureFetch } from "../../test-utils/fixture-fetch"
import { TestSearchWorker } from "../../test-utils/search-worker"

await stubFixtureFetch()
beforeEach(() => {
  vi.stubGlobal("Worker", TestSearchWorker)
  prefsStore.set({ uiLanguage: "zh-TW" })
})
afterEach(() => prefsStore.set(DEFAULT_PREFS))
afterAll(() => vi.unstubAllGlobals())

it("shows each errata card once using its basic printing", async () => {
  await renderRoutes(routes, { initialEntries: ["/errata"] })
  await waitFor(
    () => {
      expect(screen.getAllByRole("article")).toHaveLength(2)
    },
    {
      timeout: 4000,
    },
  )
  expect(screen.getByRole("link", { name: /^BP01-021[^S]/u })).toHaveAttribute(
    "href",
    "/cards/BP01-021",
  )
  expect(screen.queryByRole("link", { name: /BP01-021S/u })).not.toBeInTheDocument()
})

it("does not carry Japanese errata into the English edition", async () => {
  prefsStore.set({ cardEdition: "en" })
  await renderRoutes(routes, { initialEntries: ["/errata"] })
  await screen.findByText("目前快照沒有勘誤卡。", {}, { timeout: 4000 })
  expect(screen.queryByRole("article")).not.toBeInTheDocument()
})
