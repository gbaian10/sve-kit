import { screen } from "@testing-library/react"
import userEvent from "@testing-library/user-event"
import { afterEach, describe, expect, it } from "vitest"

import fixture from "../../../../../tests/fixtures/snapshot-contract/v3/whole-name-native.json"
import { createAnnotatedTextResolver } from "../../data"
import { DEFAULT_PREFS, prefsStore } from "../../settings"
import { renderInRouter } from "../../test-utils"
import { annotatedOrigin } from "../../test-utils/annotated-snapshot"
import { CardNameHeading } from "./CardNameHeading"

const owner = { kind: "face_revision", id: "revision" }
const fallback = {
  primary: { lang: "zh-Hant" as const, text: "Catalog stand-in" },
  missingTranslation: false,
}

afterEach(() => {
  prefsStore.set(DEFAULT_PREFS)
})

describe("CardNameHeading", () => {
  it("shows the catalog name until the exact owner resolves, then the annotated name with its origin", async () => {
    const { client } = await annotatedOrigin(fixture)
    await client.load()
    await renderInRouter(
      <CardNameHeading
        edition="jp"
        client={client}
        resolver={createAnnotatedTextResolver(client)}
        owner={owner}
        fallback={fallback}
      />,
    )
    expect(screen.getByRole("heading", { level: 2 })).toHaveTextContent("Catalog stand-in")
    const heading = await screen.findByRole("heading", { level: 2, name: "Synthetic translation" })
    expect(heading).toHaveAttribute("lang", "zh-Hant")
    expect(screen.queryByText("Catalog stand-in")).not.toBeInTheDocument()
    expect(screen.getByText(/專案譯文/)).toHaveTextContent("非官方")
    // The whole-name concept reference stays an operable explanation entry inside the heading.
    await userEvent.click(screen.getByRole("button", { name: "Synthetic translation" }))
    expect(screen.getByRole("complementary")).toBeInTheDocument()
  })

  it("adds the original line in `both` mode and keeps the original alone in `original` mode", async () => {
    const { client } = await annotatedOrigin(fixture)
    await client.load()
    prefsStore.set({ nameDisplay: "both" })
    const { unmount } = await renderInRouter(
      <CardNameHeading
        edition="jp"
        client={client}
        resolver={createAnnotatedTextResolver(client)}
        owner={owner}
        fallback={fallback}
      />,
    )
    await screen.findByRole("heading", { level: 2, name: "Synthetic text" })
    expect(screen.getByText("Synthetic translation").closest("p")).toHaveAttribute(
      "lang",
      "zh-Hant",
    )
    unmount()
    prefsStore.set({ nameDisplay: "original" })
    await renderInRouter(
      <CardNameHeading
        edition="jp"
        client={client}
        resolver={createAnnotatedTextResolver(client)}
        owner={owner}
        fallback={fallback}
      />,
    )
    await screen.findByRole("heading", { level: 2, name: "Synthetic text" })
    expect(screen.queryByText("Synthetic translation")).not.toBeInTheDocument()
    expect(screen.queryByText(/專案譯文/)).not.toBeInTheDocument()
  })
})
