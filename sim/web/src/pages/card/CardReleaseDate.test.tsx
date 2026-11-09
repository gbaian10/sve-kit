import { screen } from "@testing-library/react"
import { expect, it } from "vitest"

import { renderInRouter } from "../../test-utils"
import { CardReleaseDate } from "./CardReleaseDate"

it.each(["2025-09-24", "2025-09"])(
  "shows release date %s without inventing precision",
  async (date) => {
    const { container } = await renderInRouter(<CardReleaseDate date={date} />)
    const time = container.querySelector("time")
    expect(time).toHaveTextContent(date)
    expect(time).toHaveAttribute("datetime", date)
    expect(screen.getByText(/發行日/)).toBeInTheDocument()
    if (date.length === 7) expect(container).not.toHaveTextContent("2025-09-01")
  },
)

it("hides an unknown release date", async () => {
  const { container } = await renderInRouter(<CardReleaseDate date={undefined} />)
  expect(container.querySelector("time")).toBeNull()
  expect(screen.queryByText(/發行日/)).not.toBeInTheDocument()
})
