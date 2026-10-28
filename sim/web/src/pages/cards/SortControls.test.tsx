import { render, screen } from "@testing-library/react"
import { expect, it, vi } from "vitest"

import { DEFAULT_QUERY } from "../../domain/query/model"
import { SortControls } from "./SortControls"

vi.mock("react-i18next", () => ({ useTranslation: () => ({ t: (key: string) => key }) }))

it("keeps an empty secondary direction slot when no secondary sort is selected", () => {
  render(<SortControls query={{ ...DEFAULT_QUERY, secondarySort: null }} onChange={vi.fn()} />)

  const select = screen.getByRole("combobox", { name: "results.thenBy" })
  const direction = screen.getByRole("button", { name: "results.thenBy" })
  expect(direction).toBeDisabled()
  expect(direction).toBeEmptyDOMElement()
  expect(direction.parentElement).toHaveClass("grid-cols-[3rem_minmax(0,1fr)_minmax(0,1fr)]")
  expect(select).toHaveClass("md:w-24")
})
