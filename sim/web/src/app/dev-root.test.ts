import { describe, expect, it } from "vitest"

import { initialSnapshotRoot } from "./dev-root"

describe("initialSnapshotRoot", () => {
  it("starts on the preview root only in dev with a preview and a fixture /cdn", () => {
    expect(
      initialSnapshotRoot({ DEV: true, SVE_PREVIEW_CONFIGURED: "1", SVE_CDN_IS_FIXTURE: "1" }),
    ).toBe("preview")
  })

  it.each([
    { DEV: false, SVE_PREVIEW_CONFIGURED: "1", SVE_CDN_IS_FIXTURE: "1" },
    { DEV: true, SVE_PREVIEW_CONFIGURED: "", SVE_CDN_IS_FIXTURE: "1" },
    { DEV: true, SVE_PREVIEW_CONFIGURED: "1", SVE_CDN_IS_FIXTURE: "" },
    { DEV: true },
  ])("keeps the formal root otherwise: %o", (env) => {
    expect(initialSnapshotRoot(env)).toBe("cdn")
  })
})
