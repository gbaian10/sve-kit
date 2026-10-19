import { arrayValue, type LoadedSnapshot, objectValue, type SnapshotClient } from "../data"

/**
 * The printed-text state of one face of a printing, read from the print-detail band of its home
 * set: the state is recorded even when the official page prints no text at all, so it cannot be
 * taken from a text unit that may not exist.
 */
export async function loadPrintedTextState(
  client: SnapshotClient,
  snapshot: LoadedSnapshot | null,
  printingId: string,
  faceId: string,
  homeSetId: string,
  region: "jp" | "en",
): Promise<string | undefined> {
  if (!snapshot) return undefined
  const keys = [...snapshot.files]
    .filter(
      ([, file]) =>
        file["purpose"] === "print-detail" &&
        file["region"] === region &&
        arrayValue(file["row_counts"]).some((raw) => {
          const count = arrayValue(raw)
          return count[0] === "printing" && count[1] === "home_set" && count[2] === homeSetId
        }),
    )
    .map(([key]) => key)
  const parts = (await Promise.all(keys.map((key) => client.fragments(key)))).flat()
  if (client.snapshot()?.manifestHash !== snapshot.manifestHash)
    throw new Error("snapshot replaced")
  const row = parts
    .filter((fragment) => fragment.table === "printing")
    .flatMap((fragment) => fragment.rows)
    .find((candidate) => candidate["id"] === printingId)
  const face = arrayValue(row?.["faces"] ?? [])
    .map((raw) => objectValue(raw))
    .find((entry) => entry["face_id"] === faceId)
  const state = face?.["printed_text_state"]
  return typeof state === "string" ? state : undefined
}
