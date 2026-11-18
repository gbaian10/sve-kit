import type { Notice } from "../contract"

/** One seat's short-lived information stays readable after the update that delivered it. */
export class NoticeInbox {
  readonly #received = new Map<string, Notice | null>()

  receive(notices: readonly Notice[]): void {
    for (const notice of notices) {
      if (!this.#received.has(notice.id)) this.#received.set(notice.id, notice)
    }
  }

  pending(): Notice[] {
    return [...this.#received.values()].filter((notice) => notice !== null)
  }

  dismiss(id: string): void {
    if (this.#received.has(id)) this.#received.set(id, null)
  }

  clear(): void {
    this.#received.clear()
  }
}
