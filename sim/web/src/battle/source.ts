// The one junction between a battle screen and whatever plays the host for one seat: the local
// Worker (battle/engine) or a scripted fixture. A screen never reaches past this interface.
import type { Command, HostMessage, Receipt, Seat } from "./contract"

export interface BattleSource {
  readonly seat: Seat
  /**
   * This seat's ready message, updates and host errors, in delivery order. A new listener first
   * gets everything delivered so far, so a remounted screen starts from the same baseline.
   */
  listen(listener: (message: HostMessage) => void): () => void
  /**
   * Sends one command and resolves with the host's receipt for it. What the command changed, if
   * anything, arrives as updates through `listen`; a refused command changes nothing. Rejects when
   * no receipt will come: the source is closed or stopped, or another command is still out.
   */
  submit(command: Command): Promise<Receipt>
  close(): void
}
