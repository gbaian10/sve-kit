import { renderHook } from "@testing-library/react"
import type { ReactNode } from "react"
import { type InitialEntry, MemoryRouter } from "react-router"
import { afterEach, expect, it } from "vitest"

import { DEFAULT_PREFS, prefsStore } from "../settings"
import { useRouteEdition } from "./snapshot"

afterEach(() => prefsStore.set(DEFAULT_PREFS))

it.each<[InitialEntry, string]>([
  ["/cards", "jp"],
  ["/sets/BP01?ed=en", "en"],
  ["/cards/BP01EN-002/Queen", "en"],
  ["/cards/BP01-002/Queen", "jp"],
  ["/cards/_provisional/99999", "jp"],
  [{ pathname: "/cards/BP01-002", state: { route: { region: "en" } } }, "en"],
  [{ pathname: "/cards/BP01-002", search: "?ed=jp", state: { route: { region: "en" } } }, "jp"],
])("selects the first catalog from the route %j", (entry, edition) => {
  prefsStore.set({ cardEdition: "en" })
  const { result } = renderHook(() => useRouteEdition(), {
    wrapper: ({ children }: { children: ReactNode }) => (
      <MemoryRouter initialEntries={[entry]}>{children}</MemoryRouter>
    ),
  })
  expect(result.current).toBe(edition)
})
