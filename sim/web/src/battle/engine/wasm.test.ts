// @vitest-environment node
// The generated engine itself (built by `build:engine`), on inputs too small to start a match.
import { readFileSync } from "node:fs"
import path from "node:path"

import { beforeAll, describe, expect, it } from "vitest"

import { initSync, Session } from "./generated/engine"

beforeAll(() => {
  initSync({ module: readFileSync(path.join(import.meta.dirname, "generated/engine_bg.wasm")) })
})

const keywords = JSON.stringify({ format: 1, kind: "keyword_registry", keywords: {} })
const selection = JSON.stringify({ documents: [], authors: [] })
const deck = { construction: { kind: "class", value: "c" }, main: ["x"], evolve: [] }
const setup = JSON.stringify({
  decks: [deck, deck],
  first_chooser: "p1",
  seed: "s",
  control: ["player", "policy"],
  max_submissions: 1,
})

describe("engine WebAssembly", () => {
  it("rejects malformed setup and unusable inputs with codes only", () => {
    expect(() => new Session("", keywords, selection, "{}", "{}")).toThrow(/^format$/)
    expect(() => new Session("", keywords, "[", "{}", setup)).toThrow(/^format$/)
    expect(() => new Session("", keywords, selection, "{}", setup)).toThrow(/^selection$/)
  })
})
