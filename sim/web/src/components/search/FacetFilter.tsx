import { ChevronDown } from "lucide-react"
import { type ReactNode, useId, useState } from "react"
import { useTranslation } from "react-i18next"

import { Button } from "../ui/Button"

interface FacetOption {
  readonly code: string
  readonly label: string
  readonly fullWidth?: boolean
}

export function FacetCheckbox({
  label,
  content,
  checked,
  count,
  onChange,
}: {
  readonly label: string
  /** The visual (a badge image) shown in place of the label text; the label stays the name. */
  readonly content?: ReactNode
  readonly checked: boolean
  readonly count: number
  readonly onChange: () => void
}) {
  const countId = useId()
  return (
    <label className="flex min-h-11 cursor-pointer items-center gap-2 px-2 text-13 hover:bg-surface-2">
      <input
        type="checkbox"
        checked={checked}
        aria-label={label}
        aria-describedby={countId}
        onChange={onChange}
        className="size-4 shrink-0 accent-accent"
      />
      <span className="flex items-center">{content ?? label}</span>
      <span id={countId} className="ml-auto text-text-3 tabular-nums">
        {count}
      </span>
    </label>
  )
}

/** A dropdown checklist of flat values (the set filter); values with sub-values use FacetChips. */
export function FacetFilter({
  label,
  groups,
  counts,
  selected,
  onChange,
}: {
  readonly label: string
  readonly groups: readonly (readonly FacetOption[])[]
  readonly counts: Readonly<Record<string, number>> | undefined
  readonly selected: readonly string[]
  readonly onChange: (selected: string[]) => void
}) {
  const { t } = useTranslation()
  const [open, setOpen] = useState(false)
  const id = useId()
  const toggle = (code: string) => {
    onChange(
      selected.includes(code)
        ? selected.filter((value) => value !== code)
        : [...selected, code].sort(),
    )
  }
  return (
    <div>
      <Button
        size="sm"
        disabled={!counts}
        aria-expanded={open}
        aria-controls={id}
        onClick={() => {
          setOpen(!open)
        }}
      >
        {label}
        <span className="font-normal text-text-2">
          {selected.length
            ? t("rarityBadge.selected", { count: selected.length })
            : t("rarityBadge.all")}
        </span>
        <ChevronDown aria-hidden="true" className="size-4" />
      </Button>
      <div
        id={id}
        hidden={!open}
        className="mt-2 max-h-96 w-full max-w-xl overflow-y-auto rounded-control border border-border bg-surface-1 p-2"
      >
        <fieldset disabled={!counts}>
          <legend className="sr-only">{label}</legend>
          {groups
            .filter((items) => items.length > 0)
            .map((items, index) => (
              <div key={items.at(0)?.code ?? index}>
                {index > 0 && (
                  <hr className="my-2 border-0 border-t border-dashed border-border-strong" />
                )}
                <ul className="grid grid-cols-2 gap-x-4">
                  {items.map((item) => (
                    <li key={item.code} className={item.fullWidth ? "col-span-2" : undefined}>
                      <FacetCheckbox
                        label={item.label}
                        count={counts?.[item.code] ?? 0}
                        checked={selected.includes(item.code)}
                        onChange={() => {
                          toggle(item.code)
                        }}
                      />
                    </li>
                  ))}
                </ul>
              </div>
            ))}
        </fieldset>
      </div>
    </div>
  )
}
