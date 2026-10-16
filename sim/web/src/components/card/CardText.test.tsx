import { screen } from "@testing-library/react"
import userEvent from "@testing-library/user-event"
import { afterEach, describe, expect, it, vi } from "vitest"

import { createAnnotatedTextResolver, stringValue } from "../../data"
import { renderInRouter } from "../../test-utils"
import { annotatedOrigin } from "../../test-utils/annotated-snapshot"
import { CardText } from "./CardText"
import { plainText } from "./textSymbols"

const clipboardDescriptor = Object.getOwnPropertyDescriptor(navigator, "clipboard")

afterEach(() => {
  if (clipboardDescriptor) Object.defineProperty(navigator, "clipboard", clipboardDescriptor)
  else Reflect.deleteProperty(navigator, "clipboard")
})

async function resolvedText() {
  const { client } = await annotatedOrigin()
  await client.load()
  const resolver = createAnnotatedTextResolver(client)
  const owner = { kind: "face_revision", id: "revision" }
  const value = await resolver.resolve({ owner, field: "effect", ordinal: null }, "ja")
  if (!value) throw new Error("synthetic text missing")
  return { client, resolver, owner, value }
}

describe("CardText copy action", () => {
  it("copies the displayed synthetic text and announces success", async () => {
    const { client, resolver, owner, value } = await resolvedText()
    const writeText = vi.fn().mockResolvedValue(undefined)
    Object.defineProperty(navigator, "clipboard", { configurable: true, value: { writeText } })
    await renderInRouter(
      <CardText
        client={client}
        owner={owner}
        field="effect"
        edition="jp"
        sharedResolver={resolver}
        display="original"
      />,
      { language: "ja" },
    )

    await userEvent.click(await screen.findByRole("button", { name: "本文をコピー" }))

    expect(writeText).toHaveBeenCalledWith(plainText(stringValue(value.original.unit["text"])))
    expect(await screen.findByText("本文をコピーしました")).toBeVisible()
  })

  it("announces when the browser rejects copying synthetic text", async () => {
    const { client, resolver, owner } = await resolvedText()
    const writeText = vi.fn().mockRejectedValue(new Error("clipboard unavailable"))
    Object.defineProperty(navigator, "clipboard", { configurable: true, value: { writeText } })
    await renderInRouter(
      <CardText
        client={client}
        owner={owner}
        field="effect"
        edition="jp"
        sharedResolver={resolver}
        display="original"
      />,
      { language: "ja" },
    )

    await userEvent.click(await screen.findByRole("button", { name: "本文をコピー" }))

    expect(writeText).toHaveBeenCalledOnce()
    expect(
      await screen.findByText("本文をコピーできません。ブラウザーの権限を確認してください。"),
    ).toBeVisible()
  })
})
