import { describe, expect, it } from "vitest"

import { lockedBindgenVersion, wasmArtifact } from "./build"

describe("engine build", () => {
  it("reads the locked wasm-bindgen version, not a similarly named crate", () => {
    const lock = [
      "[[package]]",
      'name = "wasm-bindgen-macro"',
      'version = "9.9.9"',
      "",
      "[[package]]",
      'name = "wasm-bindgen"',
      'version = "0.2.129"',
    ].join("\n")
    expect(lockedBindgenVersion(lock)).toBe("0.2.129")
    expect(() => lockedBindgenVersion("")).toThrow("no wasm-bindgen")
  })

  it("picks exactly one battle-host wasm artifact from cargo messages", () => {
    const artifact = (name: string, filenames: string[]) =>
      JSON.stringify({ reason: "compiler-artifact", target: { name }, filenames })
    const messages = [
      "Compiling…",
      artifact("sve_engine", ["/t/libsve_engine.rlib"]),
      artifact("sve_battle_host", ["/t/sve_battle_host.wasm", "/t/libsve_battle_host.rlib"]),
      JSON.stringify({ reason: "build-finished", success: true }),
    ].join("\n")
    expect(wasmArtifact(messages)).toBe("/t/sve_battle_host.wasm")
    expect(() => wasmArtifact("")).toThrow("found 0")
  })
})
