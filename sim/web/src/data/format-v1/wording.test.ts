// @vitest-environment node
import { describe, expect, it } from "vitest"

import type { Row } from "./decode"
import { SnapshotError } from "./errors"
import { utf8 } from "./json"
import type { View } from "./reader"
import { validateView } from "./semantics"
import { digest } from "./sha256"
import { validateWording } from "./wording"

function pending(): View {
  return {
    face: [
      {
        id: "f:a",
        card_id: "c:a",
        current: [],
        wording: [
          {
            region: "jp",
            state: "pending",
            display: { revision_id: "r:new", basis: "latest_known_release" },
            candidates: [
              { printing_id: "p:a", revision_id: "r:old" },
              { printing_id: "p:b", revision_id: "r:new" },
            ],
            undated_printing_ids: [],
          },
        ],
      },
    ],
    face_revision: [
      { id: "r:old", face_id: "f:a", region: "jp", effect_unit_id: "t:old" },
      { id: "r:new", face_id: "f:a", region: "jp", effect_unit_id: "t:new" },
      { id: "r:other", face_id: "f:other", region: "jp" },
      { id: "r:en", face_id: "f:a", region: "en" },
    ],
    printing: [
      {
        id: "p:a",
        card_id: "c:a",
        region: "jp",
        faces: [
          {
            face_id: "f:a",
            observations: [
              { revision_id: "r:old", state: "available", source_url: "https://example.invalid/a" },
            ],
          },
        ],
      },
      {
        id: "p:b",
        card_id: "c:a",
        region: "jp",
        faces: [
          {
            face_id: "f:a",
            observations: [
              { revision_id: "r:new", state: "available", source_url: "https://example.invalid/b" },
            ],
          },
        ],
      },
    ],
    product: [
      { id: "prod:a", released_on: "2026-01-01", date_precision: "day" },
      { id: "prod:b", released_on: "2026-02-01", date_precision: "day" },
    ],
    printing_product: [
      {
        printing_id: "p:a",
        product_id: "prod:a",
        available_on: null,
        date_precision: null,
      },
      {
        printing_id: "p:b",
        product_id: "prod:b",
        available_on: null,
        date_precision: null,
      },
    ],
    card_engine_support: [
      { card_id: "c:a", region_blocks: [{ region: "jp", reasons: ["wording_pending"] }] },
    ],
  }
}
const row = (view: View, table: string, index = 0): Row => view[table]?.[index] ?? {}
const candidate = (view: View, index: number): Row => candidates(view)[index] ?? {}
const wording = (view: View): Row => (view["face"]?.[0]?.["wording"] as Row[])[0] ?? {}
const display = (view: View): Row => wording(view)["display"] as Row
const candidates = (view: View): Row[] => wording(view)["candidates"] as Row[]
const observation = (view: View, index: number): Row =>
  ((view["printing"]?.[index]?.["faces"] as Row[])[0]?.["observations"] as Row[])[0] ?? {}
function rejection(view: View): string {
  try {
    validateWording(view)
  } catch (error) {
    if (error instanceof SnapshotError) return error.code
    throw error
  }
  throw new Error("accepted")
}

describe("pending wording relationships", () => {
  it("accepts a latest provisional display and keeps current empty", () => {
    const view = pending()
    validateWording(view)
    expect(view["face"]?.[0]?.["current"]).toEqual([])
  })
  it("accepts legacy faces without a wording column", () => {
    const view = pending()
    delete view["face"]?.[0]?.["wording"]
    validateWording(view)
  })
  it.each(["unknown", "day"])(
    "uses the printing's own %s precision over product dates",
    (precision) => {
      const view = pending()
      row(view, "printing_product", 1)["date_precision"] = precision
      row(view, "printing_product", 1)["available_on"] = precision === "day" ? "2026-03-01" : null
      if (precision === "unknown") {
        wording(view)["undated_printing_ids"] = ["p:b"]
        display(view)["revision_id"] = "r:old"
      } else row(view, "product", 1)["date_precision"] = "unknown"
      validateWording(view)
    },
  )
  it("requires every inclusion date to be a complete day", () => {
    const view = pending()
    view["printing_product"]?.push({
      printing_id: "p:b",
      product_id: "prod:a",
      available_on: "2026-03",
      date_precision: "month",
    })
    wording(view)["undated_printing_ids"] = ["p:b"]
    display(view)["revision_id"] = "r:old"
    validateWording(view)
  })
  it("uses the earliest complete day across all inclusions of the same printing", () => {
    const view = pending()
    view["printing_product"]?.push({
      printing_id: "p:a",
      product_id: "prod:b",
      available_on: "2026-03-01",
      date_precision: "day",
    })
    validateWording(view)
  })
  it.each(["effect_unit_id", "sections"])(
    "rejects different %s on the same latest day",
    (field) => {
      const view = pending()
      row(view, "product")["released_on"] = "2026-02-01"
      row(view, "face_revision")["effect_unit_id"] = "t:same"
      row(view, "face_revision", 1)["effect_unit_id"] = "t:same"
      row(view, "face_revision")[field] =
        field === "sections" ? [{ text_unit_id: "t:old" }] : "t:old"
      row(view, "face_revision", 1)[field] =
        field === "sections" ? [{ text_unit_id: "t:new" }] : "t:new"
      expect(rejection(view)).toBe("wording-latest-candidate")
    },
  )
  it("rejects a null observation alongside an available observation on the latest day", () => {
    const view = pending()
    row(view, "product")["released_on"] = "2026-02-01"
    observation(view, 0)["revision_id"] = null
    candidate(view, 0)["revision_id"] = null
    expect(rejection(view)).toBe("wording-latest-candidate")
  })
  it("rejects an extra pending region while all physical regions already have current", () => {
    const view = pending()
    row(view, "face")["current"] = [{ region: "jp", revision_id: "r:old" }]
    wording(view)["region"] = "en"
    expect(rejection(view)).toBe("wording-region-coverage")
  })
  it.each([
    [
      "missing day",
      "wording-latest-date",
      (view: View) => {
        view["printing_product"] = []
        wording(view)["undated_printing_ids"] = ["p:a", "p:b"]
      },
    ],
    [
      "latest null",
      "wording-latest-candidate",
      (view: View) => {
        display(view)["revision_id"] = "r:old"
        observation(view, 1)["revision_id"] = null
        candidate(view, 1)["revision_id"] = null
      },
    ],
    [
      "old display",
      "wording-latest-candidate",
      (view: View) => {
        display(view)["revision_id"] = "r:old"
      },
    ],
    [
      "latest conflict",
      "wording-latest-candidate",
      (view: View) => {
        observation(view, 1)["state"] = "correction_conflict"
      },
    ],
    [
      "no own observation",
      "wording-candidate-observation",
      (view: View) => {
        candidate(view, 0)["revision_id"] = "r:new"
      },
    ],
    [
      "candidate wrong region",
      "wording-candidate-face",
      (view: View) => {
        row(view, "printing")["region"] = "en"
        row(view, "face")["current"] = [{ region: "en", revision_id: "r:en" }]
        observation(view, 0)["revision_id"] = "r:en"
        candidate(view, 0)["revision_id"] = "r:en"
      },
    ],
    [
      "observation wrong region",
      "wording-observation-face",
      (view: View) => {
        observation(view, 0)["revision_id"] = "r:en"
      },
    ],
    [
      "observation wrong face",
      "wording-observation-face",
      (view: View) => {
        observation(view, 0)["revision_id"] = "r:other"
      },
    ],
    [
      "missing block",
      "wording-region-block",
      (view: View) => {
        row(view, "card_engine_support")["region_blocks"] = []
      },
    ],
    [
      "missing pending",
      "wording-region-coverage",
      (view: View) => {
        row(view, "face")["wording"] = []
      },
    ],
    [
      "pending region without printing",
      "wording-region-coverage",
      (view: View) => {
        wording(view)["region"] = "en"
      },
    ],
    [
      "undated inventory",
      "wording-undated-inventory",
      (view: View) => {
        wording(view)["undated_printing_ids"] = ["p:a"]
      },
    ],
    [
      "current must remain",
      "wording-current-preserved",
      (view: View) => {
        row(view, "face")["current"] = [{ region: "jp", revision_id: "r:old" }]
      },
    ],
    [
      "current display matches",
      "wording-current-display",
      (view: View) => {
        row(view, "face")["current"] = [{ region: "jp", revision_id: "r:old" }]
        display(view)["basis"] = "current"
      },
    ],
    [
      "display outside candidates",
      "wording-display-candidate",
      (view: View) => {
        display(view)["revision_id"] = "r:other"
      },
    ],
    [
      "display wrong face",
      "wording-display-face",
      (view: View) => {
        row(view, "face")["current"] = [{ region: "jp", revision_id: "r:other" }]
        display(view)["basis"] = "current"
        display(view)["revision_id"] = "r:other"
      },
    ],
    [
      "display wrong region",
      "wording-display-face",
      (view: View) => {
        row(view, "face")["current"] = [{ region: "jp", revision_id: "r:en" }]
        display(view)["basis"] = "current"
        display(view)["revision_id"] = "r:en"
      },
    ],
  ] as const)("rejects %s independently", (_name, code, mutate) => {
    const view = pending()
    mutate(view)
    expect(rejection(view)).toBe(code)
  })
  it.each(["wording", "candidates", "observations"])("checks %s ordering", (field) => {
    const view = pending()
    display(view)["basis"] = "current"
    row(view, "face")["current"] = [{ region: "jp", revision_id: "r:new" }]
    if (field === "candidates") wording(view)[field] = candidates(view).reverse()
    if (field === "wording") row(view, "face")[field] = [wording(view), wording(view)]
    if (field === "observations") {
      const faces = row(view, "printing")["faces"] as Row[]
      ;(faces[0] ?? {})[field] = [observation(view, 0), observation(view, 0)]
    }
    const manifest = {
      qa_card_ids: [],
      errata_card_ids: [],
      mechanic_universe_id: digest(utf8("[]")),
    }
    view["card"] = [{ id: "c:a" }]
    expect(() => {
      validateView(view, manifest, [])
    }).toThrow("rows-unsorted-or-duplicate")
  })
})
