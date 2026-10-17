import { screen } from "@testing-library/react"
import userEvent from "@testing-library/user-event"
import { useRef, useState } from "react"
import { expect, it } from "vitest"

import { renderInRouter } from "../../test-utils"
import { SearchBar } from "./SearchBar"

function Harness() {
  const [filtersOpen, setFiltersOpen] = useState(false)
  const inputRef = useRef<HTMLInputElement | null>(null)
  return (
    <SearchBar
      value=""
      onChange={() => {}}
      onKeyDown={() => {}}
      onFocus={() => {}}
      onBlur={() => {}}
      inputRef={inputRef}
      listId="suggestions"
      expanded={false}
      filterCount={3}
      filtersOpen={filtersOpen}
      filtersId="filter-panel"
      onToggleFilters={() => {
        setFiltersOpen(!filtersOpen)
      }}
      summary={<p className="min-w-0 text-right">7,369 個版次</p>}
    />
  )
}

it("shows the filter label and reflects its open state with the accent style", async () => {
  await renderInRouter(<Harness />)
  const button = screen.getByRole("button", { name: "篩選" })

  expect(button).toHaveTextContent("篩選")
  expect(button).toHaveAttribute("aria-expanded", "false")
  expect(button).toHaveAttribute("aria-controls", "filter-panel")
  expect(button).toHaveClass("bg-surface-1", "border-border")
  expect(document.getElementById(button.getAttribute("aria-describedby") ?? "")).toHaveTextContent(
    "已啟用 3 個篩選條件",
  )

  await userEvent.click(button)
  expect(button).toHaveAttribute("aria-expanded", "true")
  expect(button).toHaveClass("bg-accent", "text-accent-ink", "border-accent")

  await userEvent.click(button)
  expect(button).toHaveAttribute("aria-expanded", "false")
  expect(button).toHaveClass("bg-surface-1", "border-border")
})

it.each([
  { language: "ja" as const, label: "絞り込み" },
  { language: "en" as const, label: "Filters" },
])("shows the $language filter label", async ({ language, label }) => {
  await renderInRouter(<Harness />, { language })
  expect(screen.getByRole("button", { name: label })).toHaveTextContent(label)
})
