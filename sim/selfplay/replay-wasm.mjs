// Replays self-play records on the WebAssembly battle host and writes the time of every
// submission, in the shape of `sve-selfplay replay`. See the crate README.
import { createRequire } from "node:module"
import { appendFileSync, readFileSync, writeFileSync } from "node:fs"
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

/** How the last messages say the match stands for the seats that were told. */
function standing(deliveries, before) {
  const error = deliveries.find(({ message }) => message.kind === "host_error")
  if (error !== undefined) return `host_error: ${String(error.message.reason)}`
  const kinds = deliveries
    .filter(({ message }) => message.kind === "update")
    .map(({ message }) => message.input.kind)
  if (kinds.includes("finished")) return "finished"
  if (kinds.includes("engine_fault")) return "engine_fault"
  return kinds.length === 0 ? before : "unfinished"
}

function replay(record) {
  const micros = []
  let session
  try {
    session = prepared.open(JSON.stringify(record.setup))
  } catch (error) {
    // The host throws its stage code when the decks do not open.
    return { end: `load: ${String(error.message)}`, micros }
  }
  let end = standing(JSON.parse(session.start()), "unfinished")
  for (const [index, step] of record.steps.entries()) {
    const message = JSON.stringify({
      kind: "submit",
      request_id: `r${String(index)}`,
      command: step.command,
    })
    const start = performance.now()
    const text = session.submit(step.seat, message)
    micros.push(Math.round((performance.now() - start) * 1000))
    const deliveries = JSON.parse(text)
    const receipt = deliveries.find(({ message: sent }) => sent.kind === "receipt")
    end = standing(deliveries, end)
    if (receipt?.message.result !== step.result) {
      end = `replay: step ${String(index)} got ${String(receipt?.message.result)}`
      break
    }
  }
  session.free()
  return { end, micros }
}

writeFileSync(output, "")
for (const line of readFileSync(records, "utf8").split("\n")) {
  if (line === "") continue
  // A line of failures.jsonl wraps its record; a line of records.jsonl is one.
  const entry = JSON.parse(line)
  const record = entry.record ?? entry
  let result
  try {
    result = replay(record)
  } catch (error) {
    // A trap leaves the instance unusable: keep what was measured and stop.
    appendFileSync(
      output,
      `${JSON.stringify({ seed: record.seed, end: `panic: ${String(error)}`, engine_micros: [] })}\n`,
    )
    process.exit(1)
  }
  appendFileSync(
    output,
    `${JSON.stringify({ seed: record.seed, end: result.end, engine_micros: result.micros })}\n`,
  )
}
prepared.free()
