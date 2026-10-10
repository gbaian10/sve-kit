import { SearchEngine } from "./engine"
import type { SearchRequest } from "./messages"

const engine = new SearchEngine((event) => {
  self.postMessage(event)
})
self.addEventListener("message", (event: MessageEvent<SearchRequest>) => {
  void engine.handle(event.data)
})
