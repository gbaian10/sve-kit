import { prepareImage } from "../components/card/image-loading"
import {
  type CardRouteInput,
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

/** Resolve basic text before the page commit; preloads yield between preparation steps. */
export async function prepareCardContent(
  client: SnapshotClient,
  located: LocatedCard,
  language: string,
  sizes: string,
  preloadSignal?: AbortSignal,
): Promise<CardContent> {
  if (preloadSignal) await yieldCardPreload(preloadSignal)
  const pinned = { ...client, snapshot: () => located.snapshot }
  const { printing } = located
  const snapshot = await loadFaceCandidates(
    pinned,
    located.snapshot,
    printing.cardId,
    printing.region,
  )
  if (preloadSignal) await yieldCardPreload(preloadSignal)
  const catalog = catalogOf(snapshot)
  if (preloadSignal) await yieldCardPreload(preloadSignal)
  // Candidate metadata extends the catalog; the resolver loads this card's text closure itself.
  const resolver = sharedAnnotatedTextResolver(pinned, printing.cardId)
  if (preloadSignal) await yieldCardPreload(preloadSignal)
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
    // A text that cannot load shows its own error after the commit; the card itself was located.
    Promise.all(
      [...pointers.values()].map(async (pointer) => {
        const value = await resolver.resolve(pointer, language).catch(() => null)
        await Promise.all(
          (value?.originalSections ?? []).map(({ ordinal }) =>
            resolver
              .resolve({ owner: pointer["owner"] ?? null, field: "section", ordinal }, language)
              .catch(() => null),
          ),
        )
      }),
    ),
  ])
  const prepareFace = ({ printingId, faceId }: { printingId: string; faceId: string }) => {
    const source = images.cardImage(printingId, faceId)
    return source ? prepareImage(source, sizes, preloadSignal) : Promise.resolve()
  }
  if (preloadSignal) {
    for (const face of faces) await prepareFace(face)
  } else await Promise.all(faces.map(prepareFace))
  preloadSignal?.throwIfAborted()
  return { ...located, catalog, client: pinned, images }
}

export function cardContentKey(
  client: SnapshotClient,
  input: CardRouteInput,
  language: string,
  attempt: number,
): string {
  return JSON.stringify([
    client.base,
    client.snapshot()?.manifestHash,
    input.edition,
    language,
    attempt,
    input.namespace,
    input.key,
    input.hint
      ? [
          input.hint.id,
          input.hint.cardId,
          input.hint.cardOwner,
          input.hint.printingOwner,
          input.hint.region,
        ]
      : null,
  ])
}

/** Cached fragments still need a task boundary so input can cancel the next expensive step. */
export async function yieldCardPreload(preloadSignal: AbortSignal): Promise<void> {
  await new Promise<void>((resolve) => {
    setTimeout(resolve, 0)
  })
  preloadSignal.throwIfAborted()
}
