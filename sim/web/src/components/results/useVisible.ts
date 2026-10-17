import { type RefObject, useEffect, useState } from "react"

// Sharing the observer avoids per-row work; pixel margins follow viewport height, not width.
const callbacks = new Map<Element, (visible: boolean) => void>()
let observer: IntersectionObserver | undefined
const pending = new Set<Element>()
let timer: ReturnType<typeof setTimeout> | undefined

function activate(): void {
  timer = undefined
  const node = pending.values().next().value
  if (!node) return
  pending.delete(node)
  const onSeen = callbacks.get(node)
  onSeen?.(true)
  release()
  if (pending.size) timer = setTimeout(activate, 0)
}

function release(): void {
  if (callbacks.size !== 0) return
  observer?.disconnect()
  observer = undefined
  pending.clear()
  if (timer !== undefined) clearTimeout(timer)
  timer = undefined
  window.removeEventListener("resize", resize)
}

function createObserver(): IntersectionObserver {
  const node = callbacks.keys().next().value
  const container = node?.closest<HTMLElement>("[data-list-scroll]")
  const root = container && getComputedStyle(container).overflowY === "auto" ? container : null
  const margin = root?.clientHeight ?? window.innerHeight
  return new IntersectionObserver(
    (entries) => {
      for (const entry of entries)
        if (entry.isIntersecting) {
          if (!callbacks.has(entry.target)) continue
          pending.add(entry.target)
        } else {
          pending.delete(entry.target)
          callbacks.get(entry.target)?.(false)
        }
      // A single intersection delivery can cover a whole screen of expensive previews.
      if (timer === undefined) activate()
    },
    { root, rootMargin: `${String(margin)}px 0px ${String(margin)}px 0px` },
  )
}

function resize(): void {
  observer?.disconnect()
  observer = createObserver()
  for (const node of callbacks.keys()) observer.observe(node)
}

function observe(node: Element, onSeen: (visible: boolean) => void): () => void {
  if (typeof IntersectionObserver === "undefined") {
    onSeen(true)
    return () => undefined
  }
  callbacks.set(node, onSeen)
  if (!observer) {
    observer = createObserver()
    window.addEventListener("resize", resize)
  }
  observer.observe(node)
  return () => {
    callbacks.delete(node)
    pending.delete(node)
    observer?.unobserve(node)
    release()
  }
}

/** Readers live only near the viewport; decoded text stays in the existing data-layer cache. */
export function useVisible(ref: RefObject<Element | null>): boolean {
  const [visible, setVisible] = useState(false)
  useEffect(() => {
    const node = ref.current
    if (!node) return
    return observe(node, setVisible)
  }, [ref])
  return visible
}
