import { useEffect, useState } from "react"

import {
  type Catalog,
  catalogOf,
  type ImageFace,
  type ImageIndex,
  isSnapshotImageFace,
  type LoadedSnapshot,
  loadImagePage,
  retainedImageIndex,
  type SnapshotClient,
  snapshotClient,
  type SnapshotRoot,
  type SnapshotStatus,
  useSnapshotRoot,
  useSnapshotStatus,
} from "../data"

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

interface ImageStore {
  running: number
  readonly listeners: Set<() => void>
  readonly failed: Set<string>
  readonly inflight: Set<string>
}
const imageStores = new WeakMap<LoadedSnapshot, ImageStore>()
function imageStore(snapshot: LoadedSnapshot): ImageStore {
  let store = imageStores.get(snapshot)
  if (!store) {
    store = {
      failed: new Set(),
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
        if (failed) store.failed.add(key)
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
        void loadImagePage(client, batch).then(
          () => {
            settle(batch, false)
          },
          () => {
            settle(batch, true)
          },
        )
      }
    }
    const collect = () => {
      pump()
      if (active)
        setVisible(
          new Set(
            [...onScreen]
              .filter((node) => node.isConnected)
              .map((node) => {
                const face = faceOfNode(node)
                return faceKeyOf(face.printingId, face.faceId)
              }),
          ),
        )
    }
    const schedule = () => {
      clearTimeout(timer)
      timer = setTimeout(collect, COLLECT_DEBOUNCE_MS)
    }
    const track = (set: Set<Element>) => (entries: IntersectionObserverEntry[]) => {
      for (const entry of entries) {
        if (entry.isIntersecting) set.add(entry.target)
        else set.delete(entry.target)
      }
      schedule()
    }
    const supported = typeof IntersectionObserver !== "undefined"
    // IntersectionObserver percentages use root width, so screen-height margins need pixels.
    const createNearObserver = () =>
      supported
        ? new IntersectionObserver(track(near), {
            rootMargin: `${String(window.innerHeight * 0.5)}px 0px ${String(window.innerHeight * 1.5)}px 0px`,
          })
        : undefined
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
      nearObserver?.disconnect()
      near.clear()
      nearObserver = createNearObserver()
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
