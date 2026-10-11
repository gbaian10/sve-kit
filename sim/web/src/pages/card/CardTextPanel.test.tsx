import { screen, waitFor } from "@testing-library/react"
import userEvent from "@testing-library/user-event"
import { afterEach, describe, expect, it } from "vitest"

import {
  createAnnotatedTextResolver,
  type Row,
  sharedAnnotatedTextResolver,
  stringValue,
} from "../../data"
import { DEFAULT_PREFS, prefsStore } from "../../settings"
import { renderInRouter } from "../../test-utils"
import { annotatedOrigin } from "../../test-utils/annotated-snapshot"
import { CardTextPanel } from "./CardTextPanel"

afterEach(() => prefsStore.set(DEFAULT_PREFS))

async function originWithPrintedText(available: boolean, flavor = false) {
  const { client } = await annotatedOrigin()
  await client.load()
  const resolver = createAnnotatedTextResolver(client)
  const effect = await resolver.resolve(
    { owner: { kind: "face_revision", id: "revision" }, field: "effect", ordinal: null },
    "ja",
  )
  const snapshot = client.snapshot()
  const printing = snapshot?.bootstrap
    .filter((fragment) => fragment.table === "printing")
    .flatMap((fragment) => fragment.rows)[0]
  const face = (printing?.["faces"] as Row[])[0]
  if (!printing || !face || !effect) throw new Error("synthetic printing missing")
  return {
    printingId: stringValue(printing["id"]),
    faceId: stringValue(face["face_id"]),
    client: {
      ...client,
      fragments: async (key: string) =>
        (await client.fragments(key)).map((fragment) =>
          fragment.table === "printing"
            ? {
                ...fragment,
                rows: fragment.rows.map((row) => ({
                  ...row,
                  faces: (row["faces"] as Row[]).map((face) => ({
                    ...face,
                    printed_effect_unit_id: available
                      ? stringValue(effect.original.unit["id"])
                      : null,
                    printed_text_state: available ? "verified" : "unknown",
                    ...(flavor ? { flavor_unit_id: stringValue(effect.original.unit["id"]) } : {}),
                  })),
                })),
              }
            : fragment,
        ),
    },
  }
}

describe("printed-text availability", () => {
  it("keeps the switch hidden after verifying a printing has no printed text", async () => {
    const props = await originWithPrintedText(false)
    await renderInRouter(
      <CardTextPanel {...props} edition="jp">
        <p>Synthetic current wording</p>
      </CardTextPanel>,
    )
    await waitFor(() => {
      for (const field of ["name", "effect", "flavor"])
        expect(
          sharedAnnotatedTextResolver(props.client).peek(
            {
              owner: { kind: "printing_face", id: props.printingId, face_id: props.faceId },
              field,
              ordinal: null,
            },
            "zh-Hant",
          ),
        ).toBeNull()
    })
    expect(screen.queryByRole("radiogroup", { name: "卡文版本" })).not.toBeInTheDocument()
    expect(screen.getByText("Synthetic current wording")).toBeVisible()
    expect(screen.queryByText("此段文字尚未提供")).not.toBeInTheDocument()
  })
  it("shows a printing's flavor beside the current wording without the switch", async () => {
    const props = await originWithPrintedText(false, true)
    await renderInRouter(
      <CardTextPanel {...props} edition="jp">
        <p>Synthetic current wording</p>
      </CardTextPanel>,
    )
    expect(await screen.findByRole("region", { name: "風味文字" })).toHaveTextContent(
      "Synthetic text",
    )
    await waitFor(() => {
      for (const field of ["name", "effect"])
        expect(
          sharedAnnotatedTextResolver(props.client).peek(
            {
              owner: { kind: "printing_face", id: props.printingId, face_id: props.faceId },
              field,
              ordinal: null,
            },
            "zh-Hant",
          ),
        ).toBeNull()
    })
    expect(screen.getByText("Synthetic current wording")).toBeVisible()
    expect(screen.queryByRole("radiogroup", { name: "卡文版本" })).not.toBeInTheDocument()
  })
  it("offers a working switch when a verified printing actually has effect text", async () => {
    const props = await originWithPrintedText(true)
    await renderInRouter(
      <CardTextPanel {...props} edition="jp">
        <p>Synthetic current wording</p>
      </CardTextPanel>,
    )
    expect(await screen.findByRole("radiogroup", { name: "卡文版本" })).toBeVisible()
    await userEvent.click(screen.getByRole("radio", { name: "印刷卡文" }))
    expect(screen.queryByText("Synthetic current wording")).not.toBeInTheDocument()
    expect(screen.getByRole("region", { name: "效果" })).toBeVisible()
    expect(screen.queryByText("此段文字尚未提供")).not.toBeInTheDocument()
    await userEvent.click(screen.getByRole("radio", { name: "目前卡文" }))
    expect(screen.getByText("Synthetic current wording")).toBeVisible()
  })
})
