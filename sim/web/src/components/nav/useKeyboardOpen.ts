import { useEffect, useState } from "react"

// The on-screen keyboard shrinks the visual viewport; a bottom bar pinned above it would cover
// the input, so anything fixed to the bottom hides while the keyboard is up.
export function useKeyboardOpen(): boolean {
  const [open, setOpen] = useState(false)
  useEffect(() => {
    const viewport = window.visualViewport
    if (!viewport) return
    const update = () => {
      setOpen(viewport.height < window.innerHeight * 0.75)
    }
    viewport.addEventListener("resize", update)
    update()
    return () => {
      viewport.removeEventListener("resize", update)
    }
  }, [])
  return open
}
