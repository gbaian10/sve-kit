import { act, screen, waitFor, within } from "@testing-library/react"
import { afterEach, expect, it } from "vitest"

import {
  type AnnotatedTextResolver,
  type JsonObject,
  objectValue,
  type SelectedText,
  stringValue,
} from "../../data"
import { DEFAULT_PREFS, prefsStore } from "../../settings"
import { renderInRouter } from "../../test-utils"
import { annotatedOrigin } from "../../test-utils/annotated-snapshot"
import { CardText } from "./CardText"

afterEach(() => prefsStore.set(DEFAULT_PREFS))

const latest = { kind: "face_revision", id: "latest" }
const printed = { kind: "face_revision", id: "printed" }
const selected = (original: string, translated?: string): SelectedText => ({
  original: { unit: { text: original, lang: "ja" }, annotation: null },
  ...(translated === undefined
    ? {}
    : { translated: { unit: { text: translated, lang: "zh-Hant" }, annotation: null } }),
  concepts: [],
  explanations: [],
  cards: [],
})

// A controlled resolver lets the two revisions settle independently through the public boundary.
async function pairingCase(blocked?: {
  owner: "latest" | "printed"
  request: Promise<SelectedText | null>
}) {
  const { client } = await annotatedOrigin()
  await client.load()
  const texts = new Map<string, SelectedText>([
    [
      "latest:effect:null",
      {
        ...selected("Latest effect", "效果譯文"),
        originalSections: [{ ordinal: 1 }, { ordinal: 3 }],
      },
    ],
    [
      "printed:effect:null",
      { ...selected("Printed effect"), originalSections: [{ ordinal: 3 }, { ordinal: 5 }] },
    ],
    ["latest:section:1", selected("Latest first", "第一段譯文")],
    ["latest:section:3", selected("Latest third", "第三段譯文")],
    ["printed:section:3", selected("Printed third")],
    ["printed:section:5", selected("Printed fifth")],
  ])
  const keyOf = (pointer: JsonObject) => {
    const owner = objectValue(pointer["owner"])
    const ordinal = pointer["ordinal"]
    return `${stringValue(owner["id"])}:${stringValue(pointer["field"])}:${typeof ordinal === "number" ? String(ordinal) : "null"}`
  }
  const blockedKey = blocked ? `${blocked.owner}:section:3` : undefined
  const resolver: AnnotatedTextResolver = {
    peek: (pointer) =>
      keyOf(pointer) === blockedKey ? undefined : (texts.get(keyOf(pointer)) ?? null),
    resolve: async (pointer) =>
      keyOf(pointer) === blockedKey && blocked
        ? blocked.request
        : (texts.get(keyOf(pointer)) ?? null),
  }
  prefsStore.set({ effectLanguage: "both" })
  return { client, resolver, texts }
}

it("pairs printed sections by ordinal and keeps printed-only sections in the final original box", async () => {
  const { client, resolver } = await pairingCase()
  await renderInRouter(
    <CardText
      client={client}
      edition="jp"
      owner={latest}
      printed={printed}
      field="effect"
      sharedResolver={resolver}
    />,
  )
  const region = screen.getByRole("region", { name: "效果" })
  const first = within(region).getByText("第一段譯文").closest("[data-section]")
  const third = within(region).getByText("第三段譯文").closest("[data-section]")
  expect(first?.querySelector("[data-original]")).toBeNull()
  expect(third).toContainElement(within(region).getByText("Printed third"))
  const boxes = [...region.querySelectorAll<HTMLElement>("[data-original]")]
  expect(
    boxes.map((box) => [...box.querySelectorAll("p[lang]")].map((p) => p.textContent)),
  ).toEqual([["Printed effect"], ["Printed third"], ["Printed fifth"]])
  expect(within(region).queryByText("Latest first")).not.toBeInTheDocument()
  expect(within(region).queryByText("Latest third")).not.toBeInTheDocument()
  act(() => {
    prefsStore.set({ effectLanguage: "translated" })
  })
  expect(region.querySelector("[data-original]")).toBeNull()
  expect(within(region).getByText("第三段譯文")).toBeVisible()
  expect(within(region).queryByText("Printed third")).not.toBeInTheDocument()
  act(() => {
    prefsStore.set({ effectLanguage: "original" })
  })
  expect([...region.querySelectorAll("p[lang]")].map((p) => p.textContent)).toEqual([
    "Printed effect",
    "Printed third",
    "Printed fifth",
  ])
})

it("does not create an empty original effect box when printed wording only has sections", async () => {
  const { client, resolver, texts } = await pairingCase()
  const effect = texts.get("printed:effect:null")
  if (!effect) throw new Error("printed effect missing")
  texts.set("printed:effect:null", { ...effect, original: selected("").original })
  await renderInRouter(
    <CardText
      client={client}
      edition="jp"
      owner={latest}
      printed={printed}
      field="effect"
      sharedResolver={resolver}
    />,
  )
  const region = screen.getByRole("region", { name: "效果" })
  expect(region.querySelectorAll("[data-original]")).toHaveLength(2)
  expect(within(region).getByText("Printed third")).toBeVisible()
  expect(within(region).getByText("Printed fifth")).toBeVisible()
})

it.each(["latest", "printed"] as const)(
  "keeps the other revision visible while the %s section loads and after it fails",
  async (owner) => {
    let rejectRequest: ((reason: Error) => void) | undefined
    const request = new Promise<SelectedText | null>((_resolve, reject) => {
      rejectRequest = reject
    })
    // Requests start after paint; rejection may happen before the resolver subscribes.
    void request.catch(() => undefined)
    const { client, resolver } = await pairingCase({ owner, request })
    await renderInRouter(
      <CardText
        client={client}
        edition="jp"
        owner={latest}
        printed={printed}
        field="effect"
        sharedResolver={resolver}
      />,
    )
    const region = screen.getByRole("region", { name: "效果" })
    const otherText = owner === "printed" ? "第三段譯文" : "Printed third"
    expect(within(region).getByText(otherText)).toBeVisible()
    expect(within(region).getByRole("status")).toBeVisible()
    act(() => {
      rejectRequest?.(new Error("Synthetic section load failure"))
    })
    await waitFor(() => expect(within(region).getByRole("alert")).toBeVisible())
    expect(within(region).getByText(otherText)).toBeVisible()
    expect(within(region).getByText("Printed fifth")).toBeVisible()
  },
)
