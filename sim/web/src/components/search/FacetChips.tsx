import type { ReactNode } from "react"
import { useTranslation } from "react-i18next"

import { toggleFacet } from "../../domain/rarityFilter"
import { cn } from "../ui/cn"
import { FacetCheckbox } from "./FacetFilter"
import { Section, Toggle } from "./FilterControls"
import { FilterDropdown } from "./FilterDropdown"

export interface FacetChipOption {
  readonly code: string
  /** Accessible name and tooltip; `content` is the visual (a badge image), the label by default. */
  readonly label: string
  readonly content?: ReactNode
}

export interface FacetChip extends FacetChipOption {
  /** Sub-values behind a popover; the chip itself selects or clears all of them at once. */
  readonly children?: readonly FacetChipOption[]
}

/** A row of multi-select chips; a chip with sub-values gets an arrow that opens them in a popover. */
export function FacetChips({
  title,
  items,
  counts,
  selected,
  onChange,
  locationGrid = false,
}: {
  readonly title: string
  readonly locationGrid?: boolean
  readonly items: readonly FacetChip[]
  /** Printings per value; undefined while the index is still loading (chips are disabled). */
  readonly counts: Readonly<Record<string, number>> | undefined
  readonly selected: readonly string[]
  readonly onChange: (selected: string[]) => void
}) {
  const { t } = useTranslation()
  const content = (
    <div
      className={
        locationGrid
          ? "grid w-full grid-cols-3 gap-1.5 md:flex md:flex-wrap"
          : "flex flex-wrap gap-1.5"
      }
    >
      {items.map((item) => {
        const children = item.children ?? []
        const all =
          selected.includes(item.code) ||
          (children.length > 0 && children.every((child) => selected.includes(child.code)))
        const some = !all && children.some((child) => selected.includes(child.code))
        const toggle = (code: string) => {
          onChange(
            toggleFacet(
              selected,
              code,
              children.map((child) => child.code),
              item.code,
            ),
          )
        }
        const chip = (
          <Toggle
            pressed={all ? true : some ? "mixed" : false}
            disabled={!counts}
            label={item.label}
            title={t("filters.chipTitle", { label: item.label, count: counts?.[item.code] ?? 0 })}
            className={cn(
              children.length > 0 && "rounded-r-none",
              locationGrid && "max-md:h-11 max-md:min-w-0 max-md:flex-1 max-md:px-1",
            )}
            onClick={() => {
              toggle(item.code)
            }}
          >
            {item.content ?? item.label}
          </Toggle>
        )
        return (
          <span
            key={item.code}
            data-filter-chip=""
            className={cn(
              "inline-flex min-w-0",
              locationGrid && item.code === "main" && "col-span-3 justify-self-start",
            )}
          >
            {chip}
            {children.length > 0 && (
              <FilterDropdown
                label={t("filters.details", { label: item.label })}
                compact
                disabled={!counts}
              >
                {children.map((child) => (
                  <FacetCheckbox
                    key={child.code}
                    label={child.label}
                    content={child.content}
                    count={counts?.[child.code] ?? 0}
                    checked={all || selected.includes(child.code)}
                    onChange={() => {
                      toggle(child.code)
                    }}
                  />
                ))}
              </FilterDropdown>
            )}
          </span>
        )
      })}
    </div>
  )
  const clear =
    selected.length > 0
      ? () => {
          onChange([])
        }
      : undefined
  return (
    <Section title={title} onClear={clear}>
      {content}
    </Section>
  )
}
