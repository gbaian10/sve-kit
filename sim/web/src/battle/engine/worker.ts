// Local battle Worker: loads the engine WebAssembly and serves the seat ports.
import { loadBattleAssets } from "../../data"
import { BATTLE_CDN, battleBuild } from "./assets"
import init, { PreparedMatch } from "./generated/engine"
import { BattleHost } from "./host"
import type { InitMessage } from "./protocol"

// Production skips full schema validation (the engine still range-checks every number), and the
// DEV-only dynamic import keeps the generated validator out of the production Worker entirely.
const validate = import.meta.env.DEV
  ? import("../contract").then((contract) => contract.validateHostMessage)
  : undefined
const INPUTS = new URL("./generated/battle-inputs.json", import.meta.url)
const host = new BattleHost(
  async (setup) => {
    if (battleBuild === null) throw new Error("format")
    const [, [inputs, programs]] = await Promise.all([
      init(),
      loadBattleAssets(BATTLE_CDN, INPUTS, battleBuild.programs),
    ])
    const prepared = new PreparedMatch(inputs, programs)
    try {
      return prepared.open(setup)
    } finally {
      prepared.free()
    }
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
