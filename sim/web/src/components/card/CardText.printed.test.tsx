import { screen, within } from "@testing-library/react"
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

describe("the translation's labels without a source area", () => {
  it("keeps the jp_source label and never opens the Japanese source behind the translation", async () => {
    const { client } = await annotatedOrigin(fixture, "en")
    await client.load()
    prefsStore.set({ effectLanguage: "translated" })
    await renderInRouter(
      <CardText edition="en" client={client} owner={fixture.receiver.owner} field="effect" />,
    )
    const effect = await screen.findByRole("region", { name: "效果" })
    expect(within(effect).getByText(paragraph("Synthetic translated source effect"))).toBeVisible()
    expect(within(effect).getByText("依日文原文翻譯")).toBeVisible()
    expect(screen.queryByText("來源與依據")).not.toBeInTheDocument()
    expect(screen.queryByText("翻譯依據原文")).not.toBeInTheDocument()
    expect(document.querySelector("details")).toBeNull()
  })

  it("keeps the origin line of an official counterpart without naming its edition", async () => {
    const { client } = await annotatedOrigin(fixture, "jp")
    await client.load()
    prefsStore.set({ effectLanguage: "translated" })
    await renderInRouter(
      <CardText edition="jp" client={client} owner={fixture.counterpart.owner} field="effect" />,
      { language: "en" },
    )
    const effect = await screen.findByRole("region", { name: "Effect" })
    expect(within(effect).getByText(/Official translation/)).toHaveTextContent("SVE official")
    expect(screen.queryByText("Sources and basis")).not.toBeInTheDocument()
    expect(screen.queryByText(/Official counterpart/)).not.toBeInTheDocument()
  })
})

describe("a printing whose printed text differs from the latest wording", () => {
  // The fixture holds one English owner, so it stands in for its own printed text: the box
  // structure and the three modes are what these cases check.
  const printed = fixture.receiver.owner

  it("shows the latest wording's translation and this printing's own text in both mode", async () => {
    const { client } = await annotatedOrigin(fixture, "en")
    await client.load()
    prefsStore.set({ effectLanguage: "both" })
    await renderInRouter(
      <CardText
        edition="en"
        client={client}
        owner={fixture.receiver.owner}
        field="effect"
        printed={printed}
      />,
    )
    const effect = await screen.findByRole("region", { name: "效果" })
    expect(within(effect).getByText(paragraph("Synthetic translated source effect"))).toBeVisible()
    const original = effect.querySelector("[data-original]")
    expect(original).not.toBeNull()
    expect(
      within(original as HTMLElement).getByText(paragraph("Synthetic English counterpart effect")),
    ).toBeVisible()
    expect(screen.queryByText("來源與依據")).not.toBeInTheDocument()
  })

  it("shows only the translation in translated mode and only the printed text in original mode", async () => {
    // An official counterpart (not a jp_source translation): translated mode hides the original.
    const { client } = await annotatedOrigin(fixture, "jp")
    await client.load()
    const counterpart = fixture.counterpart.owner
    prefsStore.set({ effectLanguage: "translated" })
    const view = await renderInRouter(
      <CardText
        edition="jp"
        client={client}
        owner={fixture.counterpart.owner}
        field="effect"
        printed={counterpart}
      />,
      { language: "en" },
    )
    await screen.findByText(paragraph("Synthetic English counterpart effect"))
    expect(screen.queryByText("Original text")).not.toBeInTheDocument()
    view.unmount()
    prefsStore.set({ effectLanguage: "original" })
    await renderInRouter(
      <CardText
        edition="jp"
        client={client}
        owner={fixture.counterpart.owner}
        field="effect"
        printed={counterpart}
      />,
      { language: "en" },
    )
    expect(await screen.findByText(paragraph("Synthetic Japanese source effect"))).toBeVisible()
    expect(
      screen.queryByText(paragraph("Synthetic English counterpart effect")),
    ).not.toBeInTheDocument()
  })

  it("shows the owner's own original when nothing else is printed", async () => {
    const { client } = await annotatedOrigin(fixture, "en")
    await client.load()
    prefsStore.set({ effectLanguage: "both" })
    await renderInRouter(
      <CardText edition="en" client={client} owner={fixture.receiver.owner} field="effect" />,
    )
    const effect = await screen.findByRole("region", { name: "效果" })
    expect(
      within(effect).getByText(paragraph("Synthetic English counterpart effect")),
    ).toBeVisible()
    expect(
      within(effect).getAllByText(paragraph("Synthetic English counterpart effect")),
    ).toHaveLength(1)
  })
})
