// Local battle Worker: loads the engine WebAssembly and serves the seat ports.
import init, { Session } from "./generated/engine"
import { BattleHost } from "./host"
import type { InitMessage } from "./protocol"

let ready: Promise<unknown> | undefined
// Production skips full schema validation (the engine still range-checks every number), and the
// DEV-only dynamic import keeps the generated validator out of the production Worker entirely.
const validate = import.meta.env.DEV
  ? import("../contract").then((contract) => contract.validateHostMessage)
  : undefined
const host = new BattleHost(
  async (inputs, setup) => {
    ready ??= init()
    await ready
    return new Session(inputs.snapshot, inputs.keywords, inputs.selection, inputs.rules, setup)
  },
  undefined,
  validate,
)
self.addEventListener(
  "message",
  (event: MessageEvent<InitMessage>) => {
    host.init(event.data)
  },
  { once: true },
)
