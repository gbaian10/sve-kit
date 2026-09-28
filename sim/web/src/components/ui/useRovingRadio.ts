import type { KeyboardEvent } from "react"

/**
 * Arrow-key movement for a radiogroup: one Tab stop (the checked item), arrows move and focus.
 * Each radio must have the id `${idPrefix}-${value}`.
 */
export function rovingRadioKeyDown<T extends string>(
  event: KeyboardEvent<HTMLElement>,
  values: readonly T[],
  value: T,
  onChange: (value: T) => void,
  idPrefix: string,
): void {
  const index = values.indexOf(value)
  let next = index
  if (event.key === "ArrowRight" || event.key === "ArrowDown") next = (index + 1) % values.length
  if (event.key === "ArrowLeft" || event.key === "ArrowUp")
    next = (index - 1 + values.length) % values.length
  if (event.key === "Home") next = 0
  if (event.key === "End") next = values.length - 1
  if (next === index) return
  event.preventDefault()
  const target = values[next]
  if (target === undefined) return
  onChange(target)
  document.getElementById(`${idPrefix}-${target}`)?.focus()
}
