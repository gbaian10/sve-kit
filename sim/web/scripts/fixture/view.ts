import { decodeMessage } from "../../src/data/format-v3/decode-message"
import { type JsonObject, stringValue } from "../../src/data/format-v3/json"
import { findBase, type Fragment, joinDetail, type View } from "../../src/data/format-v3/reader"

/** Assemble fixture rows through the same decoding and detail joins as the browser. */
export function snapshotView(
  manifest: JsonObject,
  payloads: ReadonlyMap<string, Uint8Array>,
): View {
  const fragments: Fragment[] = []
  for (const file of manifest["files"] as JsonObject[]) {
    if (["config", "programs"].includes(stringValue(file["role"]))) continue
    const bytes = payloads.get(stringValue(file["key"]))
    if (!bytes) throw new Error("missing fixture payload")
    const decoded = decodeMessage({
      kind: "file",
      bytes,
      file,
      version: stringValue(manifest["format_version"]),
    })
    if (decoded.kind !== "fragments") throw new Error("expected fixture rows")
    fragments.push(...decoded.fragments)
  }
  const faces = new Map(
    fragments
      .filter((fragment) => fragment.table === "face")
      .flatMap((fragment) => fragment.rows.map((row) => [stringValue(row["id"]), row] as const)),
  )
  const bases = new Set(
    fragments
      .filter((fragment) => fragment.value["base"] !== null)
      .map((fragment) => findBase(fragment, fragments)),
  )
  const view: View = {}
  for (const fragment of fragments) {
    if (bases.has(fragment)) continue
    const rows =
      fragment.value["base"] === null
        ? fragment.rows
        : joinDetail(fragment, findBase(fragment, fragments), faces)
    ;(view[fragment.table] ??= []).push(...rows)
  }
  return view
}
