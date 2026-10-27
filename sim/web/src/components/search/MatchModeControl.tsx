import { useId } from "react"
import { useTranslation } from "react-i18next"

import type { MatchMode } from "../../domain/query/model"

const MODES = ["any", "all"] as const

export function MatchModeControl({
  label,
  value,
  disabled = false,
  onChange,
}: {
  readonly label: string
  readonly value: MatchMode
  readonly disabled?: boolean
  readonly onChange: (value: MatchMode) => void
}) {
  const { t } = useTranslation()
  const name = useId()
  return (
    <fieldset disabled={disabled} className="mb-3 flex gap-3 text-12 text-text-2">
      <legend className="sr-only">{label}</legend>
      {MODES.map((mode) => (
        <label key={mode} className="flex cursor-pointer items-center gap-1.5">
          <input
            type="radio"
            name={name}
            value={mode}
            checked={value === mode}
            onChange={() => {
              onChange(mode)
            }}
          />
          {t(mode === "any" ? "filters.matchAny" : "filters.matchAll")}
        </label>
      ))}
    </fieldset>
  )
}
