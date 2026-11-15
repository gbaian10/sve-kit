import {
  type ReactNode,
  useContext,
  useEffect,
  useId,
  useLayoutEffect,
  useMemo,
  useRef,
  useState,
} from "react"
import { createPortal } from "react-dom"
import { Link } from "react-router"

import {
  findRuleReferences,
  loadComprehensiveRules,
  localizeNode,
  originalNode,
  resolveRuleReference,
  type RuleContext,
  type RulesBundle,
} from "../../data"
import { currentUiLanguage } from "../../i18n"
import { usePrefs } from "../../settings"
import { SymbolIcon } from "../card/InlineText"
import { inlinePieces } from "../card/textSymbols"
import { ruleAnchor, RuleKeysContext } from "./rule-keys"

const UNKNOWN_MARKER = /\{([^{}]+)\}/gu
const GAP = 6
const MARGIN = 8

function RulePreview({
  id,
  rules,
  ruleKey,
  anchor,
}: {
  readonly id: string
  readonly rules: RulesBundle
  readonly ruleKey: string
  readonly anchor: DOMRect
}) {
  const prefs = usePrefs()
  const ref = useRef<HTMLDivElement>(null)
  const [spot, setSpot] = useState({ left: anchor.left, top: anchor.bottom + GAP })
  // Fixed to the viewport and kept inside it: below the reference, or above when it would not fit.
  useLayoutEffect(() => {
    const node = ref.current
    if (!node) return
    const height = node.offsetHeight
    const above = anchor.bottom + GAP + height > window.innerHeight && anchor.top - GAP - height > 0
    setSpot({
      left: Math.max(MARGIN, Math.min(anchor.left, window.innerWidth - node.offsetWidth - MARGIN)),
      top: above ? anchor.top - GAP - height : anchor.bottom + GAP,
    })
  }, [anchor])
  const node = rules.byKey.get(ruleKey)
  if (!node) return null
  const view =
    currentUiLanguage(prefs.uiLanguage) === "zh-TW"
      ? localizeNode(node, rules.translations.get(ruleKey))
      : originalNode(node)
  const piece = view.text.text ? view.text : (view.title ?? view.text)
  return (
    <div
      ref={ref}
      id={id}
      role="tooltip"
      style={{ left: spot.left, top: spot.top }}
      className="pointer-events-none fixed z-50 w-max max-w-[min(24rem,calc(100vw-1rem))] rounded-block border border-border bg-surface-1 px-3 py-2 text-13 leading-relaxed text-text-1 shadow-lg"
    >
      <span className="font-mono text-12 text-accent-text">{node.number ?? node.key}</span>
      {/* No links inside a tooltip: it can be neither focused nor clicked. */}
      <RuleKeysContext.Provider value={{ keys: new Set() }}>
        <div lang={piece.translated && prefs.uiLanguage !== "ja" ? undefined : "ja"}>
          <RuleText text={piece.text} context={node.context} className="mt-0.5 line-clamp-6" />
        </div>
      </RuleKeysContext.Provider>
    </div>
  )
}

/**
 * A citation of a comprehensive-rules clause: hovering or focusing previews the clause, clicking
 * goes to it on the rules page. Shared site-wide (rules text now; card pages and Q&A later).
 * @public 待接：卡片頁與 Q&A 提到條號時直接使用（#2227 交付共用元件，接線在後續 issue）。
 */
export function RuleRef({
  ruleKey,
  children,
}: {
  readonly ruleKey: string
  readonly children: ReactNode
}) {
  const id = useId()
  const [anchor, setAnchor] = useState<DOMRect | null>(null)
  const [rules, setRules] = useState<RulesBundle | null>(null)
  // A scroll would leave the fixed preview floating away from its reference.
  useEffect(() => {
    if (!anchor) return
    const hide = () => {
      setAnchor(null)
    }
    window.addEventListener("scroll", hide, { passive: true })
    return () => {
      window.removeEventListener("scroll", hide)
    }
  }, [anchor])
  const show = (event: { currentTarget: Element }) => {
    setAnchor(event.currentTarget.getBoundingClientRect())
    // The rules chunk loads on first interest; without it the reference is still a plain link.
    if (!rules) loadComprehensiveRules().then(setRules, () => undefined)
  }
  const hide = () => {
    setAnchor(null)
  }
  const hovering = { onMouseEnter: show, onMouseLeave: hide, onFocus: show, onBlur: hide }
  return (
    <>
      <Link
        to={{ pathname: "/rules", hash: `#${ruleAnchor(ruleKey)}` }}
        aria-describedby={anchor && rules ? id : undefined}
        {...hovering}
        className="text-accent-text underline decoration-dotted underline-offset-4 hover:decoration-solid"
      >
        {children}
      </Link>
      {/* Portalled: the reference sits inside a <p>, where the preview's blocks are not allowed. */}
      {anchor &&
        rules &&
        createPortal(
          <RulePreview id={id} rules={rules} ruleKey={ruleKey} anchor={anchor} />,
          document.body,
        )}
    </>
  )
}

function Plain({ text, context }: { readonly text: string; readonly context: RuleContext }) {
  const { keys } = useContext(RuleKeysContext)
  const parts: ReactNode[] = []
  const spans = [
    ...findRuleReferences(text).flatMap((ref) => {
      const key = resolveRuleReference(ref.number, context, keys)
      return key ? [{ start: ref.start, end: ref.end, key, label: ref.number }] : []
    }),
    // Icons without a site symbol ({PP}, the trigger icons) keep their name as a small label.
    ...[...text.matchAll(UNKNOWN_MARKER)].map((match) => ({
      start: match.index,
      end: match.index + match[0].length,
      key: undefined,
      label: match[1] ?? "",
    })),
  ].sort((a, b) => a.start - b.start)
  let cursor = 0
  for (const span of spans) {
    if (span.start < cursor) continue
    if (cursor < span.start) parts.push(text.slice(cursor, span.start))
    parts.push(
      span.key ? (
        <RuleRef key={span.start} ruleKey={span.key}>
          {span.label}
        </RuleRef>
      ) : (
        <span
          key={span.start}
          className="mx-0.5 rounded-sm border border-border px-1 text-[0.85em] whitespace-nowrap text-text-2"
        >
          {span.label}
        </span>
      ),
    )
    cursor = span.end
  }
  if (cursor < text.length) parts.push(text.slice(cursor))
  return <>{parts}</>
}

function Line({ text, context }: { readonly text: string; readonly context: RuleContext }) {
  const pieces = useMemo(() => inlinePieces(text), [text])
  return (
    <>
      {pieces.map((piece) =>
        piece.kind === "text" ? (
          <Plain key={piece.start} text={piece.text} context={context} />
        ) : (
          <SymbolIcon key={piece.start} piece={piece} />
        ),
      )}
    </>
  )
}

/**
 * One piece of rules text: printed line breaks, `* ` bullet lists, the card-text icon markers and
 * cited clause numbers (a RuleRef when the number exists among the provided keys).
 */
export function RuleText({
  text,
  context,
  className,
}: {
  readonly text: string
  readonly context: RuleContext
  readonly className?: string
}) {
  const blocks: { readonly bullets: boolean; readonly lines: string[] }[] = []
  for (const line of text.split("\n")) {
    const bullets = line.startsWith("* ")
    const last = blocks.at(-1)
    if (last && bullets && last.bullets) last.lines.push(line.slice(2))
    else blocks.push({ bullets, lines: [bullets ? line.slice(2) : line] })
  }
  return (
    <div className={className}>
      {blocks.map((block, index) =>
        block.bullets ? (
          <ul key={index} className="my-1 list-disc pl-5">
            {block.lines.map((line, lineIndex) => (
              <li key={lineIndex}>
                <Line text={line} context={context} />
              </li>
            ))}
          </ul>
        ) : (
          <p key={index}>
            <Line text={block.lines[0] ?? ""} context={context} />
          </p>
        ),
      )}
    </div>
  )
}
