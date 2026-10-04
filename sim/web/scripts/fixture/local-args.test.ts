// @vitest-environment node
import { execFileSync } from "node:child_process"

import { describe, expect, it } from "vitest"

import { parseLocalArgs } from "./local-args"

describe("fixture:local arguments", () => {
  it("accepts separated and equals values without scanning the argv array", () => {
    expect(
      parseLocalArgs(["--out", "/tmp/synthetic cdn", "--sets=TS01,TS02", "--limit", "24"]),
    ).toEqual({
      out: "/tmp/synthetic cdn",
      sets: "TS01,TS02",
      limit: "24",
    })
    expect(parseLocalArgs([])).toEqual({})
  })

  it.each(["out", "sets", "limit"])("rejects a missing --%s value", (name) => {
    expect(() => parseLocalArgs([`--${name}`])).toThrow(
      expect.objectContaining({
        code: "ERR_PARSE_ARGS_INVALID_OPTION_VALUE",
      }),
    )
    expect(() => parseLocalArgs([`--${name}`, "--out=/tmp/synthetic"])).toThrow(
      expect.objectContaining({
        code: "ERR_PARSE_ARGS_INVALID_OPTION_VALUE",
      }),
    )
  })

  it.each([["--unknown"], ["--ot=/tmp/synthetic"], ["-o", "/tmp/synthetic"]])(
    "rejects unknown options %j",
    (...args) => {
      expect(() => parseLocalArgs(args)).toThrow(
        expect.objectContaining({
          code: "ERR_PARSE_ARGS_UNKNOWN_OPTION",
        }),
      )
    },
  )

  it.each([["unexpected"], ["--", "unexpected"]])("rejects positionals %j", (...args) => {
    expect(() => parseLocalArgs(args)).toThrow(
      expect.objectContaining({
        code: "ERR_PARSE_ARGS_UNEXPECTED_POSITIONAL",
      }),
    )
  })

  it.each([["--out"], ["--unknown"]])(
    "CLI rejects malformed args before data I/O %j",
    (...args) => {
      const script = new URL("./local-main.ts", import.meta.url).pathname
      // No source paths are supplied: an argv error must precede even the required-env check.
      try {
        execFileSync("bun", [script, ...args], {
          env: { PATH: process.env["PATH"] },
          stdio: "pipe",
        })
        expect.fail("CLI accepted malformed arguments")
      } catch (error) {
        expect(error).toMatchObject({ status: 1 })
        expect(error).toHaveProperty("stderr")
        const { stderr } = error as { stderr: Buffer }
        expect(stderr.toString()).toContain("ERR_PARSE_ARGS_")
        expect(stderr.toString()).not.toContain("SVE_TEST_SNAPSHOT, SVE_DATA_DIR")
      }
    },
  )
})
