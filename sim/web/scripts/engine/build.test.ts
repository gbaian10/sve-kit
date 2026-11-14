import { describe, expect, it } from "vitest"

import { lockedBindgenVersion, wasmArtifact } from "./build"

describe("engine build", () => {
  it("reads the one locked wasm-bindgen version by exact package name", () => {
    const metadata = (packages: [string, string][]) =>
      JSON.stringify({
        packages: packages.map(([name, version]) => ({ name, version, id: `${name}@${version}` })),
        workspace_members: [],
      })
    expect(
      lockedBindgenVersion(
        metadata([
          ["wasm-bindgen-macro", "9.9.9"],
          ["wasm-bindgen", "0.2.129"],
        ]),
      ),
    ).toBe("0.2.129")
    expect(() => lockedBindgenVersion(metadata([["wasm-bindgen-shared", "1.0.0"]]))).toThrow(
      "found 0",
    )
    expect(() =>
      lockedBindgenVersion(
        metadata([
          ["wasm-bindgen", "0.2.129"],
          ["wasm-bindgen", "0.2.130"],
        ]),
      ),
    ).toThrow("found 2")
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
