// Local battle Worker: loads the engine WebAssembly and serves the seat ports.
import init, { Session } from "./generated/engine"
import { BattleHost } from "./host"
import type { InitMessage } from "./protocol"

let ready: Promise<unknown> | undefined
const host = new BattleHost(async (inputs, setup) => {
  ready ??= init()
  await ready
  return new Session(inputs.snapshot, inputs.keywords, inputs.selection, inputs.rules, setup)
})
self.addEventListener(
  "message",
  (event: MessageEvent<InitMessage>) => {
    host.init(event.data)
  },
  { once: true },
)
