import type { Row } from "./decode"
import { fail } from "./errors"
import { arrayValue, canonicalText, type JsonValue, objectValue, stringValue } from "./json"
import type { View } from "./reader"

type Physical = Map<string, { region: string; face: Row }>
const physicalKey = (printing: JsonValue | undefined, face: JsonValue | undefined): string =>
  canonicalText([printing ?? null, face ?? null])
const objects = (value: JsonValue | undefined): Row[] =>
  arrayValue(value ?? []).map((item) => objectValue(item))

function observations(view: View, revisions: Map<string, Row>): Physical {
  const physical: Physical = new Map()
  for (const printing of view["printing"] ?? []) {
    for (const face of objects(printing["faces"])) {
      physical.set(physicalKey(printing["id"], face["face_id"]), {
        region: stringValue(printing["region"]),
        face,
      })
      for (const observation of objects(face["observations"])) {
        if (observation["revision_id"] === null) continue
        const revision = revisions.get(stringValue(observation["revision_id"]))
        if (
          revision?.["face_id"] !== face["face_id"] ||
          revision?.["region"] !== printing["region"]
        )
          fail("wording-observation-face", "observation belongs to another face or region")
      }
    }
  }
  return physical
}

function printingDates(view: View): Map<string, string | null> {
  const products = new Map((view["product"] ?? []).map((row) => [stringValue(row["id"]), row]))
  const dates = new Map(
    (view["printing"] ?? []).map((row) => [stringValue(row["id"]), [] as (string | null)[]]),
  )
  for (const inclusion of view["printing_product"] ?? []) {
    const product = products.get(stringValue(inclusion["product_id"]))
    const inherits = inclusion["date_precision"] === null
    const precision = inherits ? product?.["date_precision"] : inclusion["date_precision"]
    const day = inherits ? product?.["released_on"] : inclusion["available_on"]
    dates
      .get(stringValue(inclusion["printing_id"]))
      ?.push(precision === "day" ? stringValue(day) : null)
  }
  return new Map(
    [...dates].map(([id, days]) => [
      id,
      days.length > 0 && days.every((day) => day !== null) ? ([...days].sort()[0] ?? null) : null,
    ]),
  )
}

const CONTENT_FIELDS = [
  "name_unit_id",
  "effect_unit_id",
  "class_code",
  "type_code",
  "cost",
  "attack",
  "defense",
  "traits",
  "titles",
  "special_kinds",
  "sections",
] as const
function content(revision: Row | undefined): string {
  return canonicalText(CONTENT_FIELDS.map((field) => revision?.[field] ?? null))
}

function candidates(
  face: Row,
  wording: Row,
  physical: Physical,
  dates: Map<string, string | null>,
  revisions: Map<string, Row>,
): void {
  const entries = objects(wording["candidates"])
  const printings = [...new Set(entries.map((item) => stringValue(item["printing_id"])))].sort()
  const undated = printings.filter((id) => dates.get(id) === null)
  if (canonicalText(wording["undated_printing_ids"] ?? null) !== canonicalText(undated))
    fail("wording-undated-inventory", "undated printing inventory disagrees with dates")
  for (const candidate of entries) {
    const parent = physical.get(physicalKey(candidate["printing_id"], face["id"]))
    if (!parent || parent.region !== wording["region"])
      fail("wording-candidate-face", "candidate belongs to another printing face or region")
    const observed = objects(parent.face["observations"]).map((item) => item["revision_id"])
    if (!(observed.length > 0 ? observed : [null]).includes(candidate["revision_id"]))
      fail("wording-candidate-observation", "candidate has no own printing observation")
  }
  const display = objectValue(wording["display"])
  if (display["basis"] !== "latest_known_release") return
  const known = printings
    .map((id) => dates.get(id))
    .filter((day): day is string => typeof day === "string")
    .sort()
  const latest = known.at(-1)
  if (!latest) fail("wording-latest-date", "latest display requires a complete release day")
  const newest = entries.filter((item) => dates.get(stringValue(item["printing_id"])) === latest)
  const contents = new Set(
    newest
      .filter((item) => item["revision_id"] !== null)
      .map((item) => content(revisions.get(stringValue(item["revision_id"])))),
  )
  if (
    contents.size !== 1 ||
    newest.some((item) => item["revision_id"] === null) ||
    newest.some((item) =>
      objects(
        physical.get(physicalKey(item["printing_id"], face["id"]))?.face["observations"],
      ).some((observation) => observation["state"] === "correction_conflict"),
    ) ||
    !newest.some((item) => item["revision_id"] === display["revision_id"])
  )
    fail("wording-latest-candidate", "latest display skips an unavailable or latest candidate")
}

function display(
  face: Row,
  wording: Row,
  revisions: Map<string, Row>,
  current: Map<string, JsonValue | undefined>,
  blocks: Map<string, JsonValue[]>,
): void {
  const region = stringValue(wording["region"])
  const entry = objectValue(wording["display"])
  if (entry["basis"] === "current") {
    if (entry["revision_id"] !== current.get(region))
      fail("wording-current-display", "pending display disagrees with current")
  } else {
    if (current.has(region))
      fail("wording-current-preserved", "pending display must preserve current")
    if (!blocks.get(region)?.includes("wording_pending"))
      fail("wording-region-block", "missing wording_pending region block")
    if (
      entry["revision_id"] !== null &&
      !objects(wording["candidates"]).some((item) => item["revision_id"] === entry["revision_id"])
    )
      fail("wording-display-candidate", "display is not a pending candidate")
  }
  if (entry["revision_id"] !== null) {
    const revision = revisions.get(stringValue(entry["revision_id"]))
    if (revision?.["face_id"] !== face["id"] || revision?.["region"] !== region)
      fail("wording-display-face", "display belongs to another face or region")
  }
}

/** The old candidate contract has no wording column; new descriptors require it via the schema. */
export function validateWording(view: View): void {
  if ((view["face"] ?? []).every((face) => !("wording" in face))) return
  const revisions = new Map(
    (view["face_revision"] ?? []).map((row) => [stringValue(row["id"]), row]),
  )
  const physical = observations(view, revisions)
  const dates = printingDates(view)
  const supports = new Map(
    (view["card_engine_support"] ?? []).map((row) => [stringValue(row["card_id"]), row]),
  )
  for (const face of view["face"] ?? []) {
    const current = new Map(
      objects(face["current"]).map((item) => [stringValue(item["region"]), item["revision_id"]]),
    )
    const pending = objects(face["wording"])
    const required = new Set(
      [...physical.values()]
        .filter((item) => item.face["face_id"] === face["id"])
        .map((item) => item.region),
    )
    if (
      [...required].some(
        (region) => !current.has(region) && !pending.some((item) => item["region"] === region),
      ) ||
      pending.some((item) => !required.has(stringValue(item["region"])))
    )
      fail("wording-region-coverage", "every physical region requires current or pending")
    const blocks = new Map(
      objects(supports.get(stringValue(face["card_id"]))?.["region_blocks"]).map((item) => [
        stringValue(item["region"]),
        arrayValue(item["reasons"]),
      ]),
    )
    for (const item of pending) {
      display(face, item, revisions, current, blocks)
      candidates(face, item, physical, dates, revisions)
    }
  }
}
