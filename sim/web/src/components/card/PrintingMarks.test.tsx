import { screen } from "@testing-library/react"
import { expect, it } from "vitest"

import { renderInRouter } from "../../test-utils"
import { PrintingMarks } from "./PrintingMarks"

it.each([
  {
    language: "zh-TW" as const,
    anniversary: "週年標誌",
    serial: "編號卡",
    unknown: "標誌種類未確認",
  },
  {
    language: "ja" as const,
    anniversary: "周年ロゴ",
    serial: "シリアルナンバー入りカード",
    unknown: "ロゴの種類未確認",
  },
  {
    language: "en" as const,
    anniversary: "Anniversary stamp",
    serial: "Numbered cards",
    unknown: "Unconfirmed stamp kind",
  },
])("displays authored mark labels in $language", async (copy) => {
  await renderInRouter(
    <>
      <PrintingMarks printing={{ stamp: { kind: "anniversary", year: "2025" }, serial: true }} />
      <PrintingMarks printing={{ stamp: { kind: "B.S.F", year: null }, serial: false }} />
      <PrintingMarks printing={{ stamp: { kind: null, year: null }, serial: null }} />
    </>,
    { language: copy.language },
  )
  expect(screen.getByText(`${copy.anniversary} · 2025`)).toBeInTheDocument()
  expect(screen.getByText(copy.serial)).toBeInTheDocument()
  expect(screen.getByText("B.S.F")).toBeInTheDocument()
  expect(screen.getByText(copy.unknown)).toBeInTheDocument()
})

it("renders nothing for an unmarked or not yet loaded printing", async () => {
  const { container } = await renderInRouter(
    <>
      <PrintingMarks printing={undefined} />
      <PrintingMarks printing={{ stamp: null, serial: false }} />
    </>,
  )
  expect(container.querySelector("[data-printing-marks]")).toBeNull()
})
