// The battle inputs document `build:engine` wrote from the pinned public export, beside the engine.
// The local battle Worker reads only this; nothing a caller assembles reaches the session.
const INPUTS = new URL("../battle/engine/generated/battle-inputs.json", import.meta.url)

export async function loadBattleInputs(): Promise<string> {
  const response = await fetch(INPUTS)
  if (!response.ok) throw new Error(`battle inputs unavailable: ${String(response.status)}`)
  return response.text()
}
