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

it("interleaves translation and original line by line, and keeps one pair when the line counts differ", async () => {
  const { client, resolver, texts } = await pairingCase()
  texts.set("latest:effect:null", selected("一行目\n二行目\n三行目", "第一行\n第二行\n第三行"))
  const view = await renderInRouter(
    <CardText
      client={client}
      edition="jp"
      owner={latest}
      field="effect"
      sharedResolver={resolver}
    />,
  )
  const region = screen.getByRole("region", { name: "效果" })
  const lines = [...region.querySelectorAll<HTMLElement>("[data-line]")]
  expect(
    lines.map((line) => [...line.querySelectorAll("p[lang]")].map((p) => p.textContent)),
  ).toEqual([
    ["第一行", "一行目"],
    ["第二行", "二行目"],
    ["第三行", "三行目"],
  ])
  expect(lines.every((line) => line.querySelectorAll("[data-original]").length === 1)).toBe(true)
  view.unmount()
  // Two translated lines against three original lines cannot be matched without guessing.
  texts.set("latest:effect:null", selected("一行目\n二行目\n三行目", "第一行\n第二行"))
  await renderInRouter(
    <CardText
      client={client}
      edition="jp"
      owner={latest}
      field="effect"
      sharedResolver={resolver}
    />,
  )
  const whole = screen.getByRole("region", { name: "效果" })
  expect(whole.querySelector("[data-line]")).toBeNull()
  expect(whole.querySelectorAll("[data-original]")).toHaveLength(1)
  expect([...whole.querySelectorAll("p[lang]")].map((p) => p.textContent)).toEqual([
    "第一行\n第二行",
    "一行目\n二行目\n三行目",
  ])
})

it.each([true, false])(
  "keeps missing translations whole in printed pairs (sections: %s)",
  async (section) => {
    const { client, resolver, texts } = await pairingCase()
    texts.set("latest:effect:null", {
      ...selected("Latest one\nLatest two"),
      ...(section ? { originalSections: [{ ordinal: 3 }] } : {}),
    })
    texts.set("printed:effect:null", {
      ...selected("Printed one\nPrinted two"),
      ...(section ? { originalSections: [{ ordinal: 3 }] } : {}),
    })
    texts.set("latest:section:3", selected("Latest section one\nLatest section two"))
    texts.set("printed:section:3", selected("Printed section one\nPrinted section two"))
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
    expect(region.querySelector("[data-line]")).toBeNull()
    expect([...region.querySelectorAll("p[lang]")].map((p) => p.textContent)).toEqual([
      "Latest one\nLatest two",
      "Printed one\nPrinted two",
      ...(section
        ? ["Latest section one\nLatest section two", "Printed section one\nPrinted section two"]
        : []),
    ])
  },
)

it.each([false, true])(
  "pairs section lines using the matching original (printed: %s)",
  async (borrowed) => {
    const { client, resolver, texts } = await pairingCase()
    texts.set("latest:section:3", selected("Latest A\nLatest B", "甲\n乙"))
    texts.set("printed:section:3", selected("Printed A\nPrinted B"))
    await renderInRouter(
      <CardText
        client={client}
        edition="jp"
        owner={latest}
        {...(borrowed ? { printed } : {})}
        field="effect"
        sharedResolver={resolver}
      />,
    )
    const region = screen.getByRole("region", { name: "效果" })
    const lines = [...region.querySelectorAll("[data-line]")]
    expect(
      lines.map((line) => [...line.querySelectorAll("p[lang]")].map((p) => p.textContent)),
    ).toEqual([
      ["甲", borrowed ? "Printed A" : "Latest A"],
      ["乙", borrowed ? "Printed B" : "Latest B"],
    ])
  },
)

it.each([false, true])(
  "falls back to whole printed effect and section when counts differ (mismatch: %s)",
  async (mismatch) => {
    const { client, resolver, texts } = await pairingCase()
    texts.set("latest:effect:null", {
      ...selected("Latest A\nLatest B", "譯文甲\n譯文乙"),
      originalSections: [{ ordinal: 3 }],
    })
    const printedEffect = mismatch ? "Printed A\nPrinted B\nPrinted C" : "Printed A\nPrinted B"
    const printedSection = mismatch ? "Section A\nSection B\nSection C" : "Section A\nSection B"
    texts.set("printed:effect:null", {
      ...selected(printedEffect),
      originalSections: [{ ordinal: 3 }],
    })
    texts.set("latest:section:3", selected("Latest section A\nLatest section B", "段落甲\n段落乙"))
    texts.set("printed:section:3", selected(printedSection))
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
    expect([...region.querySelectorAll("p[lang]")].map((p) => p.textContent)).toEqual(
      mismatch
        ? ["譯文甲\n譯文乙", printedEffect, "段落甲\n段落乙", printedSection]
        : [
            "譯文甲",
            "Printed A",
            "譯文乙",
            "Printed B",
            "段落甲",
            "Section A",
            "段落乙",
            "Section B",
          ],
    )
    expect(region.querySelectorAll("[data-line]")).toHaveLength(mismatch ? 0 : 4)
  },
)

it("keeps multi-line flavor in one original box", async () => {
  const { client, resolver, texts } = await pairingCase()
  texts.set("latest:flavor:null", selected("Original A\nOriginal B", "譯文甲\n譯文乙"))
  prefsStore.set({ flavorDisplay: "both" })
  await renderInRouter(
    <CardText
      client={client}
      edition="jp"
      owner={latest}
      field="flavor"
      sharedResolver={resolver}
    />,
  )
  const region = screen.getByRole("region", { name: "風味文字" })
  expect(region.querySelector("[data-line]")).toBeNull()
  expect(region.querySelectorAll("[data-original]")).toHaveLength(1)
  expect([...region.querySelectorAll("p[lang]")].map((p) => p.textContent)).toEqual([
    "譯文甲\n譯文乙",
    "Original A\nOriginal B",
  ])
})

it("interleaves jp_source lines in translated mode", async () => {
  const { client, resolver, texts } = await pairingCase()
  texts.set("latest:effect:null", {
    ...selected("JP one\nJP two", "譯文甲\n譯文乙"),
    selection: { basis: "jp_source" },
  })
  prefsStore.set({ effectLanguage: "translated" })
  await renderInRouter(
    <CardText
      client={client}
      edition="en"
      owner={latest}
      field="effect"
      sharedResolver={resolver}
    />,
  )
  const region = screen.getByRole("region", { name: "效果" })
  expect(
    [...region.querySelectorAll("[data-line]")].map((line) =>
      [...line.querySelectorAll("p[lang]")].map((p) => p.textContent),
    ),
  ).toEqual([
    ["譯文甲", "JP one"],
    ["譯文乙", "JP two"],
  ])
})
