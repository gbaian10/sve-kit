import { ListTree } from "lucide-react"
import { useEffect, useRef, useState } from "react"
import { useTranslation } from "react-i18next"
import { useLocation, useNavigate } from "react-router"

import { dialogInHistory, useDialogHistory } from "../../app/dialog-history"
import { ruleAnchor } from "../../components/rules/rule-keys"
import { RuleText } from "../../components/rules/RuleText"
import { cn } from "../../components/ui/cn"
import { Dialog } from "../../components/ui/Dialog"
import type { RuleContext } from "../../data"

export interface TocEntry {
  readonly key: string
  readonly number?: string | undefined
  readonly title: string
  readonly context: RuleContext
  /** The title is shown in Japanese (untranslated, or the UI shows originals). */
  readonly original: boolean
  readonly children: readonly TocEntry[]
}

function TocList({
  entries,
  active,
  onJump,
  touch = false,
}: {
  readonly entries: readonly TocEntry[]
  readonly active: string | undefined
  readonly onJump: (key: string) => void
  /** The phone sheet: every row is a full 44px touch target; the desktop list stays compact. */
  readonly touch?: boolean
}) {
  const item = (entry: TocEntry, nested: boolean) => {
    const id = ruleAnchor(entry.key)
    const current =
      id === active || (!nested && entry.children.some((child) => ruleAnchor(child.key) === active))
    return (
      <a
        href={`#${id}`}
        aria-current={id === active ? "location" : undefined}
        onClick={(event) => {
          event.preventDefault()
          onJump(entry.key)
        }}
        className={cn(
          "flex items-baseline gap-2 rounded-button px-2 leading-snug hover:bg-surface-2 hover:text-text-1",
          touch ? "min-h-11 py-2.5" : "min-h-8 py-1",
          current ? "bg-accent-soft font-semibold text-accent-text" : "text-text-2",
        )}
      >
        {entry.number && (
          <span className="shrink-0 font-mono text-11 tabular-nums">{entry.number}</span>
        )}
        <span lang={entry.original ? "ja" : undefined} className="min-w-0">
          <RuleText text={entry.title} context={entry.context} className="inline [&>p]:inline" />
        </span>
      </a>
    )
  }
  return (
    <ol className="flex flex-col gap-0.5 text-13">
      {entries.map((entry) => (
        <li key={entry.key}>
          {item(entry, false)}
          {entry.children.length > 0 && (
            <ol className="mt-0.5 ml-3 flex flex-col gap-0.5 border-l border-dashed border-border pl-2">
              {entry.children.map((child) => (
                <li key={child.key}>{item(child, true)}</li>
              ))}
            </ol>
          )}
        </li>
      ))}
    </ol>
  )
}

const DRAWER = "rules-toc"

/** Fixed table of contents beside the rules from `lg`; a bottom sheet behind a button below that. */
export function RulesToc({
  entries,
  active,
}: {
  readonly entries: readonly TocEntry[]
  readonly active: string | undefined
}) {
  const { t } = useTranslation()
  const navigate = useNavigate()
  const location = useLocation()
  const [open, setOpen] = useState(false)
  const pending = useRef<string | null>(null)
  const close = useDialogHistory(DRAWER, open, () => {
    setOpen(false)
  })
  const jump = (key: string) => {
    void navigate({ hash: `#${ruleAnchor(key)}` }, { replace: true })
  }
  // A jump from the sheet waits until closing has popped the sheet's history entry, so the new
  // hash lands on the page's own entry instead of the one being removed.
  useEffect(() => {
    const key = pending.current
    if (open || key === null || dialogInHistory(location.state, DRAWER)) return
    pending.current = null
    void navigate({ hash: `#${ruleAnchor(key)}` }, { replace: true })
  }, [open, location, navigate])
  const sheetRef = useRef<HTMLDivElement>(null)
  // The sheet opens at the reader's place, not at chapter 1.
  useEffect(() => {
    if (!open) return
    const frame = requestAnimationFrame(() => {
      sheetRef.current
        ?.querySelector<HTMLElement>('[aria-current="location"]')
        ?.scrollIntoView({ block: "center" })
    })
    return () => {
      cancelAnimationFrame(frame)
    }
  }, [open])
  const asideRef = useRef<HTMLElement>(null)
  // Keep the current entry visible inside the TOC's own scroll area without moving the page.
  useEffect(() => {
    const aside = asideRef.current
    const link = aside?.querySelector<HTMLElement>('[aria-current="location"]')
    if (!aside || !link) return
    const top = link.getBoundingClientRect().top - aside.getBoundingClientRect().top
    if (top < 0 || top > aside.clientHeight - link.offsetHeight)
      aside.scrollTop += top - aside.clientHeight / 3
  }, [active])
  return (
    <>
      <nav
        ref={asideRef}
        aria-label={t("rules.toc")}
        className="sticky top-20 hidden max-h-[calc(100dvh-6rem)] overflow-y-auto border-l border-border pl-3 lg:block"
      >
        <h2 className="mb-2 px-2 text-12 font-semibold tracking-[0.12em] text-text-3">
          {t("rules.toc")}
        </h2>
        <TocList entries={entries} active={active} onJump={jump} />
      </nav>
      <button
        type="button"
        onClick={() => {
          setOpen(true)
        }}
        className="fixed right-4 bottom-[calc(6rem+env(safe-area-inset-bottom))] z-30 inline-flex min-h-12 items-center gap-2 rounded-full border border-border bg-surface-1 px-4 text-14 font-semibold text-text-1 shadow-lg lg:hidden phone-landscape:bottom-4"
      >
        <ListTree aria-hidden="true" className="size-5" />
        {t("rules.toc")}
      </button>
      <Dialog open={open} onClose={close} title={t("rules.toc")} variant="bottom">
        <div ref={sheetRef}>
          <TocList
            entries={entries}
            active={active}
            touch
            onJump={(key) => {
              pending.current = key
              close()
            }}
          />
        </div>
      </Dialog>
    </>
  )
}
