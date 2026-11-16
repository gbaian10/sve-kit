import { fireEvent, screen } from "@testing-library/react"
import { beforeEach, describe, expect, it } from "vitest"

import { prefsStore } from "../../../settings"
import { renderRoutes } from "../../../test-utils"
import { CardBack } from "./CardBack"

beforeEach(() => {
  prefsStore.set({ uiLanguage: "zh-TW" })
})

describe("CardBack", () => {
  it("shows the published back, and the drawn one everywhere once that fails to load", async () => {
    await renderRoutes([
      {
        path: "*",
        element: (
          <>
            <CardBack />
            <CardBack />
          </>
        ),
      },
    ])
    const published = await screen.findAllByRole("img", { name: "卡背" })
    expect(published.map((back) => back.tagName)).toEqual(["IMG", "IMG"])
    // The file sits beside the card images of the snapshot root; nothing of it is in the app.
    expect(published[0]).toHaveAttribute("src", expect.stringMatching(/\/ui\/card_back\.webp$/))
    const [first] = published
    if (first === undefined) throw new Error("no card back")
    fireEvent.error(first)
    expect(screen.getAllByRole("img", { name: "卡背" }).map((back) => back.tagName)).toEqual([
      "svg",
      "svg",
    ])
  })
})
