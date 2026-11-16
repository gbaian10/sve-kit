// @vitest-environment node
import { readdirSync, readFileSync } from "node:fs"
import path from "node:path"

import { expect, it } from "vitest"

import {
  type HostMessage,
  validateBattleObservation,
  validateClientMessage,
  validateHostMessage,
  validateQuestion,
} from "../src/battle/contract"

// The same hand-written fixtures the Rust tests decode.
const FIXTURES = path.resolve(import.meta.dirname, "../../battle-contract/fixtures")
const GROUPS = [
  ["host", validateHostMessage],
  ["client", validateClientMessage],
  ["observation", validateBattleObservation],
  ["question", validateQuestion],
] as const

type Json = null | boolean | number | string | Json[] | { [key: string]: Json }

function fixtures(group: string): [string, Json][] {
  const folder = path.join(FIXTURES, group)
  return readdirSync(folder)
    .sort()
    .map((name) => [
      `${group}/${name}`,
      JSON.parse(readFileSync(path.join(folder, name), "utf8")) as Json,
    ])
}

/** Every copy of the message with one field removed or one unknown field added. */
function* damaged(root: Json): Generator<[string, Json]> {
  function* visit(
    node: Json,
    where: string,
    rebuild: (next: Json) => Json,
  ): Generator<[string, Json]> {
    if (Array.isArray(node)) {
      for (const [index, item] of node.entries())
        yield* visit(item, `${where}/${String(index)}`, (next) =>
          rebuild(node.map((other, at) => (at === index ? next : other))),
        )
      return
    }
    if (typeof node !== "object" || node === null) return
    yield [`unknown field in ${where}`, rebuild({ ...node, unknown: null })]
    for (const [key, value] of Object.entries(node)) {
      // The object table is keyed by handle: fewer entries are still a table.
      if (!where.endsWith("/objects")) {
        const rest = Object.fromEntries(Object.entries(node).filter(([other]) => other !== key))
        yield [`missing ${key} in ${where}`, rebuild(rest)]
      }
      yield* visit(value, `${where}/${key}`, (next) => rebuild({ ...node, [key]: next }))
    }
  }
  yield* visit(root, "", (next) => next)
}

it.each(GROUPS)("accepts every %s fixture and refuses every damaged copy", (group, validate) => {
  const found = fixtures(group)
  expect(found).not.toHaveLength(0)
  for (const [name, message] of found) {
    expect(validate(message), name).toBe(true)
    for (const [damage, copy] of damaged(message))
      expect(validate(copy), `${name}: ${damage}`).toBe(false)
  }
})

it("refuses numbers a JavaScript number cannot hold exactly and another wire format", () => {
  const ready = fixtures("host").find(([name]) => name === "host/host-error.json")?.[1]
  expect(validateHostMessage(ready)).toBe(true)
  expect(validateHostMessage({ ...(ready as object), format: 2 })).toBe(false)

  const baseline = fixtures("observation").find(([name]) => name.endsWith("baseline.json"))?.[1]
  const counted = (count: number): unknown => {
    const copy = structuredClone(baseline) as { players: { zones: { deck: { count: number } } }[] }
    const first = copy.players[0]
    if (first === undefined) throw new Error("the baseline fixture has no player")
    first.zones.deck.count = count
    return copy
  }
  expect(validateBattleObservation(counted(Number.MAX_SAFE_INTEGER))).toBe(true)
  for (const count of [Number.MAX_SAFE_INTEGER + 1, -1, 0.5, Number.NaN, Number.POSITIVE_INFINITY])
    expect(validateBattleObservation(counted(count)), String(count)).toBe(false)
})

it("narrows a validated message by its kind", () => {
  const messages = fixtures("host").map(([, message]): unknown => message)
  const valid: HostMessage[] = messages.filter(validateHostMessage)
  const kinds = new Set(valid.map((message) => message.kind))
  expect(kinds).toEqual(new Set(["ready", "update", "receipt", "host_error"]))
  for (const message of valid) {
    // Each branch reads a field that only its own variant has.
    if (message.kind === "ready") expect(message.identity.region).toBe("jp")
    else if (message.kind === "update") expect(message.observation.viewer).toBe(message.viewer)
    else if (message.kind === "receipt") expect(message.request_id).not.toBe("")
    else expect(message.format).toBe(1)
  }
})

it("keeps unprovided rules distinct from known empty and supplied references", () => {
  const baseline = fixtures("question").find(([name]) => name.endsWith("first-player.json"))?.[1]
  for (const rules of [
    null,
    [],
    [{ kind: "cr", id: "0.0.1", revision: "synthetic", strength: "inferred" }],
  ]) {
    const value: unknown = { ...(baseline as object), rules }
    expect(validateQuestion(value)).toBe(true)
    if (!validateQuestion(value)) throw new Error("the rule availability fixture is invalid")
    expect(value.rules).toEqual(rules)
  }
  const missing = Object.fromEntries(
    Object.entries(baseline as object).filter(([key]) => key !== "rules"),
  )
  expect(validateQuestion(missing)).toBe(false)
})

it("preserves both safe integer operands of a conditional damage replacement", () => {
  const baseline = fixtures("observation").find(([name]) => name.endsWith("full.json"))?.[1]
  if (!validateBattleObservation(baseline)) throw new Error("the full fixture is invalid")
  const changed = (threshold: number, replacement: number): unknown => {
    const copy = structuredClone(baseline)
    for (const effect of copy.objects["o1"]?.state?.effects ?? []) {
      if (effect.change.kind === "prevention" && effect.change.change.kind === "cap") {
        effect.change.change.threshold = threshold
        effect.change.change.replacement = replacement
        return copy
      }
    }
    throw new Error("the full fixture has no conditional replacement")
  }
  for (const [threshold, replacement] of [
    [6, 5],
    [0, Number.MAX_SAFE_INTEGER],
    [Number.MAX_SAFE_INTEGER, 0],
  ] as const) {
    expect(validateBattleObservation(changed(threshold, replacement))).toBe(true)
  }
  for (const bad of [Number.MAX_SAFE_INTEGER + 1, -1, 0.5, Number.NaN, Number.POSITIVE_INFINITY]) {
    expect(validateBattleObservation(changed(bad, 5)), String(bad)).toBe(false)
    expect(validateBattleObservation(changed(6, bad)), String(bad)).toBe(false)
  }
})
