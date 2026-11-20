import { createContext } from "react"

export const CardReportContext = createContext<(() => void) | null>(null)
