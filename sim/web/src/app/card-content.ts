import { prepareImage } from "../components/card/image-loading"
import {
  catalogOf,
  faceBasics,
  type ImageIndex,
  type JsonObject,
  loadFaceCandidates,
  loadImagePage,
  type LocatedCard,
  type Row,
  sharedAnnotatedTextResolver,
  type SnapshotClient,
  stringValue,
} from "../data"

export interface CardContent extends LocatedCard {
  readonly client: SnapshotClient
  readonly images: ImageIndex
}

/** Resolve the next page's basic text before replacing the page currently on screen. */
export async function prepareCardContent(
  client: SnapshotClient,
  located: LocatedCard,
  language: string,
  sizes: string,
): Promise<CardContent> {
  const initial = { ...client, snapshot: () => located.snapshot }
  const { printing } = located
  const snapshot = await loadFaceCandidates(
    initial,
    located.snapshot,
    printing.cardId,
    printing.region,
  )
  const pinned = { ...client, snapshot: () => snapshot }
  const catalog = catalogOf(snapshot)
  const resolver = sharedAnnotatedTextResolver(pinned)
  const pointers = new Map<string, JsonObject>()
  const add = (owner: JsonObject, field: string) => {
    const pointer = { owner, field, ordinal: null }
    pointers.set(JSON.stringify(pointer), pointer)
  }
  const faces = catalog.index.facesOf(printing.cardId).map((face) => {
    const faceId = stringValue(face["id"])
    const revision = catalog.index.displayRevision(faceId, printing.region)
    const wording = catalog.index.wording(faceId, printing.region)
    const candidates = (wording?.["candidates"] ?? []) as Row[]
    const own = candidates.filter((candidate) => candidate["printing_id"] === printing.id)
    const nameId = faceBasics(catalog.index, faceId, printing.region, printing.id).revision?.["id"]
    if (typeof nameId === "string") add({ kind: "face_revision", id: nameId }, "name")
    const effects = new Set<string>()
    if (revision) effects.add(stringValue(revision["id"]))
    const candidateRevisions = new Set(candidates.map((candidate) => candidate["revision_id"]))
    const visible = revision
      ? own
      : candidates.filter(
          (candidate) => candidate["printing_id"] === printing.id || candidateRevisions.size === 1,
        )
    for (const candidate of visible) {
      if (typeof candidate["revision_id"] === "string") effects.add(candidate["revision_id"])
    }
    for (const id of effects) add({ kind: "face_revision", id }, "effect")
    const owner = { kind: "printing_face", id: printing.id, face_id: faceId }
    for (const field of ["name", "effect", "flavor"]) add(owner, field)
    return { printingId: printing.id, faceId }
  })
  const [images] = await Promise.all([
    loadImagePage(pinned, faces).catch((): ImageIndex => ({
      cardImage: () => undefined,
      asset: () => undefined,
      known: () => true,
      failed: true,
    })),
    Promise.all([...pointers.values()].map((pointer) => resolver.resolve(pointer, language))),
  ])
  await Promise.all(
    faces.map(({ printingId, faceId }) => {
      const source = images.cardImage(printingId, faceId)
      return source ? prepareImage(source, sizes) : Promise.resolve()
    }),
  )
  return { ...located, snapshot, catalog, client: pinned, images }
}
