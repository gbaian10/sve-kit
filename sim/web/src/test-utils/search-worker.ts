import { SearchEngine, type SearchEvent, type SearchRequest } from "../data"

/** The jsdom boundary schedules the real engine messages as separate tasks. */
export class TestSearchWorker {
  onmessage: ((event: MessageEvent<SearchEvent>) => void) | null = null
  onerror: (() => void) | null = null
  onmessageerror: (() => void) | null = null
  private stopped = false
  private generation = 0
  private readonly engine = new SearchEngine((event) => {
    setTimeout(() => {
      if (!this.stopped)
        this.onmessage?.({ data: structuredClone(event) } as MessageEvent<SearchEvent>)
    }, 0)
  })
  postMessage(request: SearchRequest): void {
    if (request.kind === "load") this.generation = request.generation
    setTimeout(() => {
      if (!this.stopped) void this.engine.handle(structuredClone(request))
    }, 0)
  }
  terminate(): void {
    this.stopped = true
    void this.engine.handle({ kind: "cancel", generation: this.generation })
  }
}
