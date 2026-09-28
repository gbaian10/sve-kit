import { screen } from "@testing-library/react"
import { describe, expect, it } from "vitest"

import { renderInRouter } from "../../test-utils"
import { Button } from "./Button"
import { Link } from "./Link"

// eslint.config.ts maps these components to native elements for jsx-a11y; this pins that mapping
// to what actually renders, so a refactor cannot silently break the lint coverage.
describe("jsx-a11y component mapping", () => {
  it("Button renders a real <button> with type=button by default", async () => {
    await renderInRouter(<Button>go</Button>)
    const button = screen.getByRole("button", { name: "go" })
    expect(button.tagName).toBe("BUTTON")
    expect(button).toHaveAttribute("type", "button")
  })

  it("Link renders a real <a> with an href", async () => {
    await renderInRouter(<Link to="/cards">cards</Link>)
    const link = screen.getByRole("link", { name: "cards" })
    expect(link.tagName).toBe("A")
    expect(link).toHaveAttribute("href", "/cards")
  })
})
