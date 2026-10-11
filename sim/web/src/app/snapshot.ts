import { useEffect, useMemo, useRef, useState } from "react"

import {
  type AnnotatedTextResolver,
  arrayValue,
  canonicalText,
  type Catalog,
  catalogOf,
  type ImageFace,
  type ImageIndex,
  isSnapshotImageFace,
  type JsonObject,
  type LoadedSnapshot,
  loadImagePage,
  objectValue,
  parseJson,
  type QueryFailure,
  retainedImageIndex,
  type SearchPage,
  type SearchQuery,
  type SelectedText,
  sharedAnnotatedTextResolver,
  type SnapshotClient,
  snapshotClient,
  SnapshotError,
  type SnapshotRoot,
  type SnapshotStatus,
  stringValue,
  useSnapshotRoot,
  useSnapshotStatus,
} from "../data"
import { DEFAULT_QUERY, type QueryState } from "../domain/query/model"
import type { Region } from "../domain/search"

const FOCUS_CHECK_INTERVAL_MS = 60_000

/** The snapshot every page reads: loading starts with the shell and follows the active root. */
export function useActiveSnapshot(): {
  readonly client: SnapshotClient
  readonly status: SnapshotStatus
  readonly root: SnapshotRoot
} {
  const root = useSnapshotRoot()
  const client = snapshotClient(root)
  const status = useSnapshotStatus(client)
  useEffect(() => {
    void client.load()
    // Switching back to the window only checks for a new version now and then, not on every focus.
    let lastCheck = Date.now()
    const check = () => {
      lastCheck = Date.now()
      void client.reload()
    }
    const onFocus = () => {
      if (Date.now() - lastCheck >= FOCUS_CHECK_INTERVAL_MS) check()
    }
    window.addEventListener("online", check)
    window.addEventListener("focus", onFocus)
    return () => {
      window.removeEventListener("online", check)
      window.removeEventListener("focus", onFocus)
    }
  }, [client])
  return { client, status, root }
}

/** The catalog of the loaded snapshot, or null while nothing is loaded yet. */
export function useCatalog(): {
  readonly client: SnapshotClient
  readonly status: SnapshotStatus
  readonly catalog: Catalog | null
} {
  const { client, status } = useActiveSnapshot()
  const snapshot = client.snapshot()
  return { client, status, catalog: snapshot ? catalogOf(snapshot) : null }
}

// Keep started image batches across navigation; only active viewers may schedule more (#511).
const FACE_CAP = 60
const BATCH_SIZE = 12
const CONCURRENCY = 2
const COLLECT_DEBOUNCE_MS = 100
const RETRY_COOLDOWN_MS = 10_000
const BATCH_TIMEOUT_MS = 30_000

interface ImageStore {
  running: number
  readonly listeners: Set<() => void>
  readonly failed: Map<string, number>
  readonly inflight: Set<string>
}
const imageStores = new WeakMap<LoadedSnapshot, ImageStore>()
function imageStore(snapshot: LoadedSnapshot): ImageStore {
  let store = imageStores.get(snapshot)
  if (!store) {
    store = {
      failed: new Map(),
      inflight: new Set(),
      running: 0,
      listeners: new Set(),
    }
    imageStores.set(snapshot, store)
  }
  return store
}
const faceKeyOf = (printingId: string, faceId: string) => `${printingId}\u0000${faceId}`
const faceOfNode = (node: Element): ImageFace => ({
  printingId: node.getAttribute("data-image-printing") ?? "",
  faceId: node.getAttribute("data-image-face") ?? "",
})

/** Distance from the viewport in px; 0 while any part of the box is on screen. */
function viewportDistance(node: Element, viewportHeight: number): number {
  const rect = node.getBoundingClientRect()
  if (rect.bottom < 0) return -rect.bottom
  if (rect.top > viewportHeight) return rect.top - viewportHeight
  return 0
}

export function useImageIndex(client: SnapshotClient, ready: boolean): ImageIndex | undefined {
  const snapshot = client.snapshot()
  const [, setVersion] = useState(0)
  const [visible, setVisible] = useState<ReadonlySet<string>>(new Set())
  const shownRef = useRef(visible)
  useEffect(() => {
    if (!ready || !snapshot) return
    const store = imageStore(snapshot)
    const retained = retainedImageIndex(snapshot)
    let active = true
    const bump = () => {
      if (active) setVersion((value) => value + 1)
    }
    const near = new Set<Element>()
    const onScreen = new Set<Element>()
    let timer: ReturnType<typeof setTimeout> | undefined
    // Recomputed from the current range at every start, so a batch never picks up faces that
    // left the range while an earlier batch was in flight.
    const wanted = (): ImageFace[] => {
      const viewportHeight = window.innerHeight
      const seen = new Set<string>()
      return [...near]
        .filter((node) => node.isConnected)
        .map((node) => ({ node, distance: viewportDistance(node, viewportHeight) }))
        .sort((a, b) => a.distance - b.distance)
        .map(({ node }) => faceOfNode(node))
        .filter((face) => {
          if (!isSnapshotImageFace(snapshot, face)) return false
          const key = faceKeyOf(face.printingId, face.faceId)
          if (seen.has(key)) return false
          seen.add(key)
          return true
        })
        .slice(0, FACE_CAP)
    }
    const settle = (batch: readonly ImageFace[], failed: boolean) => {
      for (const face of batch) {
        const key = faceKeyOf(face.printingId, face.faceId)
        store.inflight.delete(key)
        if (failed) store.failed.set(key, Date.now())
      }
      store.running -= 1
      for (const listener of store.listeners) listener()
    }
    const pump = () => {
      if (!active) return
      while (store.running < CONCURRENCY) {
        const batch = wanted()
          .filter((face) => {
            const key = faceKeyOf(face.printingId, face.faceId)
            return (
              !retained.known?.(face.printingId, face.faceId) &&
              !store.inflight.has(key) &&
              !store.failed.has(key)
            )
          })
          .slice(0, BATCH_SIZE)
        if (batch.length === 0) return
        for (const face of batch) store.inflight.add(faceKeyOf(face.printingId, face.faceId))
        store.running += 1
        const abort = new AbortController()
        let settled = false
        const finish = (failed: boolean) => {
          if (settled) return
          settled = true
          clearTimeout(deadline)
          settle(batch, failed)
        }
        const deadline = setTimeout(() => {
          // Expired batches must not cache late results or release their shared slot twice.
          abort.abort()
          finish(true)
        }, BATCH_TIMEOUT_MS)
        void loadImagePage(client, batch, abort.signal).then(
          () => {
            finish(false)
          },
          () => {
            finish(true)
          },
        )
      }
    }
    const collect = () => {
      pump()
      if (!active) return
      const next = new Set(
        [...onScreen]
          .filter((node) => node.isConnected)
          .map((node) => {
            const face = faceOfNode(node)
            return faceKeyOf(face.printingId, face.faceId)
          }),
      )
      // An unchanged on-screen set must not re-render the whole result list on every DOM mutation.
      const shown = shownRef.current
      if (next.size === shown.size && [...next].every((key) => shown.has(key))) return
      shownRef.current = next
      setVisible(next)
    }
    const schedule = () => {
      clearTimeout(timer)
      timer = setTimeout(collect, COLLECT_DEBOUNCE_MS)
    }
    const track =
      (set: Set<Element>, retry = false, initial = new Set<Element>()) =>
      (entries: IntersectionObserverEntry[]) => {
        let retried = false
        for (const entry of entries) {
          const initializing = initial.delete(entry.target)
          if (entry.isIntersecting) {
            const entered = !set.has(entry.target)
            set.add(entry.target)
            if (retry && entered && !initializing) {
              const face = faceOfNode(entry.target)
              const key = faceKeyOf(face.printingId, face.faceId)
              const failedAt = store.failed.get(key)
              if (failedAt !== undefined && Date.now() - failedAt >= RETRY_COOLDOWN_MS) {
                store.failed.delete(key)
                retried = true
              }
            }
          } else set.delete(entry.target)
        }
        if (retried) for (const listener of store.listeners) listener()
        schedule()
      }
    const supported = typeof IntersectionObserver !== "undefined"
    let nearHeight = window.innerHeight
    // IntersectionObserver percentages use root width, so screen-height margins need pixels.
    const createNearObserver = (initial?: Set<Element>) => {
      nearHeight = window.innerHeight
      return supported
        ? new IntersectionObserver(track(near, true, initial), {
            rootMargin: `${String(nearHeight * 0.5)}px 0px ${String(nearHeight * 1.5)}px 0px`,
          })
        : undefined
    }
    let nearObserver = createNearObserver()
    const screenObserver = supported ? new IntersectionObserver(track(onScreen)) : undefined
    const observed = new Set<Element>()
    const scan = () => {
      for (const node of document.querySelectorAll("[data-image-face]")) {
        if (observed.has(node)) continue
        observed.add(node)
        if (nearObserver && screenObserver) {
          nearObserver.observe(node)
          screenObserver.observe(node)
        } else {
          near.add(node)
          onScreen.add(node)
        }
      }
      for (const node of observed)
        if (!node.isConnected) {
          nearObserver?.unobserve(node)
          screenObserver?.unobserve(node)
          observed.delete(node)
          near.delete(node)
          onScreen.delete(node)
        }
      schedule()
    }
    const resize = () => {
      // The margins depend on the height only; width-only resizes must not rebuild every target.
      if (window.innerHeight === nearHeight) return
      nearObserver?.disconnect()
      // Resize initialization is geometry reconciliation, not a scroll-triggered retry.
      nearObserver = createNearObserver(new Set(observed))
      for (const node of observed) nearObserver?.observe(node)
      if (!supported) for (const node of observed) near.add(node)
      schedule()
    }
    const updated = () => {
      bump()
      pump()
    }
    store.listeners.add(updated)
    window.addEventListener("resize", resize)
    const mutations = new MutationObserver(scan)
    mutations.observe(document.body, { childList: true, subtree: true })
    scan()
    return () => {
      active = false
      store.listeners.delete(updated)
      window.removeEventListener("resize", resize)
      clearTimeout(timer)
      nearObserver?.disconnect()
      screenObserver?.disconnect()
      mutations.disconnect()
    }
  }, [client, ready, snapshot])
  if (!ready || !snapshot) return undefined
  const store = imageStore(snapshot)
  const retained = retainedImageIndex(snapshot)
  return {
    ...retained,
    known: (printingId, faceId) =>
      retained.known?.(printingId, faceId) === true ||
      store.failed.has(faceKeyOf(printingId, faceId)),
    failedFace: (printingId, faceId) => store.failed.has(faceKeyOf(printingId, faceId)),
    eager: (printingId, faceId) => visible.has(faceKeyOf(printingId, faceId)),
    ...(store.failed.size > 0 ? { failed: true } : {}),
  }
}

// Keep Worker query shapes while V1 still reads the synchronous Catalog (see README search hooks).
type UnsupportedFacet = NonNullable<QueryFailure["facets"]>[number] | "cost" | "sets"

type StandInFailure = Omit<QueryFailure, "facets"> & {
  readonly facets?: readonly UnsupportedFacet[]
}

interface SearchLoadFailure {
  readonly kind: "load-failed" | "worker-failed" | "cancelled"
  readonly message: string
}

type SearchLoadStep = "index" | "manifest" | "bootstrap"

/** Readiness of the search generation: which root/edition can be queried, and what is loading. */
export interface SearchReadiness {
  readonly phase: "downloading" | "ready" | "error"
  /** Increments whenever a different snapshot object becomes queryable; null before the first. */
  readonly generation: number | null
  /** The root and edition the current generation answers for (kept during a replacement load). */
  readonly root?: SnapshotRoot
  readonly edition?: Region
  /**
   * A root/edition being downloaded; `step` is the load phase. While the startup files of a first
   * download arrive, `done` of `total` files have been received (no byte progress is reported).
   */
  readonly loading?: {
    readonly root: SnapshotRoot
    readonly edition: Region
    readonly step: SearchLoadStep
    readonly done?: number
    readonly total?: number
  }
  /**
   * True while a newer snapshot downloads behind a still-queryable generation. A background check
   * of the version entry that may find nothing new is not an update.
   */
  readonly updating: boolean
  readonly error?: SearchLoadFailure
  readonly dataVersion?: string
  readonly manifestHash?: string
  /** Retries a failed load, or re-reads the entry after a failed update. */
  readonly retry: () => void
}

type SearchResult =
  | { readonly phase: "pending" }
  | { readonly phase: "error"; readonly error: StandInFailure }
  | {
      readonly phase: "complete"
      readonly generation: number
      readonly root: SnapshotRoot
      readonly page: SearchPage
    }

/** One query channel: the last input it was given and the result for exactly that input. */
export interface SearchChannelState {
  readonly lastInput: SearchQuery | null
  readonly result: SearchResult
}

const generations = new WeakMap<LoadedSnapshot, number>()
let nextGeneration = 0

function generationOf(snapshot: LoadedSnapshot | null): number | null {
  if (!snapshot) return null
  let generation = generations.get(snapshot)
  if (generation === undefined) {
    nextGeneration += 1
    generation = nextGeneration
    generations.set(snapshot, generation)
  }
  return generation
}

function loadFailure(status: SnapshotStatus): SearchLoadFailure | undefined {
  const failure =
    status.state === "error" ? status : status.state === "ready" ? status.updateError : undefined
  return failure && { kind: "load-failed", message: `${failure.kind}: ${failure.detail}` }
}

/** The search generation for the active root and the current card edition. */
export function useSearchReadiness(edition: Region): SearchReadiness {
  const { client, status, root } = useActiveSnapshot()
  const snapshot = client.snapshot()
  const generation = generationOf(snapshot)
  const error = loadFailure(status)
  return useMemo<SearchReadiness>(() => {
    const retry = () => {
      void (status.state === "error" ? client.retry() : client.reload())
    }
    const base = {
      ...(snapshot
        ? { root, edition, dataVersion: snapshot.dataVersion, manifestHash: snapshot.manifestHash }
        : {}),
      ...(error ? { error } : {}),
      retry,
    }
    if (status.state === "error") return { ...base, phase: "error", generation, updating: false }
    if (status.state === "ready" && snapshot) {
      const updating = status.updating !== undefined && status.updating !== "index"
      return {
        ...base,
        phase: "ready",
        generation,
        updating,
        ...(updating ? { loading: { root, edition, step: status.updating } } : {}),
      }
    }
    const files =
      status.state === "loading" && status.total !== undefined && status.total > 0
        ? { done: status.done ?? 0, total: status.total }
        : {}
    return {
      ...base,
      phase: "downloading",
      generation,
      updating: false,
      loading: {
        root,
        edition,
        step: status.state === "loading" ? status.phase : "index",
        ...files,
      },
    }
  }, [client, status, snapshot, root, edition, generation, error])
}

const FACETS: readonly UnsupportedFacet[] = [
  "cost",
  "sets",
  "types",
  "rarities",
  "altArtOnly",
  "mechanics",
  "unit",
  "sort",
]

// Reject unsupported facets so an unfiltered synchronous result cannot look complete.
function unsupportedFacets(state: QueryState): UnsupportedFacet[] {
  return FACETS.filter((facet) => {
    switch (facet) {
      case "cost":
        return state.cost.min !== undefined || state.cost.max !== undefined
      case "mechanics":
        return Object.keys(state.mechanics).length > 0
      case "altArtOnly":
        return state.altArtOnly
      case "unit":
      case "sort":
        return state[facet] !== DEFAULT_QUERY[facet]
      default:
        return state[facet].length > 0
    }
  })
}

function runQuery(catalog: Catalog, query: SearchQuery): SearchPage | StandInFailure {
  const facets = unsupportedFacets(query.state)
  if (facets.length > 0)
    return { kind: "unsupported-query", message: "unsupported conditions", facets }
  const hits = catalog.results(query.state, query.edition)
  const offset = query.offset ?? 0
  const slice =
    query.limit === undefined ? hits.slice(offset) : hits.slice(offset, offset + query.limit)
  return {
    total: hits.length,
    offset,
    items: slice.map((hit) => ({ ...hit, summary: catalog.summary(hit.printingId) })),
  }
}

// Retaining input across downloads prevents partial counts and false empty results.
export function useSearch(query: SearchQuery | null): SearchChannelState {
  const { client, status, root } = useActiveSnapshot()
  const snapshot = client.snapshot()
  const generation = generationOf(snapshot)
  // Keep the pending input when a caller temporarily stops supplying it.
  const [kept, setKept] = useState<SearchQuery | null>(query)
  if (query !== null && query !== kept) setKept(query)
  const lastInput = query ?? kept
  const catalog = snapshot ? catalogOf(snapshot) : null
  return useMemo<SearchChannelState>(() => {
    if (lastInput === null) return { lastInput, result: { phase: "pending" } }
    if (status.state === "error")
      return {
        lastInput,
        result: {
          phase: "error",
          error: { kind: "load-unavailable", message: `${status.kind}: ${status.detail}` },
        },
      }
    if (!catalog || generation === null) return { lastInput, result: { phase: "pending" } }
    const outcome = runQuery(catalog, lastInput)
    return {
      lastInput,
      result:
        "kind" in outcome
          ? { phase: "error", error: outcome }
          : { phase: "complete", generation, root, page: outcome },
    }
  }, [lastInput, status, catalog, generation, root])
}

// Keep all purposes pending until the full V1 resolver settles (public-annotation §4.1).

export type TextField = "name" | "effect" | "flavor"

/** `none` = this purpose does not apply to the text (no JP source, no annotation set). */
type TextPurposePhase = "pending" | "ready" | "none" | "error"

interface TextFailure {
  /** `validation`: the snapshot rejected the closure (a `public-annotation/*` code); `load`: transport. */
  readonly kind: "validation" | "load"
  readonly code?: string
  readonly message: string
}

interface TextReadiness {
  readonly root: SnapshotRoot
  readonly generation: number | null
  readonly dataVersion?: string
  readonly manifestHash?: string
  readonly edition: Region
  readonly owner: JsonObject
  /** V1 only consumes whole name/effect/flavor fields, not section paragraphs. */
  readonly ordinal: null
  readonly field: TextField
  /** Target text language of the translation row (`zh-Hant`, `ja`, `en`). */
  readonly language: string
  /** Original and translated strings with origin/authority/low_confidence. */
  readonly basic: TextPurposePhase
  /** The JP owner a `jp_source` translation was made from. */
  readonly source: TextPurposePhase
  /** Ranges, concepts, vocabulary and explanations. */
  readonly annotations: TextPurposePhase
  readonly error?: TextFailure
}

export interface AnnotatedTextState {
  readonly readiness: TextReadiness
  /** Only set once `basic` is ready; null means the owner has no such field. */
  readonly value: SelectedText | null
}

function purposesOf(
  value: SelectedText | null,
): Pick<TextReadiness, "basic" | "source" | "annotations"> {
  if (value === null) return { basic: "none", source: "none", annotations: "none" }
  const annotated = [value.original, value.translated, value.source].some(
    (text) => text !== undefined && text.annotation !== null,
  )
  return {
    basic: "ready",
    source:
      value.selection?.["basis"] === "jp_source" ? (value.source ? "ready" : "error") : "none",
    annotations: annotated || value.concepts.length > 0 ? "ready" : "none",
  }
}

/** Runs `task` after the next frame has painted; the returned function cancels it. */
function afterPaint(task: () => void): () => void {
  let timer: ReturnType<typeof setTimeout> | undefined
  const frame = requestAnimationFrame(() => {
    timer = setTimeout(task, 0)
  })
  return () => {
    cancelAnimationFrame(frame)
    if (timer !== undefined) clearTimeout(timer)
  }
}

// Bootstrap indexing can delay a tap's next paint; render only existing resolvers and initialize
// new ones after paint or during idle time.
const createdResolvers = new WeakSet<LoadedSnapshot>()

function existingResolver(
  client: SnapshotClient,
  snapshot: LoadedSnapshot | null,
): AnnotatedTextResolver | null {
  return snapshot && createdResolvers.has(snapshot) ? sharedAnnotatedTextResolver(client) : null
}

function createResolver(client: SnapshotClient, snapshot: LoadedSnapshot): AnnotatedTextResolver {
  const resolver = sharedAnnotatedTextResolver(client)
  createdResolvers.add(snapshot)
  return resolver
}

function whenIdle(task: () => void): () => void {
  if (typeof requestIdleCallback === "function") {
    const handle = requestIdleCallback(task, { timeout: 2000 })
    return () => {
      cancelIdleCallback(handle)
    }
  }
  const timer = setTimeout(task, 300)
  return () => {
    clearTimeout(timer)
  }
}

/**
 * Builds the loaded snapshot's text indexes while the page is idle by resolving one text, so the
 * first card opened from a list does not pay for them. Returns the cancel function.
 */
export function warmTextResolver(
  client: SnapshotClient,
  owner: JsonObject,
  field: TextField,
  language: string,
): () => void {
  const snapshot = client.snapshot()
  if (!snapshot) return () => undefined
  return whenIdle(() => {
    if (client.snapshot() !== snapshot) return
    void createResolver(client, snapshot)
      .resolve({ owner, field, ordinal: null }, language)
      .catch(() => undefined)
  })
}

// A changed exact owner or edition must never display the previous identity's text.
export function useAnnotatedText(
  client: SnapshotClient,
  owner: JsonObject,
  field: TextField,
  language: string,
  edition: Region,
  sharedResolver?: AnnotatedTextResolver | null,
): AnnotatedTextState {
  const root = useSnapshotRoot()
  const snapshot = client.snapshot()
  const identity = canonicalText([owner, field, language, edition])
  const [loaded, setLoaded] = useState<{
    readonly snapshot: typeof snapshot
    readonly identity: string
    readonly value?: SelectedText | null
    readonly error?: TextFailure
  }>()
  // A text resolved earlier for this snapshot renders final on the first pass.
  const peeked = (sharedResolver ?? existingResolver(client, snapshot))?.peek(
    { owner, field, ordinal: null },
    language,
  )
  useEffect(() => {
    if (!snapshot) return
    let active = true
    const [currentOwner, currentField, currentLanguage] = arrayValue(parseJson(identity))
    const pointer = {
      owner: objectValue(currentOwner),
      field: stringValue(currentField),
      ordinal: null,
    }
    const existing = sharedResolver ?? existingResolver(client, snapshot)
    if (existing?.peek(pointer, stringValue(currentLanguage)) !== undefined) return
    const cancel = afterPaint(() => {
      const resolver = sharedResolver ?? createResolver(client, snapshot)
      void resolver.resolve(pointer, stringValue(currentLanguage)).then(
        (value) => {
          if (active) setLoaded({ snapshot, identity, value })
        },
        (error: unknown) => {
          if (active)
            setLoaded({
              snapshot,
              identity,
              error:
                error instanceof SnapshotError
                  ? { kind: "validation", code: error.code, message: error.message }
                  : {
                      kind: "load",
                      message: error instanceof Error ? error.message : String(error),
                    },
            })
        },
      )
    })
    return () => {
      active = false
      cancel()
    }
  }, [client, sharedResolver, snapshot, identity])
  const settled =
    loaded?.snapshot === snapshot && loaded.identity === identity
      ? loaded
      : peeked === undefined
        ? undefined
        : { snapshot, identity, value: peeked }
  const base = {
    root,
    generation: generationOf(snapshot),
    ...(snapshot ? { dataVersion: snapshot.dataVersion, manifestHash: snapshot.manifestHash } : {}),
    owner,
    ordinal: null,
    field,
    language,
    edition,
  }
  if (settled === undefined)
    return {
      readiness: { ...base, basic: "pending", source: "pending", annotations: "pending" },
      value: null,
    }
  if (settled.error)
    return {
      readiness: {
        ...base,
        basic: "error",
        source: "error",
        annotations: "error",
        error: settled.error,
      },
      value: null,
    }
  const value = settled.value ?? null
  return { readiness: { ...base, ...purposesOf(value) }, value }
}
