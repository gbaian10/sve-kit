import { useId, useState } from "react"
import { useTranslation } from "react-i18next"

import type { KeywordInfo } from "../../data"
import { type CardTextVocabulary, parseCardText, type Segment } from "../../domain/cardText"
import type { TextLang } from "../../domain/search"
import { cn } from "../ui/cn"
import { Link } from "../ui/Link"
import { symbolIcon } from "./symbolIcons"

export interface CardTextProps {
  readonly text: string
  readonly lang: TextLang
  readonly vocabulary: CardTextVocabulary
  /** Name/tooltip of a symbol in the UI's text language. */
  readonly symbolLocalization: (
    symbolId: string,
    lang: TextLang,
  ) => { readonly [key: string]: unknown } | undefined
  readonly keyword: (id: string) => KeywordInfo | undefined
  /** The UI's text language, for symbol labels, keyword names and explanations. */
  readonly uiLang: TextLang
  /** Show the symbol's name next to keyword-like icons (setting `symbolLabels`). */
  readonly symbolLabels: boolean
  readonly className?: string
}

const LABELLED_SYMBOLS = new Set(["fanfare", "lastword", "evolve", "stand", "act", "quick", "ub"])

function SymbolIcon({
  segment,
  label,
  showLabel,
}: {
  readonly segment: Extract<Segment, { kind: "symbol" }>
  readonly label: string
  readonly showLabel: boolean
}) {
  const icon = symbolIcon(segment.code, segment.parameter)
  if (icon === undefined) return <span>{segment.raw}</span>
  return (
    <span className="inline-flex items-center gap-0.5 align-[-0.3em]">
      <img src={icon} alt={label} title={label} className="inline-block size-5.5" />
      {showLabel && <span className="text-[0.875em] text-text-2">{label}</span>}
    </span>
  )
}

function KeywordChip({
  segment,
  info,
  lang,
}: {
  readonly segment: Extract<Segment, { kind: "keyword" }>
  readonly info: KeywordInfo | undefined
  readonly lang: TextLang
}) {
  const { t } = useTranslation()
  const id = useId()
  const [open, setOpen] = useState(false)
  // The definition lives in a detail bucket: fetched when the chip opens; a failed fetch is tried
  // again the next time it opens (a card without a definition is a different, final state).
  const [definition, setDefinition] = useState<
    | { readonly status: "idle" | "loading" | "failed" }
    | { readonly status: "ready"; readonly text: string | null }
  >({ status: "idle" })
  const load = () => {
    if (!info) return
    setDefinition({ status: "loading" })
    // A response after unmount is dropped by React itself; no guard needed (and a guard that
    // outlives StrictMode's double effect run would swallow every response).
    info.definition().then(
      (text) => {
        setDefinition({ status: "ready", text: text ?? null })
      },
      () => {
        setDefinition({ status: "failed" })
      },
    )
  }
  const name = info?.name(lang) ?? segment.name
  const text = segment.parameter === undefined ? name : `${name} ${segment.parameter}`
  return (
    <>
      <button
        type="button"
        aria-expanded={open}
        aria-controls={`${id}-definition`}
        onClick={() => {
          const opening = !open
          setOpen(opening)
          if (opening && (definition.status === "idle" || definition.status === "failed")) load()
        }}
        className={cn(
          "mx-0.5 inline-flex h-6 items-center rounded-sm border border-border-strong bg-surface-2 px-1.5 align-[-0.25em] text-[0.875em] font-semibold text-text-1",
          open && "bg-surface-3",
        )}
      >
        {text}
      </button>
      {open && (
        <span
          id={`${id}-definition`}
          className="my-1.5 block rounded-control border border-border bg-surface-1 px-3 py-2 text-14 leading-relaxed text-text-2"
        >
          <span
            className="block"
            lang={definition.status === "ready" && definition.text !== null ? "ja" : undefined}
          >
            {definition.status === "ready"
              ? (definition.text ?? t("cardPage.noDefinition"))
              : definition.status === "failed"
                ? t("cardPage.definitionFailed")
                : !info
                  ? t("cardPage.noDefinition")
                  : t("cardPage.loading")}
          </span>
          {info && (
            <Link
              to={`/cards?mech=${encodeURIComponent(info.id)}`}
              className="mt-1 inline-block text-13"
            >
              {t("cardPage.findCards", { name: info.name(lang) })}
            </Link>
          )}
        </span>
      )}
    </>
  )
}

// Effect text as the design renders it: official icons for `{記號}`, chips for `【關鍵字】` that
// explain themselves in place, paragraphs at `\n`. Japanese text gets `lang="ja"` for the font.
export function CardText({
  text,
  lang,
  vocabulary,
  symbolLocalization,
  keyword,
  uiLang,
  symbolLabels,
  className,
}: CardTextProps) {
  const segments = parseCardText(text, lang, vocabulary)
  const paragraphs: Segment[][] = [[]]
  for (const segment of segments) {
    if (segment.kind === "break") paragraphs.push([])
    else paragraphs.at(-1)?.push(segment)
  }
  return (
    <div lang={lang} className={cn("text-16 leading-effect", className)}>
      {paragraphs.map((paragraph, index) => (
        // Paragraph order never changes for one text; the index is the identity.
        <p key={index} className="min-h-[1lh]">
          {paragraph.map((segment, position) => {
            const key = position
            switch (segment.kind) {
              case "text":
              case "unknown":
                return <span key={key}>{segment.kind === "text" ? segment.text : segment.raw}</span>
              case "symbol": {
                const localized = symbolLocalization(segment.symbolId, uiLang)
                const name =
                  typeof localized?.["name"] === "string" ? localized["name"] : segment.raw
                const label =
                  segment.parameter === undefined ? name : `${name} ${segment.parameter}`
                return (
                  <SymbolIcon
                    key={key}
                    segment={segment}
                    label={label}
                    showLabel={symbolLabels && LABELLED_SYMBOLS.has(segment.code)}
                  />
                )
              }
              case "keyword":
                return (
                  <KeywordChip
                    key={key}
                    segment={segment}
                    info={keyword(segment.keywordId)}
                    lang={uiLang}
                  />
                )
              case "break":
                return null
            }
          })}
        </p>
      ))}
    </div>
  )
}
