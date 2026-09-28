import "@testing-library/jest-dom/vitest"

import { cleanup } from "@testing-library/react"
import { afterEach } from "vitest"

// Vitest runs without globals, so Testing Library cannot register this itself.
afterEach(cleanup)

// jsdom has no matchMedia; the theme hooks only need "not dark" plus inert listeners.
if (typeof window !== "undefined" && typeof window.matchMedia !== "function") {
  window.matchMedia = (query: string): MediaQueryList =>
    ({
      matches: false,
      media: query,
      onchange: null,
      addEventListener: () => undefined,
      removeEventListener: () => undefined,
      addListener: () => undefined,
      removeListener: () => undefined,
      dispatchEvent: () => false,
    }) as MediaQueryList
}

// jsdom 30 has no <dialog> behaviour; this is only what the Dialog component relies on.
if (typeof HTMLDialogElement !== "undefined" && !("showModal" in HTMLDialogElement.prototype)) {
  const proto = HTMLDialogElement.prototype as HTMLDialogElement & {
    showModal: () => void
    show: () => void
    close: (returnValue?: string) => void
  }
  proto.showModal = function showModal(this: HTMLDialogElement) {
    this.setAttribute("open", "")
  }
  proto.show = function show(this: HTMLDialogElement) {
    this.setAttribute("open", "")
  }
  proto.close = function close(this: HTMLDialogElement, returnValue?: string) {
    if (returnValue !== undefined) this.returnValue = returnValue
    this.removeAttribute("open")
    this.dispatchEvent(new Event("close"))
  }
}
