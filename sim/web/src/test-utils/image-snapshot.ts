import type { ImageFace, LoadedSnapshot, SnapshotClient } from "../data"

type Fragment = LoadedSnapshot["bootstrap"][number]

/** Synthetic decoded media, so hook tests exercise the real image loader without official text. */
export function imageSnapshot(count: number) {
  const owner = { kind: "home_set", id: "set:test" }
  const faces: ImageFace[] = Array.from({ length: count }, (_, i) => ({
    printingId: `p:${String(i)}`,
    faceId: `f:${String(i)}`,
  }))
  const fragment = (table: string, rows: Fragment["rows"]): Fragment => ({
    file: "media",
    table,
    rows,
    identity: table,
    value: { owner },
  })
  const media = [
    fragment(
      "printing_image",
      faces.map(({ printingId, faceId }, i) => ({
        printing_id: printingId,
        face_id: faceId,
        availability: "available",
        publication_state: "approved",
        card_version: 1,
        variants: [{ size_key: "card_l", width: 320, height: 447 }],
        image_id: `img:${String(i)}`,
      })),
    ),
  ]
  const snapshot: LoadedSnapshot = {
    dataVersion: "synthetic",
    manifestHash: "synthetic",
    manifest: {},
    config: {},
    files: new Map([
      [
        "media",
        {
          bytes: count * 200,
          row_counts: Array.from({ length: 64 }, (_, bucket) => ({
            table: "printing_image",
            owner,
            bucket,
            partition: "detail",
            count,
          })),
        },
      ],
    ]),
    bootstrap: [
      fragment(
        "printing",
        faces.map(({ printingId }, i) => ({
          id: printingId,
          card_id: `c:${String(i)}`,
          int_id: i + 1,
        })),
      ),
      fragment(
        "face",
        faces.map(({ faceId }, i) => ({ id: faceId, card_id: `c:${String(i)}`, ordinal: 0 })),
      ),
    ],
  }
  const client: SnapshotClient = {
    base: "/cdn",
    snapshot: () => snapshot,
    fragments: () => Promise.resolve(media),
    status: () => ({ state: "ready", dataVersion: snapshot.dataVersion }),
    subscribe: () => () => undefined,
    load: () => Promise.resolve(),
    retry: () => Promise.resolve(),
    reload: () => Promise.resolve(),
    metadataStatus: () => ({ state: "idle", done: 0, total: 1, persistent: false }),
    prefetchImages: () => Promise.resolve(),
    cancelImagePrefetch: () => undefined,
  }
  return { client, snapshot, faces, media }
}
