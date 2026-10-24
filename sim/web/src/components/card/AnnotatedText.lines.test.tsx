import { screen } from "@testing-library/react"
import { expect, it } from "vitest"

import type { SelectedText } from "../../data"
import { renderInRouter } from "../../test-utils"
import { annotatedOrigin } from "../../test-utils/annotated-snapshot"
import { AnnotatedText } from "./AnnotatedText"

it.each([
  ["single", 0, "single"],
  ["single", 1, ""],
  ["first\n\nlast\n", 0, "first"],
  ["first\n\nlast\n", 1, ""],
  ["first\n\nlast\n", 2, "last"],
  ["first\n\nlast\n", 3, ""],
  ["first\n\nlast\n", 4, ""],
  ["first\n\nlast\n", 5, ""],
  ["first\nlast", -1, ""],
  ["first\nlast", 0.5, ""],
])("shows only the requested line of %j at %s", async (text, line, expected) => {
  const value = { unit: { text, lang: "ja" }, annotation: null }
  const { client } = await annotatedOrigin()
  const { container } = await renderInRouter(
    <AnnotatedText
      value={value}
      context={{ original: value, concepts: [], explanations: [], cards: [] }}
      emphasis={true}
      client={client}
      edition="jp"
      line={line}
    />,
  )
  expect(container.querySelector("p")?.textContent).toBe(expected)
})

it("clips references and emphasis at line boundaries without shifting Unicode offsets or icons", async () => {
  const value = {
    unit: { text: "😀 before\n  Second {ファンファーレ} tail  \nlast", lang: "ja" },
    annotation: {
      occurrences: [
        {
          ordinal: 0,
          reference: { kind: "card_name", term_id: "card:example" },
          ranges: [{ start: 2, end: 17 }],
          bold: true,
        },
        {
          ordinal: 1,
          reference: { kind: "vocabulary", key: ["ability", "fanfare"] },
          ranges: [{ start: 19, end: 26 }],
          bold: true,
        },
        {
          ordinal: 2,
          reference: { kind: "vocabulary", key: ["zone", "hand"] },
          ranges: [{ start: 28, end: 39 }],
          bold: true,
        },
      ],
    },
  }
  const context: SelectedText = {
    original: value,
    concepts: [{ id: "card:example", category: "card_name", card_ids: ["c:example"] }],
    explanations: [],
    cards: [
      {
        cardId: "c:example",
        printingId: "p:example",
        faceId: "f:example",
        cardNo: "BP01-001",
        region: "jp",
        classCode: null,
        cost: null,
        attack: null,
        defense: null,
        name: { original: { lang: "ja", text: "Example" }, translations: {} },
      },
    ],
  }
  const { client } = await annotatedOrigin()
  const view = (line: number, annotationReady = true) => (
    <AnnotatedText
      value={value}
      context={context}
      emphasis={true}
      annotationReady={annotationReady}
      client={client}
      edition="jp"
      line={line}
    />
  )
  const rendered = await renderInRouter(view(1))
  expect(rendered.container.querySelector("p")?.textContent).toBe("  Second  tail  ")
  expect(screen.getByRole("link", { name: "Second" })).toHaveAttribute("href", "/cards/BP01-001")
  expect(screen.getByRole("link").querySelector("strong")?.textContent).toBe("  Second")
  expect(screen.getAllByRole("img", { name: "ファンファーレ" })).toHaveLength(1)
  expect(
    [...rendered.container.querySelectorAll("strong")].map((node) => node.textContent),
  ).toEqual(["  Second", "tail  "])
  rendered.rerender(view(2))
  expect(rendered.container.querySelector("p")?.textContent).toBe("last")
  expect(rendered.container.querySelector("strong")?.textContent).toBe("last")
  expect(screen.queryByRole("link")).not.toBeInTheDocument()
  expect(screen.queryByRole("img")).not.toBeInTheDocument()
  rendered.rerender(view(1, false))
  expect(rendered.container.querySelector("p")?.textContent).toBe("  Second  tail  ")
  expect(rendered.container.querySelector("strong")).toBeNull()
  expect(screen.queryByRole("link")).not.toBeInTheDocument()
  expect(screen.getAllByRole("img", { name: "ファンファーレ" })).toHaveLength(1)
})

it("keeps a marker spanning two source lines as clipped text instead of duplicating its icon", async () => {
  const value = { unit: { text: "{icon_fanfare|first\nsecond}", lang: "en" }, annotation: null }
  const { client } = await annotatedOrigin()
  const view = (line: number) => (
    <AnnotatedText
      value={value}
      context={{ original: value, concepts: [], explanations: [], cards: [] }}
      emphasis={true}
      client={client}
      edition="en"
      line={line}
    />
  )
  const rendered = await renderInRouter(view(0))
  expect(rendered.container.querySelector("p")?.textContent).toBe("{icon_fanfare|first")
  expect(screen.queryByRole("img")).not.toBeInTheDocument()
  rendered.rerender(view(1))
  expect(rendered.container.querySelector("p")?.textContent).toBe("second}")
  expect(screen.queryByRole("img")).not.toBeInTheDocument()
})
