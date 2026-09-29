import { act, screen } from "@testing-library/react"
import userEvent from "@testing-library/user-event"
import { useNavigate } from "react-router"
import { describe, expect, it } from "vitest"

import { renderRoutes } from "../test-utils"
import { useListEntryState } from "./listEntryState"

function List() {
  const [state, update] = useListEntryState()
  const navigate = useNavigate()
  return (
    <div>
      <output>{JSON.stringify(state)}</output>
      <button
        type="button"
        onClick={() => {
          update({ pages: 2 })
        }}
      >
        more
      </button>
      <button
        type="button"
        onClick={() => {
          // The list's anchor is written first, then the card entry is pushed, in one event.
          update({ anchor: "c:1" })
          void navigate("/card", { state: { background: "?q=x" } })
        }}
      >
        open
      </button>
    </div>
  )
}

const routes = [
  { path: "/", Component: List },
  { path: "/card", element: <p>card</p> },
]

describe("useListEntryState", () => {
  it("writes through the router and keeps react-router's own history fields", async () => {
    const { router } = await renderRoutes(routes, { initialEntries: ["/?q=x"] })
    await userEvent.click(screen.getByRole("button", { name: "more" }))
    expect(screen.getByRole("status")).toHaveTextContent('{"pages":2}')
    expect(router.state.location.search).toBe("?q=x")
    expect(router.state.location.state).toEqual({ pages: 2 })
    // react-router keeps its own key on the location; a raw replaceState would have dropped it.
    expect(router.state.location.key).toBeTruthy()
  })

  it("survives replace-then-push in one event, so back finds the anchor", async () => {
    const { router } = await renderRoutes(routes, { initialEntries: ["/?q=x"] })
    await userEvent.click(screen.getByRole("button", { name: "more" }))
    await userEvent.click(screen.getByRole("button", { name: "open" }))
    expect(router.state.location.pathname).toBe("/card")
    expect(router.state.location.state).toEqual({ background: "?q=x" })
    await act(async () => {
      await router.navigate(-1)
    })
    expect(router.state.location.search).toBe("?q=x")
    expect(router.state.location.state).toEqual({ pages: 2, anchor: "c:1" })
    expect(screen.getByRole("status")).toHaveTextContent('{"pages":2,"anchor":"c:1"}')
  })
})
