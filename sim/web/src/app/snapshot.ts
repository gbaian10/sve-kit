import { useEffect, useState } from "react"

import {
  type Catalog,
  catalogOf,
  type ImageFace,
  type ImageIndex,
  type LoadedSnapshot,
  loadImagePage,
  type SnapshotClient,
  snapshotClient,
  type SnapshotRoot,
  type SnapshotStatus,
  useSnapshotRoot,
  useSnapshotStatus,
} from "../data"

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

/** Only intersecting faces own decoded rows; leaving the viewport releases the previous page. */
export function useImageIndex(client: SnapshotClient, ready: boolean): ImageIndex | undefined {
  const [selection, setSelection] = useState("")
  const [loaded, setLoaded] = useState<{
    readonly snapshot: LoadedSnapshot
    readonly selection: string
    readonly index: ImageIndex
  }>()
  const snapshot = client.snapshot()
  useEffect(() => {
    if (!ready || !snapshot) return
    const visible = new Set<Element>()
    const collect = () => {
      const unique = new Set<string>()
      const faces = [...visible]
        .filter((node) => node.isConnected)
        .sort((a, b) => a.getBoundingClientRect().top - b.getBoundingClientRect().top)
        .map((node) => ({
          printingId: node.getAttribute("data-image-printing") ?? "",
          faceId: node.getAttribute("data-image-face") ?? "",
        }))
        .filter((face) => {
          const key = JSON.stringify([face.printingId, face.faceId])
          if (unique.has(key)) return false
          unique.add(key)
          return true
        })
        .slice(0, 24)
      setSelection(JSON.stringify(faces))
    }
    const observer =
      typeof IntersectionObserver === "undefined"
        ? undefined
        : new IntersectionObserver((entries) => {
            for (const entry of entries) {
              if (entry.isIntersecting) visible.add(entry.target)
              else visible.delete(entry.target)
            }
            collect()
          })
    const observed = new Set<Element>()
    const scan = () => {
      for (const node of document.querySelectorAll("[data-image-face]")) {
        if (observed.has(node)) continue
        observed.add(node)
        if (observer) observer.observe(node)
        else visible.add(node)
      }
      for (const node of observed)
        if (!node.isConnected) {
          observer?.unobserve(node)
          observed.delete(node)
          visible.delete(node)
        }
      collect()
    }
    const mutations = new MutationObserver(scan)
    mutations.observe(document.body, { childList: true, subtree: true })
    scan()
    return () => {
      observer?.disconnect()
      mutations.disconnect()
    }
  }, [ready, snapshot])
  useEffect(() => {
    if (!ready || !snapshot || !selection) return
    const abort = new AbortController()
    const faces = JSON.parse(selection) as ImageFace[]
    void loadImagePage(client, faces, abort.signal).then(
      (index) => {
        if (!abort.signal.aborted) setLoaded({ snapshot, selection, index })
      },
      () => {
        if (!abort.signal.aborted)
          setLoaded({
            snapshot,
            selection,
            index: { asset: () => undefined, cardImage: () => undefined, failed: true },
          })
      },
    )
    return () => {
      abort.abort()
    }
  }, [client, ready, snapshot, selection])
  return loaded?.snapshot === snapshot && loaded.selection === selection ? loaded.index : undefined
}
