// A source that plays a scene's script for one seat. It follows the script and decides nothing
// about the rules; each seat gets its own source, as with the Worker's per-seat channels.
import type { Seat } from "../contract"
import type { BattleSource } from "../source"
import type { Scene } from "./scenes"

export function scriptedSource(scene: Scene, seat: Seat): BattleSource {
  let closed = false
  return {
    seat,
    listen(listener) {
      if (!closed) for (const message of scene.script[seat]) listener(message)
      return () => undefined
    },
    close() {
      closed = true
    },
  }
}
