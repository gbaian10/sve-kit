import { SearchEngine } from "./engine"
import type { SearchRequest } from "./messages"
import { type NetworkMessage, workerFetch } from "./network"

const network = workerFetch((message) => {
  self.postMessage(message)
})
const background = workerFetch((message) => {
  self.postMessage(message)
}, false)
const engine = new SearchEngine(
  (event) => {
    self.postMessage(event)
  },
  { fetch: network.fetch, backgroundFetch: background.fetch },
)
self.addEventListener("message", (event: MessageEvent<SearchRequest | NetworkMessage>) => {
  if (event.data.kind === "fetch-reply") {
    network.receive(event.data)
    background.receive(event.data)
  } else if (event.data.kind !== "fetch" && event.data.kind !== "abort-fetch")
    void engine.handle(event.data)
})
