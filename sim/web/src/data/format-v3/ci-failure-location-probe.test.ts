import { expect, test } from "vitest"

test("CI failure location probe", function probeLocation() {
  const wording = ["synthetic failure probe", "ci-private-web-wording"].join(" ")
  expect(wording).toBe("synthetic comparison value")
})
