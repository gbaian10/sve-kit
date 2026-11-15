import { useEffect, useMemo, useState } from "react"
import { useTranslation } from "react-i18next"
import { useLocation, useNavigate } from "react-router"

import { cn } from "../../components/ui/cn"
import { useToast } from "../../components/ui/toast-context"
import {
  loadComprehensiveRules,
  type LocalizedNode,
  localizeNode,
  originalNode,
  type RulesBundle,
} from "../../data"
import { currentUiLanguage } from "../../i18n"
import { usePrefs } from "../../settings"
import { ruleAnchor, RuleKeysContext } from "./rule-keys"
import { RuleText } from "./RuleText"

type Load =
  | { readonly state: "loading" }
  | { readonly state: "failed" }
  | { readonly state: "ready"; readonly rules: RulesBundle }

function useRules(): Load {
  const [load, setLoad] = useState<Load>({ state: "loading" })
  useEffect(() => {
    let active = true
    loadComprehensiveRules()
      .then((rules) => {
        if (active) setLoad({ state: "ready", rules })
      })
      .catch(() => {
        if (active) setLoad({ state: "failed" })
      })
    return () => {
      active = false
    }
  }, [])
  return load
}

const FLASH_MS = 1500

/** Scrolls to the hash target once it exists and marks it briefly (`data-flash`). */
function useHashTarget(ready: boolean) {
  const { hash } = useLocation()
  useEffect(() => {
    if (!ready || !hash) return
    const element = document.getElementById(decodeURIComponent(hash.slice(1)))
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
  showPendingTag,
  onCopy,
}: {
  readonly view: LocalizedNode
  /** The view shows the Japanese original as a whole (ja UI, or the en UI's original toggle). */
  readonly original: boolean
  readonly showPendingTag: boolean
  readonly onCopy: (key: string, number: string) => void
}) {
  const { t } = useTranslation()
  const { node } = view
  const id = ruleAnchor(node.key)
  const lang = (translated: boolean) => (original || !translated ? "ja" : undefined)
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
          <RuleText
            text={view.text.text}
            context={node.context}
            className="mt-2 max-w-[66ch] text-15 text-text-1"
          />
        )}
        {view.table && <RuleTable view={view} lang={lang} />}
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

function RuleTable({
  view,
  lang,
}: {
  readonly view: LocalizedNode
  readonly lang: (translated: boolean) => string | undefined
}) {
  if (!view.table) return null
  const { header, rows } = view.table
  const last = header.length - 1
  return (
    <>
      {/* Phones get one card per row: name first, the short columns inline, the text below. */}
      <ul className="mt-3 flex flex-col gap-2 md:hidden">
        {rows.map((row, rowIndex) => (
          <li
            key={rowIndex}
            className="rounded-card border border-border bg-surface-1 px-3 py-2 text-13"
          >
            <p lang={lang(row[0]?.translated ?? true)} className="font-semibold text-text-1">
              {row[0]?.text}
            </p>
            <dl className="mt-1 flex flex-wrap gap-x-3 gap-y-0.5 text-text-2">
              {row.slice(1, last).map((cell, index) =>
                cell.text ? (
                  <div key={index} className="flex gap-1">
                    <dt lang={lang(header[index + 1]?.translated ?? true)}>
                      {header[index + 1]?.text}
                    </dt>
                    <dd lang={lang(cell.translated)} className="text-text-1">
                      {cell.text}
                    </dd>
                  </div>
                ) : null,
              )}
            </dl>
            {row[last]?.text && (
              <div lang={lang(row[last].translated)} className="mt-1 text-text-1">
                <RuleText text={row[last].text} context={view.node.context} />
              </div>
            )}
          </li>
        ))}
      </ul>
      <div className="mt-3 hidden overflow-x-auto rounded-card border border-border md:block">
        <table className="w-full min-w-2xl border-collapse text-13">
          <thead className="bg-surface-2 text-left text-text-2">
            <tr>
              {header.map((cell, index) => (
                <th
                  key={index}
                  lang={lang(cell.translated)}
                  className="px-2 py-1.5 font-semibold whitespace-nowrap"
                >
                  {cell.text}
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
                    lang={lang(cell.translated)}
                    className={cn(
                      "px-2 py-1.5",
                      index === row.length - 1 ? "min-w-64" : "whitespace-nowrap",
                    )}
                  >
                    <RuleText text={cell.text} context={view.node.context} />
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
  const load = useRules()
  const navigate = useNavigate()
  const location = useLocation()
  const toast = useToast()
  const [showJapanese, setShowJapanese] = useState(false)
  const rules = load.state === "ready" ? load.rules : undefined
  // English has no rules text yet; its readers opt in to the Japanese original.
  const mode =
    uiLanguage === "zh-TW"
      ? "translated"
      : uiLanguage === "ja" || showJapanese
        ? "original"
        : "none"
  useHashTarget(load.state === "ready" && mode !== "none")
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
      {uiLanguage === "en" && (
        <div className="mt-4 flex flex-wrap items-center gap-3 rounded-card border border-dashed border-border bg-surface-1 px-4 py-3 text-13 text-text-2">
          <span>{t("rules.englishUnavailable")}</span>
          <button
            type="button"
            aria-pressed={showJapanese}
            onClick={() => {
              setShowJapanese((value) => !value)
            }}
            className="min-h-11 rounded-button border border-border bg-surface-2 px-3 font-semibold text-text-1 hover:border-accent/50"
          >
            {showJapanese ? t("rules.hideJapanese") : t("rules.showJapanese")}
          </button>
        </div>
      )}
      {mode !== "none" && (
        <RuleKeysContext.Provider value={{ keys }}>
          <article className="mt-4">
            {views.map((view) => (
              <RuleNodeView
                key={view.node.key}
                view={view}
                original={mode === "original"}
                showPendingTag={mode === "translated" && translatedCount > 0}
                onCopy={copy}
              />
            ))}
          </article>
        </RuleKeysContext.Provider>
      )}
    </div>
  )
}
