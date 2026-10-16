import { catalogOf } from "./catalog"
import type { LoadedSnapshot, SnapshotClient } from "./client"
import type { Row } from "./format-v3/decode"
import { stringValue } from "./format-v3/json"
import { createLocator, homeSetOwner } from "./locator"
import type { CardIndex } from "./store"

/** Load the selected card's pending candidates, never the whole region's history. */
export async function loadFaceCandidates(
  client: SnapshotClient,
  snapshot: LoadedSnapshot,
  cardId: string,
  region: string,
): Promise<LoadedSnapshot> {
  const index = catalogOf(snapshot).index
  const pending = index.facesOf(cardId).some((face) => {
    const id = stringValue(face["id"])
    return index.wording(id, region) && !index.displayRevision(id, region)
  })
  if (!pending) return snapshot
  const card = index.card(cardId)
  const key = createLocator(snapshot.files)({
    table: "face_revision",
    owner: homeSetOwner(stringValue(card?.["home_set_id"])),
    entityKey: [cardId],
    region,
    partition: "history",
  })
  if (!key) return snapshot
  const parts = await client.fragments(key)
  if (client.snapshot()?.manifestHash !== snapshot.manifestHash)
    throw new Error("snapshot replaced")
  const present = new Set(snapshot.bootstrap.map((part) => part.identity))
  return {
    ...snapshot,
    bootstrap: [...snapshot.bootstrap, ...parts.filter((part) => !present.has(part.identity))],
  }
}

/** Presentation values only; candidate agreement never establishes current wording. */
export function faceBasics(
  index: CardIndex,
  faceId: string,
  region: string,
  printingId: string,
): { readonly values: Row; readonly revision: Row | undefined } {
  const display = index.displayRevision(faceId, region)
  if (display) return { values: display, revision: display }
  const candidates = (index.wording(faceId, region)?.["candidates"] ?? []) as Row[]
  const revisions = candidates.map((candidate) =>
    typeof candidate["revision_id"] === "string"
      ? index.revision(candidate["revision_id"])
      : undefined,
  )
  const own = candidates.filter((candidate) => candidate["printing_id"] === printingId)
  const ownIds = new Set(own.map((candidate) => candidate["revision_id"]))
  const ownId = ownIds.size === 1 ? own[0]?.["revision_id"] : undefined
  const revision = typeof ownId === "string" ? index.revision(ownId) : undefined
  const values: Row = {}
  for (const field of ["class_code", "type_code", "traits", "cost", "attack", "defense"]) {
    const first = revisions[0]?.[field]
    const common =
      first !== undefined &&
      revisions.every(
        (row) => row !== undefined && JSON.stringify(row[field]) === JSON.stringify(first),
      )
    const value = common ? first : revision?.[field]
    if (value !== undefined) values[field] = value
  }
  return { values, revision }
}
