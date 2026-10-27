import { useEffect } from "react"

import { usePrefs } from "../settings"
import { applyThemeAttributes } from "./theme-apply"

export function useThemeAttributes(): void {
  const { theme, accent } = usePrefs()
  useEffect(() => {
    applyThemeAttributes({ theme, accent })
  }, [theme, accent])
}
