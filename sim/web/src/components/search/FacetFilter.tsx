import { ChevronDown } from "lucide-react"
import { Fragment, type ReactNode, useEffect, useId, useRef, useState } from "react"
import { useTranslation } from "react-i18next"

import { toggleFacet } from "../../domain/rarityFilter"
import { Button } from "../ui/Button"

interface FacetOption {
  readonly code: string
  readonly label: string
  readonly children?: readonly FacetOption[]
  readonly fullWidth?: boolean
}

export function FacetCheckbox({
  label,
  content,
  checked,
  mixed = false,
  count,
  onChange,
}: {
  readonly label: string
  /** The visual (a badge image) shown in place of the label text; the label stays the name. */
  readonly content?: ReactNode
  readonly checked: boolean
  readonly mixed?: boolean
  readonly count: number
  readonly onChange: () => void
}) {
  const ref = useRef<HTMLInputElement>(null)
  const countId = useId()
  useEffect(() => {
    if (ref.current) ref.current.indeterminate = mixed
  }, [mixed])
  return (
    <label className="flex min-h-11 cursor-pointer items-center gap-2 px-2 text-13 hover:bg-surface-2">
      <input
        ref={ref}
        type="checkbox"
        checked={checked}
        aria-checked={mixed ? "mixed" : checked}
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
  const toggle = (token: string) => {
    const parent = groups
      .flat()
      .find((item) => item.code === token || item.children?.some((child) => child.code === token))
    if (parent)
      onChange(
        toggleFacet(
          selected,
          token,
          parent.children?.map((child) => child.code) ?? [],
          parent.code,
        ),
      )
  }
  const checkbox = (item: FacetOption, checked: boolean, mixed = false) => (
    <FacetCheckbox
      label={item.label}
      count={counts?.[item.code] ?? 0}
      checked={checked}
      mixed={mixed}
      onChange={() => {
        toggle(item.code)
      }}
    />
  )
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
          {groups.map((items) => (
            <ul key={items[0]?.code} className="grid grid-cols-2 gap-x-4">
              {items.map((item) => {
                const children = item.children ?? []
                const all =
                  selected.includes(item.code) ||
                  (children.length > 0 && children.every((child) => selected.includes(child.code)))
                const some = children.some((child) => selected.includes(child.code))
                return (
                  <Fragment key={item.code}>
                    <li className={item.fullWidth ? "col-span-2" : undefined}>
                      {checkbox(item, all, !all && some)}
                    </li>
                    {children.length > 0 && (
                      <li className="col-span-2">
                        <ul
                          aria-label={item.label}
                          className="ml-4 grid grid-cols-2 gap-x-4 border-l border-border pl-2"
                        >
                          {children.map((child) => (
                            <li key={child.code}>
                              {checkbox(child, all || selected.includes(child.code))}
                            </li>
                          ))}
                        </ul>
                      </li>
                    )}
                  </Fragment>
                )
              })}
            </ul>
          ))}
        </fieldset>
      </div>
    </div>
  )
}
