// Replays self-play records on the WebAssembly battle host and writes the time of every
// submission, in the shape of `sve-selfplay replay`. See the crate README.
import { createRequire } from "node:module"
import { readFileSync, writeFileSync } from "node:fs"
import path from "node:path"

const [glue, inputs, programs, records, output] = process.argv.slice(2)
if (output === undefined) {
  console.error(
    "usage: node replay-wasm.mjs <sve_battle_host.js> <inputs.json> <programs.json> <records.jsonl> <timings.jsonl>",
  )
  process.exit(2)
}

// wasm-bindgen's nodejs target emits CommonJS.
const { PreparedMatch } = createRequire(import.meta.url)(path.resolve(glue))
const prepared = new PreparedMatch(readFileSync(inputs, "utf8"), readFileSync(programs, "utf8"))
const lines = []
for (const line of readFileSync(records, "utf8").split("\n")) {
  if (line === "") continue
  const record = JSON.parse(line)
  const micros = []
  let session
  try {
    session = prepared.open(JSON.stringify(record.setup))
  } catch (error) {
    // The host throws its stage code when the decks do not open.
    lines.push(
      JSON.stringify({
        seed: record.seed,
        end: `load: ${String(error.message)}`,
        engine_micros: micros,
      }),
    )
    continue
  }
  let end = "finished"
  try {
    let deliveries = JSON.parse(session.start())
    for (const [index, step] of record.steps.entries()) {
      const message = JSON.stringify({
        kind: "submit",
        request_id: `r${String(index)}`,
        command: step.command,
      })
      const start = performance.now()
      const text = session.submit(step.seat, message)
      micros.push(Math.round((performance.now() - start) * 1000))
      deliveries = JSON.parse(text)
      const receipt = deliveries.find(({ message: sent }) => sent.kind === "receipt")
      if (receipt?.message.result !== step.result) {
        end = `replay: step ${String(index)} got ${String(receipt?.message.result)}`
        break
      }
    }
    const error = deliveries.find(({ message: sent }) => sent.kind === "host_error")
    if (error !== undefined) end = `host_error: ${String(error.message.reason)}`
  } finally {
    session.free()
  }
  lines.push(JSON.stringify({ seed: record.seed, end, engine_micros: micros }))
}
prepared.free()
writeFileSync(output, lines.map((line) => `${line}\n`).join(""))
