// @vitest-environment node
import { describe, expect, it } from "vitest"

import { TextOwners } from "./text-owners"

const card = { id: "card", regions: [{ region: "jp", mapping_state: "confirmed" }] }
const face = { id: "face", card_id: "card" }

describe("TextOwners", () => {
  it("extends the index with rows that arrive later, using cards it already holds", () => {
    const owners = new TextOwners({
      card: [card],
      face: [face],
      face_revision: [{ id: "first", face_id: "face", region: "jp" }],
    })
    owners.include({ face_revision: [{ id: "later", face_id: "face", region: "jp" }] })
    expect(owners.get({ kind: "face_revision", id: "later" })).toMatchObject({ cardId: "card" })
    expect(owners.get({ kind: "face_revision", id: "first" })).toMatchObject({ cardId: "card" })
  })
})
