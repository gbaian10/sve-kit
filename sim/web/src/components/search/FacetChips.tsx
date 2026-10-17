import { ChevronDown } from "lucide-react"
import {
  Fragment,
  type ReactNode,
  useEffect,
  useId,
  useLayoutEffect,
  useRef,
  useState,
} from "react"
import { useTranslation } from "react-i18next"

import { toggleFacet } from "../../domain/rarityFilter"
import { cn } from "../ui/cn"
import { FacetCheckbox } from "./FacetFilter"
import { Section, Toggle } from "./FilterControls"
import { popoverAlignment } from "./popover-placement"

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

// The sub-values float over the content instead of pushing it (the maintainer's choice), and open
// on a click so phones can use them. Light dismiss and Escape are done here because the native
// popover attribute has no test support yet.
function ChipPopover({
  label,
  chip,
  disabled,
  children,
}: {
  readonly label: string
  readonly chip: ReactNode
  readonly disabled: boolean
  readonly children: ReactNode
}) {
  const { t } = useTranslation()
  const [open, setOpen] = useState(false)
  const [alignEnd, setAlignEnd] = useState(false)
  const root = useRef<HTMLDivElement>(null)
  const button = useRef<HTMLButtonElement>(null)
  const panel = useRef<HTMLDivElement>(null)
  const id = useId()
  useEffect(() => {
    if (!open) return
    const onPointerDown = (event: PointerEvent) => {
      if (event.target instanceof Node && !root.current?.contains(event.target)) setOpen(false)
    }
    const onKeyDown = (event: KeyboardEvent) => {
      if (event.key !== "Escape") return
      setOpen(false)
      button.current?.focus()
    }
    document.addEventListener("pointerdown", onPointerDown)
    document.addEventListener("keydown", onKeyDown)
    return () => {
      document.removeEventListener("pointerdown", onPointerDown)
      document.removeEventListener("keydown", onKeyDown)
    }
  }, [open])
  // A chip near the right edge opens its popover leftwards so it stays inside the viewport. The
  // side is derived from the anchor (a panel already hung from one side would always "fit"), and
  // redone whenever the viewport or the chip row's wrapping changes while it is open.
  useLayoutEffect(() => {
    if (!open || !root.current || !panel.current) return
    const anchor = root.current
    const place = () => {
      const rect = anchor.getBoundingClientRect()
      setAlignEnd(
        popoverAlignment({
          anchorLeft: rect.left,
          anchorRight: rect.right,
          panelWidth: panel.current?.offsetWidth ?? 0,
          viewportWidth: document.documentElement.clientWidth,
        }) === "end",
      )
    }
    place()
    panel.current.querySelector<HTMLElement>("input")?.focus()
    window.addEventListener("resize", place)
    const observer = new ResizeObserver(place)
    if (anchor.parentElement) observer.observe(anchor.parentElement)
    return () => {
      window.removeEventListener("resize", place)
      observer.disconnect()
    }
  }, [open])
  return (
    <div ref={root} className="relative inline-flex">
      {chip}
      <button
        ref={button}
        type="button"
        disabled={disabled}
        aria-label={t("filters.details", { label })}
        aria-expanded={open}
        aria-controls={id}
        onClick={() => {
          setOpen(!open)
        }}
        className={cn(
          "flex h-9 w-7 items-center justify-center rounded-r-control border border-l-0 border-border bg-surface-1 text-text-2 hover:bg-surface-2",
          "disabled:cursor-not-allowed disabled:text-text-3",
        )}
      >
        <ChevronDown aria-hidden="true" className={cn("size-4", open && "rotate-180")} />
      </button>
      <div
        ref={panel}
        id={id}
        role="group"
        aria-label={label}
        hidden={!open}
        className={cn(
          "absolute top-full z-30 mt-1 min-w-52 rounded-control border border-border bg-surface-1 p-1 shadow-lg",
          alignEnd ? "right-0" : "left-0",
        )}
      >
        {children}
      </div>
    </div>
  )
}

/** A row of multi-select chips; a chip with sub-values gets an arrow that opens them in a popover. */
export function FacetChips({
  title,
  items,
  counts,
  selected,
  onChange,
}: {
  readonly title: string
  readonly items: readonly FacetChip[]
  /** Printings per value; undefined while the index is still loading (chips are disabled). */
  readonly counts: Readonly<Record<string, number>> | undefined
  readonly selected: readonly string[]
  readonly onChange: (selected: string[]) => void
}) {
  const { t } = useTranslation()
  return (
    <Section
      title={title}
      onClear={
        selected.length > 0
          ? () => {
              onChange([])
            }
          : undefined
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
            className={children.length > 0 ? "rounded-r-none" : undefined}
            onClick={() => {
              toggle(item.code)
            }}
          >
            {item.content ?? item.label}
          </Toggle>
        )
        if (children.length === 0) return <Fragment key={item.code}>{chip}</Fragment>
        return (
          <ChipPopover key={item.code} label={item.label} chip={chip} disabled={!counts}>
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
          </ChipPopover>
        )
      })}
    </Section>
  )
}
