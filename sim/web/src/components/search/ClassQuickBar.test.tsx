import { render, screen } from "@testing-library/react"
import userEvent from "@testing-library/user-event"
import { describe, expect, it, vi } from "vitest"

import { ClassQuickBar } from "./ClassQuickBar"

const classes = [
  { code: "elf", label: "精靈" },
  { code: "royal", label: "皇家" },
  { code: "zz_future", label: "未來職業" },
  { code: "neutral", label: "中立" },
]

describe("ClassQuickBar", () => {
  it("shows labels only when the measured labelled row fits", () => {
    const { rerender } = render(
      <ClassQuickBar
        label="職業"
        classes={classes}
        selected={[]}
        onToggle={() => undefined}
        fits={() => false}
      />,
    )
    const group = screen.getByRole("group", { name: "職業" })
    expect(group.querySelectorAll("button[aria-pressed]")).toHaveLength(4)
    expect(screen.getByRole("button", { name: "精靈" })).toHaveAttribute("aria-label", "精靈")
    expect(screen.getByRole("button", { name: "精靈" })).not.toHaveTextContent("精靈")
    rerender(
      <ClassQuickBar
        label="職業"
        classes={classes}
        selected={[]}
        onToggle={() => undefined}
        fits={() => true}
      />,
    )
    expect(screen.getByRole("button", { name: "精靈" })).toHaveTextContent("精靈")
  })

  it("marks the selected classes and toggles on click", async () => {
    const onToggle = vi.fn()
    render(
      <ClassQuickBar
        label="職業"
        classes={classes}
        selected={["royal"]}
        onToggle={onToggle}
        fits={() => true}
      />,
    )
    expect(screen.getByRole("button", { name: "皇家" })).toHaveAttribute("aria-pressed", "true")
    expect(screen.getByRole("button", { name: "精靈" })).toHaveAttribute("aria-pressed", "false")
    await userEvent.click(screen.getByRole("button", { name: "中立" }))
    expect(onToggle).toHaveBeenCalledWith("neutral")
  })

  it("draws an initial for a class the design has no icon for", () => {
    render(
      <ClassQuickBar
        label="職業"
        classes={classes}
        selected={[]}
        onToggle={() => undefined}
        fits={() => false}
      />,
    )
    const future = screen.getByRole("button", { name: "未來職業" })
    expect(future.querySelector("img")).toBeNull()
    expect(future).toHaveTextContent("未")
    expect(screen.getByRole("button", { name: "精靈" }).querySelector("img")).not.toBeNull()
  })
})
