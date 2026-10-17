import { act, screen, within } from "@testing-library/react"
import userEvent from "@testing-library/user-event"
import { afterEach, describe, expect, it } from "vitest"

import fixture from "../../../../../tests/fixtures/snapshot-contract/v3/english-native.json"
import { createAnnotatedTextResolver, objectValue, type Row } from "../../data"
import { DEFAULT_PREFS, prefsStore, readPrefs } from "../../settings"
import { renderInRouter } from "../../test-utils"
import { annotatedOrigin } from "../../test-utils/annotated-snapshot"
import { CardText } from "./CardText"

afterEach(() => prefsStore.set(DEFAULT_PREFS))

// The face's effect is translated; its name has no selected translation, which is
// the complete-EN fallback case of the same exact owner.
async function englishOrigin() {
  const origin = await annotatedOrigin(fixture)
  origin.client.setRegion?.("en")
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

  it("shows pending proofreading and the complete exception without EN emphasis", async () => {
    const { client, owner } = await englishOrigin()
    await client.load()
    prefsStore.set({ effectLanguage: "both" })
    const { container } = await renderInRouter(
      <CardText edition="en" client={client} owner={owner} field="effect" />,
    )
    expect(await screen.findByText("合成規則😀。")).toBeVisible()
    expect(screen.getByText("Rule.")).toBeVisible()
    expect(screen.getByText(/待校對/)).toBeVisible()
    expect(screen.getByText(/機器譯文/)).toHaveTextContent("非官方")
    const effect = screen.getByRole("region", { name: "效果" })
    const original = screen.getByText("Rule.").closest(".rounded-block")
    expect(effect).not.toHaveClass("rounded-block")
    expect(original).toContainElement(screen.getByText("原文"))
    expect(original).not.toContainElement(screen.getByText("合成規則😀。"))
    expect(container.querySelectorAll(".rounded-block")).toHaveLength(2)
    const translation = screen.getByText("合成規則😀。").closest(".rounded-block")
    expect(translation).toContainElement(screen.getByText(/機器譯文/))
    expect(translation?.querySelector("button")).toBeNull()
    expect(container.querySelector("strong")).toBeNull()
    act(() => {
      prefsStore.set({ termEmphasis: false })
    })
    expect(screen.getByText("Rule.")).toBeVisible()
    expect(screen.getByText("合成規則😀。")).toBeVisible()
  })

  it("shows the complete EN fallback and missing label for an untranslated English field", async () => {
    const { client, owner } = await englishOrigin()
    await client.load()
    const { container } = await renderInRouter(
      <CardText edition="en" client={client} owner={owner} field="name" />,
    )
    expect(await screen.findByText("Reskin")).toBeVisible()
    expect(screen.getByText("缺少此語言譯文")).toBeVisible()
    expect(container.querySelector("strong")).toBeNull()
    expect(screen.getByRole("region", { name: "名稱" })).not.toHaveClass("rounded-block")
  })
})

it.each(["translated", "original"] as const)(
  "groups a single-language effect and its source in one box (%s)",
  async (effectLanguage) => {
    const { client, owner } = await englishOrigin()
    await client.load()
    prefsStore.set({ effectLanguage })
    await renderInRouter(<CardText edition="en" client={client} owner={owner} field="effect" />)
    const text = effectLanguage === "translated" ? "合成規則😀。" : "Rule."
    await screen.findByText(text)
    const effect = screen.getByRole("region", { name: "效果" })
    expect(effect).toHaveClass("rounded-block", "border", "bg-surface-1")
    expect(within(effect).getByRole("heading", { name: "效果" })).toHaveClass("text-12")
    expect(within(effect).getByText(text)).toBeVisible()
    expect(effect.querySelector(".rounded-block")).toBeNull()
    expect(within(effect).queryByRole("button")).not.toBeInTheDocument()
    expect(within(effect).queryByText("原文")).not.toBeInTheDocument()
    if (effectLanguage === "translated") {
      expect(within(effect).getByText(/機器譯文/)).toHaveTextContent("非官方")
      expect(within(effect).queryByText("Rule.")).not.toBeInTheDocument()
    } else expect(within(effect).queryByText("合成規則😀。")).not.toBeInTheDocument()
  },
)

async function bilingualCard() {
  const { client } = await annotatedOrigin()
  await client.load()
  return {
    ...client,
    fragments: async (key: string) =>
      (await client.fragments(key)).map((fragment) => ({
        ...fragment,
        rows: fragment.rows.map((row) => {
          if (fragment.table === "face_revision")
            return {
              ...row,
              translations: (row["translations"] as Row[]).flatMap((translation) => [
                translation,
                {
                  ...translation,
                  field: "effect",
                  source: {
                    owner: { kind: "face_revision", id: "revision" },
                    field: "effect",
                    ordinal: null,
                  },
                },
              ]),
            }
          if (fragment.table !== "printing") return row
          return {
            ...row,
            faces: (row["faces"] as Row[]).map((face) => ({
              ...face,
              flavor_unit_id: (face["sections"] as Row[])[0]?.["text_unit_id"] ?? null,
              translations: [
                {
                  field: "flavor",
                  ordinal: null,
                  target_lang: "zh-Hant",
                  translation_id: "translation",
                  basis: "own_source",
                  source: {
                    owner: { kind: "printing_face", id: "printing", face_id: "face" },
                    field: "flavor",
                    ordinal: null,
                  },
                  counterpart: null,
                },
              ],
            })),
          }
        }),
      })),
  }
}

it("remembers a bilingual effect's original across cards and supports keyboard controls", async () => {
  const user = userEvent.setup()
  const { client, owner } = await englishOrigin()
  await client.load()
  prefsStore.set({ effectLanguage: "both" })
  const view = await renderInRouter(
    <CardText edition="en" client={client} owner={owner} field="effect" />,
  )
  const toggle = await screen.findByRole("button", { name: "收起原文" })
  const contentId = toggle.getAttribute("aria-controls") ?? ""
  expect(toggle).toHaveAttribute("aria-expanded", "true")
  expect(document.getElementById(contentId)).toContainElement(screen.getByText("Rule."))
  await user.tab()
  expect(toggle).toHaveFocus()
  await user.keyboard("{Enter}")
  expect(toggle).toHaveAttribute("aria-expanded", "false")
  expect(toggle).toHaveAccessibleName("展開原文")
  expect(screen.getByText("Rule.")).not.toBeVisible()
  expect(readPrefs().collapsedText.effectOriginal).toBe(true)
  view.unmount()
  prefsStore.reload()

  const next = await bilingualCard()
  await renderInRouter(
    <CardText
      edition="jp"
      client={next}
      owner={{ kind: "face_revision", id: "revision" }}
      field="effect"
    />,
  )
  const nextToggle = await screen.findByRole("button", { name: "展開原文" })
  expect(nextToggle).toHaveAttribute("aria-expanded", "false")
  const content = document.getElementById(nextToggle.getAttribute("aria-controls") ?? "")
  expect(content).not.toBeVisible()
  nextToggle.focus()
  await user.keyboard(" ")
  expect(nextToggle).toHaveAttribute("aria-expanded", "true")
  expect(content).toBeVisible()
})

it("boxes both translations and originals, collapses each original independently and restores defaults", async () => {
  const client = await bilingualCard()
  prefsStore.set({ effectLanguage: "both" })
  const view = await renderInRouter(
    <>
      <CardText
        edition="jp"
        client={client}
        owner={{ kind: "face_revision", id: "revision" }}
        field="effect"
      />
      <CardText
        edition="jp"
        client={client}
        owner={{ kind: "printing_face", id: "printing", face_id: "face" }}
        field="flavor"
      />
    </>,
  )
  await screen.findAllByRole("button", { name: "收起原文" })
  const effect = within(screen.getByRole("region", { name: "效果" }))
  const flavor = within(screen.getByRole("region", { name: "風味文字" }))
  const effectToggle = await effect.findByRole("button", { name: "收起原文" })
  const flavorToggle = await flavor.findByRole("button", { name: "收起原文" })
  expect(view.container.querySelectorAll(".rounded-block")).toHaveLength(4)
  expect(
    within(effect.getByRole("heading", { name: "效果" })).queryByRole("button"),
  ).not.toBeInTheDocument()
  expect(
    within(flavor.getByRole("heading", { name: "風味文字" })).queryByRole("button"),
  ).not.toBeInTheDocument()
  await userEvent.click(effectToggle)
  expect(effectToggle).toHaveAttribute("aria-expanded", "false")
  expect(flavorToggle).toHaveAttribute("aria-expanded", "true")
  expect(effect.getByRole("heading", { name: "效果" }).closest(".rounded-block")).toBeVisible()
  await userEvent.click(flavorToggle)
  await userEvent.click(effectToggle)
  expect(effectToggle).toHaveAttribute("aria-expanded", "true")
  expect(flavorToggle).toHaveAttribute("aria-expanded", "false")
  expect(readPrefs().collapsedText).toEqual({
    effectOriginal: false,
    flavorOriginal: true,
    sectionOriginal: false,
  })
  act(() => {
    prefsStore.set(DEFAULT_PREFS)
  })
  expect(effectToggle).toHaveAttribute("aria-expanded", "true")
  expect(flavorToggle).toHaveAttribute("aria-expanded", "true")
})
