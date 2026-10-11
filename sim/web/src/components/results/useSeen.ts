import { type RefObject, useEffect, useState } from "react"

// Sharing the observer avoids per-row work; pixel margins follow viewport height, not width.
const callbacks = new Map<Element, () => void>()
let observer: IntersectionObserver | undefined
const pending = new Set<Element>()
let timer: ReturnType<typeof setTimeout> | undefined

function activate(): void {
  timer = undefined
  const node = pending.values().next().value
  if (!node) return
  pending.delete(node)
  const onSeen = callbacks.get(node)
  callbacks.delete(node)
  onSeen?.()
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
  return new IntersectionObserver(
    (entries) => {
      for (const entry of entries)
        if (entry.isIntersecting) {
          if (!callbacks.has(entry.target)) continue
          pending.add(entry.target)
          observer?.unobserve(entry.target)
        }
      // A single intersection delivery can cover a whole screen of expensive previews.
      if (timer === undefined) activate()
    },
    { rootMargin: `${String(window.innerHeight)}px 0px ${String(window.innerHeight)}px 0px` },
  )
}

function resize(): void {
  observer?.disconnect()
  observer = createObserver()
  for (const node of callbacks.keys()) observer.observe(node)
}

function observe(node: Element, onSeen: () => void): () => void {
  if (typeof IntersectionObserver === "undefined") {
    onSeen()
    return () => undefined
  }
  if (!observer) {
    observer = createObserver()
    window.addEventListener("resize", resize)
  }
  callbacks.set(node, onSeen)
  observer.observe(node)
  return () => {
    callbacks.delete(node)
    pending.delete(node)
    observer?.unobserve(node)
    release()
  }
}

/** True once the referenced element has come near the viewport; never goes back to false. */
export function useSeen(ref: RefObject<Element | null>): boolean {
  const [seen, setSeen] = useState(false)
  useEffect(() => {
    const node = ref.current
    if (!node || seen) return
    return observe(node, () => {
      setSeen(true)
    })
  }, [ref, seen])
  return seen
}
