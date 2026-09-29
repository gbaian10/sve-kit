import { useEffect, useState } from "react"

/** The value once it has stayed unchanged for `delay` ms; clearing the text applies at once. */
export function useDebouncedValue(value: string, delay: number): string {
  const [settled, setSettled] = useState(value)
  useEffect(() => {
    const timer = window.setTimeout(
      () => {
        setSettled(value)
      },
      value === "" ? 0 : delay,
    )
    return () => {
      window.clearTimeout(timer)
    }
  }, [value, delay])
  return settled
}
