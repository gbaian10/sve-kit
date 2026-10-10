import { act, screen } from "@testing-library/react"
import userEvent from "@testing-library/user-event"
import { afterEach, describe, expect, it, vi } from "vitest"

import fixture from "../../../../../tests/fixtures/snapshot-contract/v3/english-native.json"
import { createAnnotatedTextResolver, objectValue } from "../../data"
import { DEFAULT_PREFS, prefsStore } from "../../settings"
import { renderInRouter } from "../../test-utils"
import { annotatedOrigin } from "../../test-utils/annotated-snapshot"
import { CardText } from "./CardText"

afterEach(() => prefsStore.set(DEFAULT_PREFS))

// The face's effect is translated; its name has no selected translation, which is
// the complete-EN fallback case of the same exact owner.
async function englishOrigin() {
  const origin = await annotatedOrigin(fixture)
  return { ...origin, owner: objectValue(fixture["owner"]) }
}

describe("English whole-field display exceptions", () => {
  it("reads a translated and an untranslated field of one English owner from the Python-produced snapshot", async () => {
    const { client, owner } = await englishOrigin()
    await client.load()
    const resolver = createAnnotatedTextResolver(client)
    const translated = await resolver.resolve(
      { owner: owner, field: "effect", ordinal: null },
      "zh-Hant",
    )
    expect(translated?.original.unit["text"]).toBe("Rule.")
    expect(translated?.original.unit["lang"]).toBe("en")
    expect(translated?.original.annotation).toBeNull()
    expect(translated?.translated?.unit["text"]).toBe("合成規則😀。")
    expect(translated?.translated?.annotation).toBeNull()
    expect(translated?.selection?.["basis"]).toBe("own_source")
    expect(translated?.translation?.["low_confidence"]).toBe(true)
    expect(translated?.translation?.["authority"]).toBe("unofficial")
    const missing = await resolver.resolve(
      { owner: owner, field: "name", ordinal: null },
      "zh-Hant",
    )
    expect(missing?.original.unit["text"]).toBe("Reskin")
    expect(missing?.original.unit["lang"]).toBe("en")
    expect(missing?.original.annotation).toBeNull()
    expect(missing?.translated).toBeUndefined()
    expect(missing?.selection).toBeUndefined()
  })

  it("shows pending proofreading and copies the complete exception without EN emphasis", async () => {
    const { client, owner } = await englishOrigin()
    await client.load()
    const copy = vi.fn().mockResolvedValue(undefined)
    Object.defineProperty(navigator, "clipboard", {
      configurable: true,
      value: { writeText: copy },
    })
    prefsStore.set({ effectLanguage: "both" })
    const { container } = await renderInRouter(
      <CardText client={client} owner={owner} field="effect" />,
    )
    expect(await screen.findByText("合成規則😀。")).toBeVisible()
    expect(screen.getByText("Rule.")).toBeVisible()
    expect(screen.getByText(/待校對/)).toBeVisible()
    expect(screen.getByText(/機器譯文/)).toHaveTextContent("非官方")
    expect(container.querySelector("strong")).toBeNull()
    await userEvent.click(screen.getByRole("button", { name: "複製文字" }))
    expect(copy).toHaveBeenLastCalledWith("合成規則😀。")
    act(() => {
      prefsStore.set({ termEmphasis: false })
    })
    expect(screen.getByText("Rule.")).toBeVisible()
    expect(screen.getByText("合成規則😀。")).toBeVisible()
  })

  it("shows the complete EN fallback and missing label for an untranslated English field", async () => {
    const { client, owner } = await englishOrigin()
    await client.load()
    const copy = vi.fn().mockResolvedValue(undefined)
    Object.defineProperty(navigator, "clipboard", {
      configurable: true,
      value: { writeText: copy },
    })
    const { container } = await renderInRouter(
      <CardText client={client} owner={owner} field="name" />,
    )
    expect(await screen.findByText("Reskin")).toBeVisible()
    expect(screen.getByText("缺少此語言譯文")).toBeVisible()
    expect(container.querySelector("strong")).toBeNull()
    await userEvent.click(screen.getByRole("button", { name: "複製文字" }))
    expect(copy).toHaveBeenLastCalledWith("Reskin")
  })
})
