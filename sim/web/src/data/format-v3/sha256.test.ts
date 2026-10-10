// @vitest-environment node
import { describe, expect, it } from "vitest"

import vectorsText from "../../../fixtures/canonical-vectors.json?raw"
import { type JsonObject, parseStrict, utf8 } from "./json"
import { bucket, digest, hex, sha256 } from "./sha256"

const vectors = parseStrict(vectorsText) as JsonObject

describe("sha256", () => {
  it.each([
    ["", "e3b0c44298fc1c149afbf4c8996fb92427ae41e4649b934ca495991b7852b855"],
    ["abc", "ba7816bf8f01cfea414140de5dae2223b00361a396177a9cb410ff61f20015ad"],
    [
      "abcdbcdecdefdefgefghfghighijhijkijkljklmklmnlmnomnopnopq",
      "248d6a61d20638b8e5c026930c3e6039a33ce45964ff2167f6ecedd419db06c1",
    ],
  ])("hashes %j like FIPS 180-4", (input, expected) => {
    expect(hex(sha256(utf8(input)))).toBe(expected)
  })

  it("agrees with WebCrypto across block boundaries", async () => {
    for (let length = 0; length < 200; length += 7) {
      const data = new Uint8Array(length).map((_, i) => (i * 31 + length) & 0xff)
      const expected = new Uint8Array(await crypto.subtle.digest("SHA-256", data))
      expect(hex(sha256(data))).toBe(hex(expected))
    }
    const big = new Uint8Array(70000).map((_, i) => i & 0xff)
    expect(hex(sha256(big))).toBe(hex(new Uint8Array(await crypto.subtle.digest("SHA-256", big))))
  })

  it("prefixes digests and computes the shared bucket vector", () => {
    const vector = vectors["bucket"] as JsonObject
    const key = vector["key"] as string[]
    expect(digest(utf8(JSON.stringify(key)))).toBe(vector["sha256"])
    expect(bucket(key, vector["count"] as number)).toBe(vector["expected"])
    expect(bucket(key, 1)).toBe(0)
    expect(() => bucket(key, 0)).toThrow(RangeError)
  })
})
