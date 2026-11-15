import { useEffect, useMemo, useState } from "react"
import { useTranslation } from "react-i18next"
import { useLocation, useNavigate } from "react-router"

import { ruleAnchor, RuleKeysContext, RULES_LANG } from "../../components/rules/rule-keys"
import { RuleText } from "../../components/rules/RuleText"
import { cn } from "../../components/ui/cn"
import { useToast } from "../../components/ui/toast-context"
import {
  loadComprehensiveRules,
  type LocalizedNode,
  type LocalizedText,
  localizeNode,
  originalNode,
  type RulesBundle,
  type RulesRegion,
  rulesRegionFor,
} from "../../data"
import { currentUiLanguage } from "../../i18n"
import { usePrefs } from "../../settings"
import { RulesToc, type TocEntry } from "./RulesToc"
import { useActiveSection } from "./use-active-section"

type Load =
  | { readonly state: "loading" }
  | { readonly state: "failed" }
  | { readonly state: "ready"; readonly rules: RulesBundle }

function useRules(region: RulesRegion): Load {
  // Tagged with its edition, so switching the UI language shows loading until the other arrives.
  const [load, setLoad] = useState<{ readonly region: RulesRegion; readonly load: Load }>()
  useEffect(() => {
    let active = true
    loadComprehensiveRules(region)
      .then((rules) => {
        if (active) setLoad({ region, load: { state: "ready", rules } })
      })
      .catch(() => {
        if (active) setLoad({ region, load: { state: "failed" } })
      })
    return () => {
      active = false
    }
  }, [region])
  return load?.region === region ? load.load : { state: "loading" }
}

const FLASH_MS = 1500

/** Chapters (body and appendices) with their sections, and appendix B's two formats. */
function tableOfContents(
  views: readonly LocalizedNode[],
  original: boolean,
  originalLang: string,
): TocEntry[] {
  const entries: { entry: TocEntry; children: TocEntry[] }[] = []
  for (const view of views) {
    const { node } = view
    const top = node.kind === "chapter"
    const child =
      (node.context === "main" && node.kind === "section") ||
      (node.kind === "section" && node.key === node.context)
    if (!top && !child) continue
    const children: TocEntry[] = []
    const entry: TocEntry = {
      key: node.key,
      number: node.number,
      title: view.title?.text ?? "",
      context: node.context,
      lang: original || !(view.title?.translated ?? true) ? originalLang : undefined,
      children,
    }
    if (top) entries.push({ entry, children })
    else entries.at(-1)?.children.push(entry)
  }
  return entries.map(({ entry }) => entry)
}

/** Scrolls to the hash target once it exists and marks it briefly (`data-flash`). */
function useHashTarget(ready: boolean) {
  const { hash } = useLocation()
  useEffect(() => {
    if (!ready || !hash) return
    let id: string
    try {
      id = decodeURIComponent(hash.slice(1))
    } catch {
      // A malformed escape ("#%") names no clause; leave the page as it is.
      return
    }
    const element = document.getElementById(id)
    if (!element) return
    const reduce = window.matchMedia("(prefers-reduced-motion: reduce)").matches
    element.scrollIntoView({ block: "start", behavior: reduce ? "auto" : "smooth" })
    element.dataset["flash"] = ""
    const timer = window.setTimeout(() => {
      delete element.dataset["flash"]
    }, FLASH_MS)
    return () => {
      window.clearTimeout(timer)
      delete element.dataset["flash"]
    }
  }, [hash, ready])
}

function RuleNodeView({
  view,
  original,
  originalLang,
  showPendingTag,
  onCopy,
}: {
  readonly view: LocalizedNode
  /** The view shows the original edition as a whole (the ja and en UIs). */
  readonly original: boolean
  readonly originalLang: string
  readonly showPendingTag: boolean
  readonly onCopy: (key: string, number: string) => void
}) {
  const { t } = useTranslation()
  const { node } = view
  const id = ruleAnchor(node.key)
  const lang = (translated: boolean) => (original || !translated ? originalLang : undefined)
  const number = node.number
  const numberButton = number && (
    <button
      type="button"
      onClick={() => {
        onCopy(node.key, number)
      }}
      aria-label={t("rules.copyLink", { number })}
      className="-mt-2.5 inline-flex min-h-11 min-w-11 shrink-0 items-start pt-3 font-mono text-12 text-text-3 tabular-nums hover:text-accent-text"
    >
      {number}
    </button>
  )
  const pendingTag = showPendingTag && view.pending && (
    <span className="ml-2 rounded-sm bg-surface-2 px-1.5 text-11 font-normal text-text-3">
      {t("rules.originalTag")}
    </span>
  )
  const flashClass = "data-[flash]:bg-accent-soft"
  if (node.kind === "chapter" || node.kind === "section" || node.kind === "block") {
    const Heading = node.kind === "chapter" ? "h2" : "h3"
    return (
      <div id={id} className={cn("scroll-mt-20 rounded-card transition-colors", flashClass)}>
        <Heading
          className={cn(
            "flex items-baseline gap-3 font-bold text-text-1",
            node.kind === "chapter" ? "mt-10 border-b border-border pb-2 text-20" : "mt-6 text-16",
          )}
        >
          {numberButton}
          {view.title && (
            <span lang={lang(view.title.translated)}>
              <RuleText
                text={view.title.text}
                context={node.context}
                className="inline [&>p]:inline"
              />
            </span>
          )}
          {pendingTag}
        </Heading>
        {node.figure && (
          <p className="mt-2 text-13 text-text-2">
            {t("rules.figure", { page: node.figure.page })}
          </p>
        )}
        {view.text.text && (
          <div lang={lang(view.text.translated)}>
            <RuleText
              text={view.text.text}
              context={node.context}
              className="mt-2 max-w-[66ch] text-15 text-text-1"
            />
          </div>
        )}
        {view.table && <RuleTable view={view} lang={lang} markPending={showPendingTag} />}
      </div>
    )
  }
  const indent = Math.min(Math.max(node.depth - 3, 0), 4)
  return (
    <div
      id={id}
      className={cn(
        "mt-1.5 grid scroll-mt-20 grid-cols-[auto_minmax(0,1fr)] gap-x-3 rounded-button px-1.5 py-0.5 transition-colors",
        flashClass,
      )}
      style={{ marginLeft: `${String(indent * 0.75)}rem` }}
    >
      {numberButton ?? <span />}
      <div className="max-w-[66ch] min-w-0 text-15 text-text-1">
        <div lang={lang(view.text.translated)}>
          <RuleText text={view.text.text} context={node.context} />
        </div>
        {pendingTag}
        {view.examples.map((example, index) => (
          <div
            key={index}
            lang={lang(example.translated)}
            className="mt-1 rounded-r-button border-l-2 border-border bg-surface-2 px-3 py-1 text-13 text-text-2"
          >
            <RuleText text={example.text} context={node.context} />
          </div>
        ))}
      </div>
    </div>
  )
}

/** Marks one untranslated piece while some of the rules are translated. */
function PendingMark() {
  const { t } = useTranslation()
  return (
    <span className="ml-1.5 rounded-sm bg-surface-2 px-1 align-[0.1em] text-11 font-normal whitespace-nowrap text-text-3">
      {t("rules.originalTag")}
    </span>
  )
}

function RuleTable({
  view,
  lang,
  markPending,
}: {
  readonly view: LocalizedNode
  readonly lang: (translated: boolean) => string | undefined
  /** Partial translation: every untranslated header or cell carries its own mark. */
  readonly markPending: boolean
}) {
  if (!view.table) return null
  const { header, rows } = view.table
  const last = header.length - 1
  const context = view.node.context
  // The mark stays outside the lang="ja" span: it is UI text, not part of the original.
  const piece = (cell: LocalizedText | undefined, rich = false) =>
    cell?.text ? (
      <>
        <span lang={lang(cell.translated)}>
          {rich ? (
            // A token's later abilities start on their own line, as printed; a one-line cell
            // stays inline so its pending mark follows it.
            <RuleText
              text={cell.text}
              context={context}
              className="inline [&>p]:inline [&>p+p]:block"
            />
          ) : (
            cell.text
          )}
        </span>
        {markPending && !cell.translated && <PendingMark />}
      </>
    ) : null
  return (
    <>
      {/* Phones get one card per row: name first, the short columns inline, the text below. */}
      <ul className="mt-3 flex flex-col gap-2 md:hidden">
        {rows.map((row, rowIndex) => (
          <li
            key={rowIndex}
            className="rounded-card border border-border bg-surface-1 px-3 py-2 text-13"
          >
            <p className="font-semibold text-text-1">{piece(row[0])}</p>
            <dl className="mt-1 flex flex-wrap gap-x-3 gap-y-0.5 text-text-2">
              {row.slice(1, last).map((cell, index) =>
                cell.text ? (
                  <div key={index} className="flex gap-1">
                    <dt>{piece(header[index + 1])}</dt>
                    <dd className="text-text-1">{piece(cell)}</dd>
                  </div>
                ) : null,
              )}
            </dl>
            {row[last]?.text && <div className="mt-1 text-text-1">{piece(row[last], true)}</div>}
          </li>
        ))}
      </ul>
      <div className="mt-3 hidden overflow-x-auto rounded-card border border-border md:block">
        <table className="w-full min-w-2xl border-collapse text-13">
          <thead className="bg-surface-2 text-left text-text-2">
            <tr>
              {header.map((cell, index) => (
                <th key={index} className="px-2 py-1.5 font-semibold whitespace-nowrap">
                  {piece(cell)}
                </th>
              ))}
            </tr>
          </thead>
          <tbody>
            {rows.map((row, rowIndex) => (
              <tr key={rowIndex} className="border-t border-border align-top">
                {row.map((cell, index) => (
                  <td
                    key={index}
                    className={cn(
                      "px-2 py-1.5",
                      index === row.length - 1 ? "min-w-64" : "whitespace-nowrap",
                    )}
                  >
                    {piece(cell, true)}
                  </td>
                ))}
              </tr>
            ))}
          </tbody>
        </table>
      </div>
    </>
  )
}

export function ComprehensiveRulesPage() {
  const { t } = useTranslation()
  const prefs = usePrefs()
  const uiLanguage = currentUiLanguage(prefs.uiLanguage)
  const region = rulesRegionFor(uiLanguage)
  const originalLang = RULES_LANG[region]
  const load = useRules(region)
  const navigate = useNavigate()
  const location = useLocation()
  const toast = useToast()
  const rules = load.state === "ready" ? load.rules : undefined
  // Chinese translates the Japanese rules; Japanese and English show their own edition.
  const mode = uiLanguage === "zh-TW" ? "translated" : "original"
  useHashTarget(load.state === "ready")
  const views = useMemo(
    () =>
      rules?.nodes.map((node) =>
        mode === "translated"
          ? localizeNode(node, rules.translations.get(node.key))
          : originalNode(node),
      ) ?? [],
    [mode, rules],
  )
  const keys = useMemo(() => new Set(rules?.byKey.keys()), [rules])
  const toc = useMemo(
    () => tableOfContents(views, mode === "original", originalLang),
    [views, mode, originalLang],
  )
  const tocIds = useMemo(
    () => toc.flatMap((entry) => [entry, ...entry.children]).map((entry) => ruleAnchor(entry.key)),
    [toc],
  )
  const active = useActiveSection(tocIds)
  const translatedCount = rules
    ? rules.nodes.filter((node) => rules.translations.has(node.key)).length
    : 0
  const copy = (key: string, number: string) => {
    const hash = `#${ruleAnchor(key)}`
    void navigate({ hash }, { replace: true })
    const url = new URL(`${location.pathname}${hash}`, window.location.origin).href
    navigator.clipboard.writeText(url).then(
      () => {
        toast.show({ message: t("rules.linkCopied", { number }) })
      },
      () => {
        toast.show({ message: t("rules.linkCopyFailed") })
      },
    )
  }
  if (load.state === "loading")
    return (
      <p className="rounded-card border border-border bg-surface-1 p-6 text-14 text-text-2">
        {t("rules.loading")}
      </p>
    )
  if (load.state === "failed" || !rules)
    return (
      <p className="rounded-card border border-border bg-surface-1 p-6 text-14 text-text-2">
        {t("rules.loadFailed")}
      </p>
    )
  const { index } = rules
  return (
    <div>
      <p className="flex flex-wrap items-center gap-x-4 gap-y-1 text-13 text-text-2">
        <span className="rounded-full border border-border px-2.5 font-mono text-12 text-text-1">
          {t("rules.version", { version: index.version })}
        </span>
        <span>{t("rules.lastUpdated", { date: index.last_updated })}</span>
        <a
          href={index.source_url}
          target="_blank"
          rel="noreferrer"
          className="text-accent-text underline-offset-2 hover:underline"
        >
          {t("rules.officialPdf")}
        </a>
      </p>
      {mode === "translated" && translatedCount < rules.nodes.length && (
        <p className="mt-4 rounded-card border border-dashed border-border bg-surface-1 px-4 py-3 text-13 text-text-2">
          {translatedCount === 0
            ? t("rules.translationNone")
            : t("rules.translationPartial", { done: translatedCount, total: rules.nodes.length })}
        </p>
      )}
      <RuleKeysContext.Provider value={{ keys }}>
        <div className="mt-4 lg:grid lg:grid-cols-[minmax(0,1fr)_15rem] lg:items-start lg:gap-10">
          <article className="min-w-0">
            {views.map((view) => (
              <RuleNodeView
                key={view.node.key}
                view={view}
                original={mode === "original"}
                originalLang={originalLang}
                showPendingTag={mode === "translated" && translatedCount > 0}
                onCopy={copy}
              />
            ))}
          </article>
          <RulesToc entries={toc} active={active} />
        </div>
      </RuleKeysContext.Provider>
    </div>
  )
}
