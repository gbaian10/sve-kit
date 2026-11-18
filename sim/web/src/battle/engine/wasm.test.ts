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

const keywords = JSON.stringify({ format: 1, kind: "keyword_registry", keywords: {} })
const selection = JSON.stringify({ documents: [], authors: [] })
const references = JSON.stringify({
  region: "jp",
  snapshot_sha256: "0".repeat(64),
  faces: [],
  wording: null,
  rules: null,
})
const sha256 = (text: string) => createHash("sha256").update(text).digest("hex")
const content = (id: string, sha: string) => ({ id, sha256: sha })
/** Names exactly the selection and rules below; the snapshot and engine are stated by the host. */
const identity = (rules: string) =>
  JSON.stringify({
    snapshot: content("snapshot", "1".repeat(64)),
    programs: content("programs", sha256(selection)),
    engine: content("engine", "2".repeat(64)),
    rules: content("rules", sha256(rules)),
    region: "jp",
  })
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
    const stated = identity("{}")
    expect(() => new Session("", keywords, selection, "{}", references, stated, "{}")).toThrow(
      /^format$/,
    )
    expect(() => new Session("", keywords, "[", "{}", references, stated, setup)).toThrow(
      /^format$/,
    )
    expect(() => new Session("", keywords, selection, "{}", "{}", stated, setup)).toThrow(
      /^format$/,
    )
    expect(
      () => new Session("", keywords, selection, "{}", references, identity("[]"), setup),
    ).toThrow(/^identity$/)
    expect(() => new Session("", keywords, selection, "{}", references, stated, setup)).toThrow(
      /^selection$/,
    )
  })
})
