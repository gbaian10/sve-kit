import { readFileSync } from "node:fs"
import path from "node:path"

import { screen } from "@testing-library/react"
import { beforeEach, describe, expect, it } from "vitest"

import { prefsStore } from "../../../settings"
import { renderRoutes } from "../../../test-utils"
import { CounterTokens } from "./Tokens"

beforeEach(() => {
  prefsStore.set({ uiLanguage: "zh-TW" })
})

/** The rules names of the keyword registry's counters: what the authority reports a counter as. */
function registryCounters(): string[] {
  const registry = readFileSync(
    path.join(import.meta.dirname, "../../../../../../authored/rules/keyword-registry.yaml"),
    "utf8",
  )
  // One entry per two-space key; a counter is an entry whose role says so.
  return registry
    .split(/^ {2}(?=\w+:\s*$)/m)
    .filter((entry) => /^ {4}role: counter\s*$/m.test(entry))
    .flatMap((entry) => /^ {4}ja: (.+?)\s*$/m.exec(entry)?.[1] ?? [])
}

describe("counter names", () => {
  it("names every counter the keyword registry lists", async () => {
    const names = registryCounters()
    // The registry is read by shape; an empty list would mean the shape changed, not that all is well.
    expect(names).toContain("スタックカウンター")
    await renderRoutes([
      {
        path: "*",
        element: (
          <CounterTokens
            card={{
              handle: "a1",
              card: { printing_id: "p:test", face: 0 },
              face: { printing_id: "p:test", face: 0 },
              faceDown: false,
              acted: false,
              evolution: "base",
              stats: null,
              keywords: [],
              counters: names.map((name) => ({ name, amount: 1 })),
              cost: null,
              attack: null,
            }}
          />
        ),
      },
    ])
    const chips = await screen.findAllByRole("img", { name: /^指示物：/ })
    expect(chips).toHaveLength(names.length)
    expect(screen.getByRole("img", { name: "指示物：童話カウンター ×1" })).toBeVisible()
    expect(new Set(chips.map((chip) => chip.getAttribute("title"))).size).toBe(names.length)
  })
})
