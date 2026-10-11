import { fail } from "./errors"
import { arrayValue, type JsonObject, type JsonValue, objectValue, stringValue } from "./json"
import type { Fragment, View } from "./reader"

function displayRevisions(view: View): Set<JsonValue> {
  return new Set<JsonValue>(
    (view["face"] ?? [])
      .flatMap((face) => [
        ...arrayValue(face["current"]).map((value) => objectValue(value)["revision_id"]),
        ...arrayValue(face["wording"]).map(
          (value) => objectValue(objectValue(value)["display"])["revision_id"],
        ),
        ...arrayValue(face["wording"]).flatMap((value) =>
          arrayValue(objectValue(value)["candidates"]).map(
            (candidate) => objectValue(candidate)["revision_id"],
          ),
        ),
      ])
      .filter((value) => value !== null && value !== undefined),
  )
}

export function annotationLocations(view: View, fragments: readonly Fragment[]) {
  const display = displayRevisions(view)
  const revisions = new Map(
    (view["face_revision"] ?? []).map((row) => [stringValue(row["id"]), row]),
  )
  const faces = new Map((view["face"] ?? []).map((row) => [stringValue(row["id"]), row]))
  const cards = new Map((view["card"] ?? []).map((row) => [stringValue(row["id"]), row]))
  const printingHomes = new Map(
    fragments
      .filter((f) => f.table === "printing" && f.value["base"] === null)
      .flatMap((f) =>
        f.rows.map((row) => [stringValue(row["id"]), objectValue(f.value["owner"])["id"]] as const),
      ),
  )
  return (owner: JsonObject, field: string, ordinal: JsonValue = null) => {
    const kind = stringValue(owner["kind"])
    let home: JsonValue = null
    let entity: JsonValue[] = [owner, field, ordinal]
    if (kind === "face_revision") {
      const revision = revisions.get(stringValue(owner["id"]))
      const face = faces.get(stringValue(revision?.["face_id"]))
      const card = cards.get(stringValue(face?.["card_id"]))
      if (!card) fail("public-annotation/owner", "missing annotation owner card")
      home = card["home_set_id"] ?? null
      entity = [card["id"] ?? null]
    } else if (kind === "printing_face") {
      if (!printingHomes.has(stringValue(owner["id"])))
        fail("public-annotation/owner", "missing printing placement")
      home = printingHomes.get(stringValue(owner["id"])) ?? null
      entity = [owner["id"] ?? null]
    }
    const partition =
      kind === "face_revision" && !display.has(owner["id"] ?? null)
        ? "history"
        : (kind === "face_revision" && field === "name") ||
            ["keyword", "vocabulary", "product", "product_family"].includes(kind)
          ? "bootstrap"
          : "detail"
    return {
      physical: { kind: home === null ? "global" : "home_set", id: home },
      entity,
      partition,
    } as const
  }
}
