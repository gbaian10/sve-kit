import { describe, expect, it } from "vitest"

import { classStyle, quickBarClasses } from "./classes"

describe("classes", () => {
  it("maps vocabulary codes to the design's class families, unknown ones to neutral", () => {
    expect(classStyle("elf")).toBe("forest")
    expect(classStyle("bishop")).toBe("haven")
    expect(classStyle("nemesis")).toBe("neutral")
    expect(classStyle(null)).toBe("neutral")
  })

  it("orders the quick bar by the game's class order with neutral last", () => {
    expect(
      quickBarClasses(["bishop", "dragon", "elf", "nemesis", "nightmare", "royal", "witch"]),
    ).toEqual(["elf", "royal", "witch", "dragon", "nightmare", "bishop", "nemesis", "neutral"])
    expect(quickBarClasses(["zz_new", "elf"])).toEqual(["elf", "zz_new", "neutral"])
  })
})
