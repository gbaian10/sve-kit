import { render } from "@testing-library/react"
import { afterEach, expect, it } from "vitest"

import { textDiff } from "../../data"
import { DEFAULT_PREFS, prefsStore } from "../../settings"
import { InlineText } from "./InlineText"

afterEach(() => prefsStore.set(DEFAULT_PREFS))

it("marks replacements inside one emphasis group without duplicating icons", () => {
  const before = "【Combo {コスト1} 3】Do alpha. Keep beta."
  const after = "【Combo {コスト2} 5】Do omega. Keep beta."
  const diff = textDiff(before, after)
  const { container } = render(
    <>
      <div data-side="before">
        <InlineText text={before} emphasis changes={{ kind: "removed", ranges: diff.removed }} />
      </div>
      <div data-side="after">
        <InlineText text={after} emphasis changes={{ kind: "added", ranges: diff.added }} />
      </div>
    </>,
  )
  const old = container.querySelector('[data-side="before"]')
  const current = container.querySelector('[data-side="after"]')
  expect(old?.querySelectorAll("strong")).toHaveLength(1)
  expect(current?.querySelectorAll("strong")).toHaveLength(1)
  expect(old?.querySelectorAll("img")).toHaveLength(1)
  expect(current?.querySelectorAll("img")).toHaveLength(1)
  expect(old?.querySelector("strong del img")).toHaveAttribute("alt", "コスト1")
  expect(current?.querySelector("strong ins img")).toHaveAttribute("alt", "コスト2")
  expect(old?.querySelector("strong")).toHaveTextContent("【Combo 3】")
  expect(current?.querySelector("strong")).toHaveTextContent("【Combo 5】")
  expect(
    [...container.querySelectorAll("del, ins")].some((node) =>
      node.textContent.includes("Keep beta"),
    ),
  ).toBe(false)
})

it("preserves reading notation once and keeps marks when emphasis is disabled", () => {
  const before = "【漢字（かんじ）】\uFFF9漢字\uFFFAかんじ\uFFFB"
  const after = "【漢字（かんじ）】\uFFF9漢字\uFFFAかんじ\uFFFB。"
  prefsStore.set({ termEmphasis: false })
  const { container } = render(
    <InlineText
      text={after}
      emphasis
      changes={{ kind: "added", ranges: textDiff(before, after).added }}
    />,
  )
  expect(container.textContent).toBe(after)
  expect(container.querySelector("strong")).toBeNull()
  expect(container.querySelectorAll("ins")).toHaveLength(1)
  expect(container.querySelector("ins")).toHaveTextContent("。")
})
