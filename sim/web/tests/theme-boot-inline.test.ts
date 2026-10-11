// @vitest-environment node
import { readFileSync } from "node:fs"
import path from "node:path"

import { describe, expect, it } from "vitest"

import { inlineThemeBoot } from "../vite.config"

const root = path.join(import.meta.dirname, "..")
const read = (file: string) => readFileSync(path.join(root, file), "utf8")

function transform(html: string): string {
  const hook = inlineThemeBoot().transformIndexHtml
  if (typeof hook !== "function") throw new Error("expected a plain transformIndexHtml hook")
  return hook.call({} as never, html, {} as never) as string
}

describe("inlineThemeBoot", () => {
  it("puts the tested boot script into index.html instead of loading it", () => {
    const html = transform(read("index.html"))
    expect(html).not.toContain('src="/theme-boot.js"')
    expect(html).toContain(`<script>${read("public/theme-boot.js")}</script>`)
  })

  it("fails the build when index.html stops loading the boot script", () => {
    expect(() => transform("<html><head></head></html>")).toThrow("theme-boot.js")
  })
})
