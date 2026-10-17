import { ChevronDown } from "lucide-react"
import { useEffect, useId, useRef, useState } from "react"
import { useTranslation } from "react-i18next"

import { BADGE_STYLES } from "../../domain/rarityBadges"
import { toggleRarity, UNRATED_RARITY } from "../../domain/rarityFilter"
import { Button } from "../ui/Button"

interface RarityOption {
  readonly code: string
  readonly label: string
}

function RarityCheckbox({
  label,
  checked,
  mixed = false,
  count,
  onChange,
}: {
  readonly label: string
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
      <span>{label}</span>
      <span id={countId} className="ml-auto text-text-3 tabular-nums">
        {count}
      </span>
    </label>
  )
}

export function RarityFilter({
  rarities,
  options,
  selected,
  onChange,
}: {
  readonly rarities: readonly RarityOption[]
  readonly options:
    | { readonly styles: readonly string[]; readonly counts: Readonly<Record<string, number>> }
    | undefined
  readonly selected: readonly string[]
  readonly onChange: (selected: string[]) => void
}) {
  const { t } = useTranslation()
  const [open, setOpen] = useState(false)
  const id = useId()
  const styles = ([...BADGE_STYLES, "unknown"] as const).filter((style) =>
    options?.styles.includes(style),
  )
  const children = styles.map((style) => `pr:${style}`)
  const allPr =
    selected.includes("pr") ||
    (children.length > 0 && children.every((child) => selected.includes(child)))
  const somePr = selected.some((token) => token.startsWith("pr:"))
  const toggle = (token: string) => {
    onChange(toggleRarity(selected, token, children))
  }
  const checkbox = (code: string, label: string) => (
    <RarityCheckbox
      label={label}
      count={options?.counts[code] ?? 0}
      checked={code === "pr" ? allPr : selected.includes(code)}
      mixed={code === "pr" && !allPr && somePr}
      onChange={() => {
        toggle(code)
      }}
    />
  )
  return (
    <div>
      <Button
        size="sm"
        disabled={!options}
        aria-expanded={open}
        aria-controls={id}
        onClick={() => {
          setOpen(!open)
        }}
      >
        {t("rarityBadge.label")}
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
        <fieldset disabled={!options}>
          <legend className="sr-only">{t("rarityBadge.label")}</legend>
          <ul className="grid grid-cols-2 gap-x-4">
            {rarities.map(({ code, label }) => (
              <li key={code} className={code === "pr" ? "col-span-2" : undefined}>
                {checkbox(code, label)}
                {code === "pr" && children.length > 0 && (
                  <ul className="ml-4 grid grid-cols-2 gap-x-4 border-l border-border pl-2">
                    {styles.map((style) => (
                      <li key={style}>
                        <RarityCheckbox
                          label={t("rarityBadge.prStyle", { style: t(`rarityBadge.${style}`) })}
                          count={options?.counts[`pr:${style}`] ?? 0}
                          checked={allPr || selected.includes(`pr:${style}`)}
                          onChange={() => {
                            toggle(`pr:${style}`)
                          }}
                        />
                      </li>
                    ))}
                  </ul>
                )}
              </li>
            ))}
            <li>{checkbox(UNRATED_RARITY, t("rarityBadge.unrated"))}</li>
          </ul>
        </fieldset>
      </div>
    </div>
  )
}
