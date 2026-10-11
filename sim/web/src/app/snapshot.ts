import { useEffect, useMemo, useRef, useState, useSyncExternalStore } from "react"

import {
  activeSnapshotRoot,
  type AnnotatedTextResolver,
  arrayValue,
  canonicalText,
  type Catalog,
  catalogOf,
  type CompleteView,
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
  type SearchChannel,
  type SearchQuery,
  SearchSession,
  type SearchStatus,
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
  viewImages,
} from "../data"
import { DEFAULT_QUERY } from "../domain/query/model"
import type { Region } from "../domain/search"
import { usePrefs } from "../settings"

performance.mark("sve-app-start")

const FOCUS_CHECK_INTERVAL_MS = 60_000

interface ActiveWorker {
  readonly session: SearchSession
  readonly client: SnapshotClient
  start(edition: Region, force?: boolean): void
}
const workers = new Map<SnapshotRoot, ActiveWorker>()
function workerOf(root: SnapshotRoot): ActiveWorker {
  const existing = workers.get(root)
  if (existing) return existing
  const detail = snapshotClient(root)
  const session = new SearchSession()
  let edition: Region | undefined
  let status: SnapshotStatus = { state: "idle" }
  const listeners = new Set<() => void>()
  session.subscribe(() => {
    const next = session.getStatus()
    status =
      next.phase === "ready"
        ? {
            state: "ready",
            dataVersion: next.dataVersion ?? "",
            unconfirmed: next.unconfirmed ?? false,
            ...(next.updating ? { updating: next.step ?? ("index" as const) } : {}),
            ...(next.error
              ? {
                  updateError: { kind: "network" as const, detail: next.error.message },
                  outdated: true,
                }
              : {}),
          }
        : next.phase === "error"
          ? { state: "error", kind: "network", detail: next.error?.message ?? "search failed" }
          : {
              state: "loading",
              phase: next.step ?? "index",
              ...(next.step === "bootstrap" && next.progress
                ? { done: next.progress.done, total: next.progress.total }
                : {}),
            }
    for (const listener of listeners) listener()
  })
  const start = (next: Region, force = false) => {
    if (!force && edition === next && status.state !== "error") return
    edition = next
    session.load(detail.base, next, root === "preview" ? "preview" : "index")
  }
  let lastCheck = Date.now()
  const check = () => {
    if (activeSnapshotRoot() !== root) return
    lastCheck = Date.now()
    start(edition ?? "jp", true)
  }
  window.addEventListener("online", check)
  window.addEventListener("focus", () => {
    if (Date.now() - lastCheck >= FOCUS_CHECK_INTERVAL_MS) check()
  })
  const client: SnapshotClient = {
    ...detail,
    status: () => status,
    subscribe: (listener) => {
      listeners.add(listener)
      return () => listeners.delete(listener)
    },
    load: () => {
      start(edition ?? "jp")
      return Promise.resolve()
    },
    retry: () => {
      start(edition ?? "jp", true)
      return Promise.resolve()
    },
    reload: () => {
      start(edition ?? "jp", true)
      return Promise.resolve()
    },
  }
  const worker = { session, client, start }
  workers.set(root, worker)
  return worker
}

export function useActiveSnapshot(): {
  readonly client: SnapshotClient
  readonly status: SnapshotStatus
  readonly root: SnapshotRoot
} {
  const root = useSnapshotRoot()
  const { cardEdition } = usePrefs()
  const worker = workerOf(root)
  const status = useSnapshotStatus(worker.client)
  useEffect(() => {
    worker.start(cardEdition)
  }, [worker, cardEdition])
  return { client: worker.client, status, root }
}

function deferredDisposal(dispose: () => void) {
  let timer: ReturnType<typeof setTimeout> | undefined
  return {
    retain: () => {
      clearTimeout(timer)
    },
    release: () => {
      timer = setTimeout(dispose, 0)
    },
  }
}
function useWorkerChannel(identity = ""): { channel: SearchChannel; status: SearchStatus } {
  const { root } = useActiveSnapshot()
  const { session } = workerOf(root)
  const holder = useMemo(() => {
    const channel = session.createChannel()
    return {
      channel,
      identity,
      ...deferredDisposal(() => {
        channel.dispose()
      }),
    }
  }, [session, identity])
  const { channel } = holder
  useEffect(() => {
    holder.retain()
    return holder.release
  }, [holder])
  const status = useSyncExternalStore(session.subscribe, session.getStatus, session.getStatus)
  return { channel, status }
}
export interface SearchReadiness extends Omit<SearchStatus, "root" | "loading"> {
  readonly root: SnapshotRoot
  readonly requestedEdition: Region
  readonly loading?: {
    readonly root: SnapshotRoot
    readonly edition: Region
    readonly step: NonNullable<SearchStatus["step"]>
    readonly done?: number
    readonly total?: number
    readonly persistent?: boolean
  }
  readonly retry: () => void
}
export function useSearchReadiness(edition: Region): SearchReadiness {
  const { root, client } = useActiveSnapshot()
  const { session } = workerOf(root)
  const status = useSyncExternalStore(session.subscribe, session.getStatus, session.getStatus)
  const { loading: _loading, root: _root, ...state } = status
  return {
    ...state,
    root,
    requestedEdition: edition,
    retry: () => {
      void client.retry()
    },
    ...(status.loading
      ? {
          loading: {
            ...status.loading,
            root,
            step: status.step ?? ("index" as const),
            ...status.progress,
          },
        }
      : {}),
  }
}

export function useSearch(query: SearchQuery | null) {
  const { channel } = useWorkerChannel()
  const key = query ? JSON.stringify(query) : null
  useEffect(() => {
    if (key) channel.search(JSON.parse(key) as SearchQuery)
  }, [channel, key])
  return useSyncExternalStore(channel.subscribe, channel.getStatus, channel.getStatus)
}
function useSearchView(query: SearchQuery) {
  const { channel, status } = useWorkerChannel()
  const key = JSON.stringify(query)
  useEffect(() => {
    channel.view(JSON.parse(key) as SearchQuery)
  }, [channel, key])
  const channelState = useSyncExternalStore(channel.subscribe, channel.getStatus, channel.getStatus)
  const result =
    JSON.stringify(channelState.lastInput) === key
      ? channelState.result
      : { phase: "pending" as const }
  const view = result.phase === "complete" ? result.view : undefined
  const images = useMemo(() => (view ? viewImages(view) : undefined), [view])
  return { result, view, images, status }
}
export function useListView(query: SearchQuery, pages: number) {
  const { root } = useActiveSnapshot()
  const { session } = workerOf(root)
  const key = JSON.stringify(query)
  const store = useMemo(() => {
    const channels = Array.from({ length: pages }, () => session.createChannel())
    let value: { view?: CompleteView; error?: QueryFailure } = {}
    const listeners = new Set<() => void>()
    const update = () => {
      const results = channels.map((channel) => channel.getStatus().result)
      const error = results.find((result) => result.phase === "error")
      const complete = results.flatMap((result) =>
        result.phase === "complete" && result.view ? [result.view] : [],
      )
      const first = complete[0]
      value =
        error?.phase === "error"
          ? { error: error.error }
          : complete.length === pages && first
            ? {
                view: {
                  ...first,
                  page: { ...first.page, items: complete.flatMap((view) => view.page.items) },
                  faces: complete.flatMap((view) => view.faces),
                },
              }
            : {}
      for (const listener of listeners) listener()
    }
    const unsubs = channels.map((channel) => channel.subscribe(update))
    const lifecycle = deferredDisposal(() => {
      unsubs.forEach((unsub) => {
        unsub()
      })
      channels.forEach((channel) => {
        channel.dispose()
      })
    })
    return {
      get: () => value,
      subscribe: (listener: () => void) => {
        listeners.add(listener)
        return () => listeners.delete(listener)
      },
      start: () => {
        lifecycle.retain()
        channels.forEach((channel, page) => {
          channel.view(
            { ...(JSON.parse(key) as SearchQuery), offset: page * 60, limit: 60 },
            page === 0,
          )
        })
      },
      release: lifecycle.release,
    }
  }, [session, key, pages])
  useEffect(() => {
    store.start()
    return store.release
  }, [store])
  const result = useSyncExternalStore(store.subscribe, store.get, store.get)
  const images = useMemo(() => (result.view ? viewImages(result.view) : undefined), [result.view])
  useEffect(() => {
    if (result.view) performance.mark("sve-view-ready")
  }, [result.view])
  return { ...result, images, retry: store.start }
}

export function useWorkerSuggestions(text: string, edition: Region) {
  const { channel } = useWorkerChannel(edition)
  useEffect(() => {
    channel.suggest(text, edition)
  }, [channel, text, edition])
  return useSyncExternalStore(channel.subscribe, channel.getStatus, channel.getStatus)
}
export function useRecentSummaries(ids: readonly string[], edition: Region) {
  const { channel } = useWorkerChannel(JSON.stringify([ids, edition]))
  const key = JSON.stringify(ids)
  useEffect(() => {
    channel.recent(JSON.parse(key) as string[], edition)
  }, [channel, key, edition])
  return useSyncExternalStore(channel.subscribe, channel.getStatus, channel.getStatus).result
}
export function useSetOptions() {
  const { cardEdition } = usePrefs()
  const { view, result, status } = useSearchView({
    state: DEFAULT_QUERY,
    edition: cardEdition,
    limit: 60,
  })
  return { sets: view?.options.sets, classes: view?.options.classes, result, status }
}

/** The catalog of the loaded snapshot, or null while nothing is loaded yet. */
export function useCardSnapshot(): {
  readonly client: SnapshotClient
  readonly status: SnapshotStatus
  readonly catalog: Catalog | null
} {
  const root = useSnapshotRoot()
  const client = snapshotClient(root)
  const status = useSnapshotStatus(client)
  useEffect(() => {
    void client.load()
  }, [client])
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

export function useNeighbours(query: SearchQuery | null, target: string | null, maxItems?: number) {
  const identity = JSON.stringify([query, target, maxItems])
  const { channel } = useWorkerChannel(identity)
  useEffect(() => {
    if (query && target) channel.neighbours(query, target, maxItems)
  }, [channel, query, target, maxItems])
  return useSyncExternalStore(channel.subscribe, channel.getStatus, channel.getStatus)
}

export function useAbilityPreview(
  printingId: string,
  faceId: string,
  language: string,
  edition: Region,
) {
  const identity = JSON.stringify([printingId, faceId, language, edition])
  const { channel } = useWorkerChannel(identity)
  useEffect(() => {
    channel.preview(printingId, faceId, language, edition)
  }, [channel, printingId, faceId, language, edition])
  return useSyncExternalStore(channel.subscribe, channel.getStatus, channel.getStatus).result
}
