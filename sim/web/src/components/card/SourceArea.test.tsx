import { screen, within } from "@testing-library/react"
import userEvent from "@testing-library/user-event"
import { afterEach, describe, expect, it } from "vitest"

import fixture from "../../../../../tests/fixtures/snapshot-contract/v3/source-native.json"
import { DEFAULT_PREFS, prefsStore } from "../../settings"
import { renderInRouter } from "../../test-utils"
import { annotatedOrigin } from "../../test-utils/annotated-snapshot"
import { CardText } from "./CardText"

afterEach(() => prefsStore.set(DEFAULT_PREFS))

// Annotated texts are split into buttons; match whole paragraphs instead of text nodes.
const paragraph = (text: string) => (_content: string, node: Element | null) =>
  node?.tagName === "P" && node.textContent === text

describe("the translation's source area", () => {
  it("opens the exact Japanese source text behind a jp_source translation", async () => {
    const { client } = await annotatedOrigin(fixture, "en")
    await client.load()
    prefsStore.set({ effectLanguage: "translated" })
    await renderInRouter(
      <CardText edition="en" client={client} owner={fixture.receiver.owner} field="effect" />,
    )
    const effect = await screen.findByRole("region", { name: "效果" })
    expect(within(effect).getByText(paragraph("Synthetic translated source effect"))).toBeVisible()
    expect(within(effect).getByText("依日文原文翻譯")).toBeVisible()
    expect(
      within(effect).queryByText(paragraph("Synthetic Japanese source effect")),
    ).not.toBeVisible()
    await userEvent.click(within(effect).getByText("來源與依據"))
    expect(within(effect).getByText("翻譯依據原文")).toBeVisible()
    expect(within(effect).getByText(paragraph("Synthetic Japanese source effect"))).toBeVisible()
    expect(within(effect).queryByText(/官方對照/)).not.toBeInTheDocument()
  })

  it("names the official counterpart's edition instead of repeating its text", async () => {
    const { client } = await annotatedOrigin(fixture, "jp")
    await client.load()
    prefsStore.set({ effectLanguage: "translated" })
    await renderInRouter(
      <CardText edition="jp" client={client} owner={fixture.counterpart.owner} field="effect" />,
      { language: "en" },
    )
    const effect = await screen.findByRole("region", { name: "Effect" })
    expect(
      within(effect).getAllByText(paragraph("Synthetic English counterpart effect")),
    ).toHaveLength(1)
    expect(within(effect).getByText(/Official translation/)).toHaveTextContent("SVE official")
    await userEvent.click(within(effect).getByText("Sources and basis"))
    expect(within(effect).getByText("Official counterpart: English official text")).toBeVisible()
    expect(
      within(effect).getAllByText(paragraph("Synthetic English counterpart effect")),
    ).toHaveLength(1)
    expect(within(effect).queryByText(/Source text of the translation/)).not.toBeInTheDocument()
  })

  it("has no source area for an original-only view", async () => {
    const { client } = await annotatedOrigin(fixture, "en")
    await client.load()
    prefsStore.set({ effectLanguage: "original" })
    await renderInRouter(
      <CardText edition="en" client={client} owner={fixture.receiver.owner} field="effect" />,
    )
    const effect = await screen.findByRole("region", { name: "效果" })
    expect(
      within(effect).getByText(paragraph("Synthetic English counterpart effect")),
    ).toBeVisible()
    expect(within(effect).queryByText("來源與依據")).not.toBeInTheDocument()
  })
})
