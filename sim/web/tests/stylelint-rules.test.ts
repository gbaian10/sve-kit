// @vitest-environment node
import path from "node:path"

import stylelint from "stylelint"
import { describe, expect, it } from "vitest"

const root = path.resolve(import.meta.dirname, "..")

async function rulesFor(code: string, file = "src/styles/app.css"): Promise<string[]> {
  const { results } = await stylelint.lint({
    code,
    codeFilename: path.join(root, file),
    configFile: path.join(root, "stylelint.config.js"),
  })
  return (results[0]?.warnings ?? []).map((warning) => warning.rule).sort()
}

describe("stylelint rules", () => {
  it.each([
    ["hex colour", ".a { color: #fff; }", ["color-no-hex"]],
    ["named colour", ".a { color: red; }", ["color-named"]],
    ["colour function", ".a { color: oklch(50% 0.1 200deg); }", ["function-disallowed-list"]],
    [
      "color-mix",
      ".a { color: color-mix(in oklch, var(--text), transparent); }",
      ["function-disallowed-list"],
    ],
    ["100vh", ".a { height: 100vh; }", ["unit-disallowed-list"]],
    ["!important", ".a { color: var(--text) !important; }", ["declaration-no-important"]],
  ])("rejects %s", async (_name, code, expected) => {
    expect(await rulesFor(code)).toEqual(expected)
  })

  it.each([
    ["token reference", ".a {\n  color: var(--text);\n  background: transparent;\n}"],
    ["currentColor", ".a { border-color: currentcolor; }"],
    ["dynamic viewport", ".a { min-height: 100dvh; }"],
    ["tailwind theme", "@theme inline { --color-*: initial; --color-text: var(--text); }"],
  ])("accepts %s", async (_name, code) => {
    expect(await rulesFor(code)).toEqual([])
  })

  it("allows colour literals only in the token file", async () => {
    const code = ":root {\n  --text: oklch(24% 0.01 250deg);\n  --edge: #000;\n}"
    expect(await rulesFor(code, "src/styles/tokens.css")).toEqual([])
    expect(await rulesFor(code)).toEqual(["color-no-hex", "function-disallowed-list"])
  })
})
