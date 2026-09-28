import { describe, expect, it } from "vitest"

import { formatDataVersion, parseDataVersion } from "./dataVersion"

describe("data version display", () => {
  it("shows the date, and the serial only for a second batch on the same day", () => {
    expect(formatDataVersion("20260929T000000Z-0001")).toBe("2026-09-29")
    expect(formatDataVersion("20260929T153000Z-0002")).toBe("2026-09-29 #2")
    expect(formatDataVersion("preview-20260929T010203Z-0001")).toBe("2026-09-29")
    expect(parseDataVersion("preview-20260929T010203Z-0003")).toEqual({
      preview: true,
      date: "2026-09-29",
      serial: 3,
    })
  })

  it("leaves anything else untouched", () => {
    expect(formatDataVersion("synthetic")).toBe("synthetic")
    expect(parseDataVersion("2026-09-29")).toBeNull()
  })
})
