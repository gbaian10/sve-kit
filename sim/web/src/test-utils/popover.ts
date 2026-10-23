import { afterEach } from "vitest"

// jsdom has no Popover API; model the native open state and dismissal used by UI tests.
if (typeof HTMLElement !== "undefined" && typeof HTMLElement.prototype.showPopover !== "function") {
  const shown = new Map<HTMLElement, HTMLElement | undefined>()
  afterEach(() => {
    shown.clear()
  })
  const nativeMatches = Object.getOwnPropertyDescriptor(Element.prototype, "matches")
    ?.value as Element["matches"]
  Object.defineProperty(Element.prototype, "matches", {
    configurable: true,
    writable: true,
    value: function matches(this: Element, selector: string) {
      if (selector === ":popover-open") return this instanceof HTMLElement && shown.has(this)
      return nativeMatches.call(this, selector)
    },
  })
  HTMLElement.prototype.showPopover = function showPopover(options?: ShowPopoverOptions) {
    for (const popup of shown.keys()) {
      if (popup !== this && !popup.contains(this)) popup.hidePopover()
    }
    shown.set(this, options?.source)
    this.style.display = "block"
    this.dispatchEvent(Object.assign(new Event("toggle"), { oldState: "closed", newState: "open" }))
  }
  HTMLElement.prototype.hidePopover = function hidePopover() {
    if (!shown.has(this)) return
    shown.delete(this)
    this.style.display = "none"
    this.dispatchEvent(Object.assign(new Event("toggle"), { oldState: "open", newState: "closed" }))
  }
  document.addEventListener("keydown", (event) => {
    if (event.key !== "Escape") return
    const last = [...shown.entries()].at(-1)
    if (!last) return
    const [popup, source] = last
    popup.hidePopover()
    source?.focus()
  })
  document.addEventListener("pointerdown", (event) => {
    if (!(event.target instanceof Node)) return
    for (const [popup, source] of [...shown]) {
      if (!popup.contains(event.target) && !source?.contains(event.target)) popup.hidePopover()
    }
  })
}
