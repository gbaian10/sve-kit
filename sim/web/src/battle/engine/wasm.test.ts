// @vitest-environment node
// The generated engine itself (built by `build:engine`), on inputs too small to start a match.
import { createHash } from "node:crypto"
import { readFileSync } from "node:fs"
import path from "node:path"

import { beforeAll, describe, expect, it } from "vitest"

import { initSync, Session } from "./generated/engine"

beforeAll(() => {
  initSync({ module: readFileSync(path.join(import.meta.dirname, "generated/engine_bg.wasm")) })
})

const sha256 = (text: string) => createHash("sha256").update(text).digest("hex")
const content = (id: string, sha: string) => ({ id, sha256: sha })
const selection = JSON.stringify({ documents: [], authors: [] })

/** A battle inputs document over an empty snapshot whose identity names the stated programs. */
function inputs(
  edit: { selection?: string; references?: unknown; programs?: string } = {},
): string {
  const programs = edit.selection ?? selection
  return JSON.stringify({
    snapshot: "",
    keywords: JSON.stringify({ format: 1, kind: "keyword_registry", keywords: {} }),
    selection: programs,
    rules: "{}",
    references: edit.references ?? {
      region: "jp",
      snapshot_sha256: sha256(""),
      faces: [],
      wording: null,
      rules: null,
    },
    identity: {
      snapshot: content("snapshot", "1".repeat(64)),
      programs: content("programs", sha256(edit.programs ?? programs)),
      engine: content("engine", "2".repeat(64)),
      rules: content("rules", sha256("{}")),
      region: "jp",
    },
  })
}
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
    expect(() => new Session(inputs(), "{}")).toThrow(/^format$/)
    expect(() => new Session(inputs({ selection: "[" }), setup)).toThrow(/^format$/)
    expect(() => new Session(inputs({ references: {} }), setup)).toThrow(/^format$/)
    expect(() => new Session(inputs({ programs: "[]" }), setup)).toThrow(/^identity$/)
    expect(() => new Session(inputs(), setup)).toThrow(/^selection$/)
  })
})
