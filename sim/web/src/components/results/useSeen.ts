import { type RefObject, useEffect, useState } from "react"

// One observer for every row: a row is "seen" once it comes within a screen of the viewport and
// stays seen, so work it triggers (the ability preview) runs once per row and only for rows the
// reader can reach soon. Without IntersectionObserver (tests) every row counts as seen.
const callbacks = new WeakMap<Element, () => void>()
let observer: IntersectionObserver | undefined

function observe(node: Element, onSeen: () => void): () => void {
  if (typeof IntersectionObserver === "undefined") {
    onSeen()
    return () => undefined
  }
  observer ??= new IntersectionObserver(
    (entries) => {
      for (const entry of entries)
        if (entry.isIntersecting) {
          callbacks.get(entry.target)?.()
          callbacks.delete(entry.target)
          observer?.unobserve(entry.target)
        }
    },
    { rootMargin: "100% 0px 100% 0px" },
  )
  callbacks.set(node, onSeen)
  observer.observe(node)
  return () => {
    callbacks.delete(node)
    observer?.unobserve(node)
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
