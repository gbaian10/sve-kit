// @vitest-environment node
import { expect, it } from "vitest"

import { popoverAlignment } from "./popover-placement"

it("hangs from the left edge while the panel fits, from the right edge near the viewport edge", () => {
  const panelWidth = 208
  const viewportWidth = 410
  expect(popoverAlignment({ anchorLeft: 29, anchorRight: 122, panelWidth, viewportWidth })).toBe(
    "start",
  )
  // Sol's reproduction: the PR chip at 241..334 in a 410px viewport (209px of room to the right).
  expect(popoverAlignment({ anchorLeft: 241, anchorRight: 334, panelWidth, viewportWidth })).toBe(
    "end",
  )
  // Exactly fitting counts as fitting on either side.
  expect(popoverAlignment({ anchorLeft: 202, anchorRight: 300, panelWidth, viewportWidth })).toBe(
    "start",
  )
  expect(popoverAlignment({ anchorLeft: 203, anchorRight: 208, panelWidth, viewportWidth })).toBe(
    "end",
  )
})

it("does not depend on where the panel currently is, so reopening gives the same answer", () => {
  const box = { anchorLeft: 241, anchorRight: 334, panelWidth: 208, viewportWidth: 410 }
  expect(popoverAlignment(box)).toBe(popoverAlignment({ ...box }))
  expect(popoverAlignment({ ...box, viewportWidth: 1440 })).toBe("start")
})

it("picks the side with more room when the panel fits neither way", () => {
  expect(
    popoverAlignment({ anchorLeft: 100, anchorRight: 150, panelWidth: 300, viewportWidth: 320 }),
  ).toBe("start")
  expect(
    popoverAlignment({ anchorLeft: 200, anchorRight: 250, panelWidth: 300, viewportWidth: 320 }),
  ).toBe("end")
})
