import { render, screen } from "@testing-library/react"
import { expect, it, vi } from "vitest"

import { DEFAULT_QUERY } from "../../domain/query/model"
import { SortControls } from "./SortControls"

vi.mock("react-i18next", () => ({ useTranslation: () => ({ t: (key: string) => key }) }))

it("explains the disabled direction when no secondary sort is selected", () => {
  render(<SortControls query={{ ...DEFAULT_QUERY, secondarySort: null }} onChange={vi.fn()} />)

  const select = screen.getByRole("combobox", { name: "results.thenBy" })
  const direction = screen.getByRole("button", { name: "results.thenBy" })
  expect(direction).toBeDisabled()
  expect(direction).toHaveTextContent("results.noSecondary")
  expect(direction.parentElement).toHaveClass("gap-2")
  expect(select).toHaveClass("w-auto")
})

it("shows the direction as a button with its full meaning", () => {
  render(<SortControls query={DEFAULT_QUERY} onChange={vi.fn()} />)
  const direction = screen.getByRole("button", { name: "results.sort：results.newestFirst" })
  expect(direction).toHaveTextContent("results.newestFirst ↓")
  expect(direction).toHaveAttribute("title", "results.newestFirst")
  expect(screen.getByRole("combobox", { name: "results.sort" })).toHaveValue("set")
})
