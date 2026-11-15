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

// Leaving the reference for the preview crosses a small gap; closing waits this long for it.
const LEAVE_DELAY_MS = 150

function RulePreview({
  id,
  rules,
  ruleKey,
  anchor,
  hover,
}: {
  readonly id: string
  readonly rules: RulesBundle
  readonly ruleKey: string
  readonly anchor: DOMRect
  /** Keeps the preview open while the pointer is on it, so it can be read in place. */
  readonly hover: { readonly onMouseEnter: () => void; readonly onMouseLeave: () => void }
}) {
  const prefs = usePrefs()
  const ref = useRef<HTMLDivElement>(null)
  const [spot, setSpot] = useState({ left: anchor.left, top: anchor.bottom + GAP })
  // Fixed to the viewport and always inside it: below the reference, above when only that fits,
  // otherwise clamped to the viewport (its height is capped there too).
  useLayoutEffect(() => {
    const node = ref.current
    if (!node) return
    const height = node.offsetHeight
    const below = anchor.bottom + GAP
    const above = anchor.top - GAP - height
    const fitsBelow = below + height <= window.innerHeight - MARGIN
    const top = fitsBelow ? below : above >= MARGIN ? above : below
    setSpot({
      left: Math.max(MARGIN, Math.min(anchor.left, window.innerWidth - node.offsetWidth - MARGIN)),
      top: Math.max(MARGIN, Math.min(top, window.innerHeight - height - MARGIN)),
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
      {...hover}
      className="fixed z-50 max-h-[calc(100dvh-1rem)] w-max max-w-[min(24rem,calc(100vw-1rem))] overflow-hidden rounded-block border border-border bg-surface-1 px-3 py-2 text-13 leading-relaxed text-text-1 shadow-lg"
    >
      <span className="font-mono text-12 text-accent-text">{node.number ?? node.key}</span>
      {/* Plain text only: nothing inside a tooltip can take focus. */}
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
 * goes to it on the rules page. Escape dismisses the preview without moving focus, and the pointer
 * can move onto the preview to read it. Shared site-wide (rules text now; card pages and Q&A later).
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
  const leaving = useRef(0)
  const open = anchor !== null
  // A scroll would leave the fixed preview floating away from its reference; Escape dismisses it.
  useEffect(() => {
    if (!open) return
    const hide = () => {
      setAnchor(null)
    }
    const escape = (event: KeyboardEvent) => {
      if (event.key === "Escape") hide()
    }
    window.addEventListener("scroll", hide, { passive: true })
    window.addEventListener("keydown", escape)
    return () => {
      window.removeEventListener("scroll", hide)
      window.removeEventListener("keydown", escape)
    }
  }, [open])
  useEffect(
    () => () => {
      window.clearTimeout(leaving.current)
    },
    [],
  )
  const stay = () => {
    window.clearTimeout(leaving.current)
  }
  const show = (event: { currentTarget: Element }) => {
    stay()
    setAnchor(event.currentTarget.getBoundingClientRect())
    // The rules chunk loads on first interest; without it the reference is still a plain link.
    if (!rules) loadComprehensiveRules().then(setRules, () => undefined)
  }
  const hide = () => {
    stay()
    setAnchor(null)
  }
  const hideSoon = () => {
    stay()
    leaving.current = window.setTimeout(hide, LEAVE_DELAY_MS)
  }
  const hovering = { onMouseEnter: show, onMouseLeave: hideSoon, onFocus: show, onBlur: hide }
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
          <RulePreview
            id={id}
            rules={rules}
            ruleKey={ruleKey}
            anchor={anchor}
            hover={{ onMouseEnter: stay, onMouseLeave: hideSoon }}
          />,
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
