import { Fragment, type ReactNode, useContext, useMemo } from "react"

import { SymbolIcon } from "../../components/card/InlineText"
import { inlinePieces } from "../../components/card/textSymbols"
import { findRuleReferences, resolveRuleReference, type RuleContext } from "../../data"
import { ruleAnchor, RuleKeysContext } from "./rule-keys"

const UNKNOWN_MARKER = /\{([^{}]+)\}/gu

function Plain({ text, context }: { readonly text: string; readonly context: RuleContext }) {
  const { keys, renderLink } = useContext(RuleKeysContext)
  const parts: ReactNode[] = []
  const spans = [
    ...findRuleReferences(text).flatMap((ref) => {
      const key = resolveRuleReference(ref.number, context, keys)
      return key
        ? [{ start: ref.start, end: ref.end, kind: "ref" as const, key, label: ref.number }]
        : []
    }),
    // Icons without a site symbol ({PP}, the trigger icons) keep their name as a small label.
    ...[...text.matchAll(UNKNOWN_MARKER)].map((match) => ({
      start: match.index,
      end: match.index + match[0].length,
      kind: "marker" as const,
      key: "",
      label: match[1] ?? "",
    })),
  ].sort((a, b) => a.start - b.start)
  let cursor = 0
  for (const span of spans) {
    if (span.start < cursor) continue
    if (cursor < span.start) parts.push(text.slice(cursor, span.start))
    const target = { key: span.key, number: span.label }
    parts.push(
      span.kind === "ref" ? (
        <Fragment key={span.start}>
          {renderLink ? (
            renderLink(target, span.label)
          ) : (
            <a
              href={`#${ruleAnchor(span.key)}`}
              className="text-accent-text underline underline-offset-2"
            >
              {span.label}
            </a>
          )}
        </Fragment>
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
 * cited clause numbers (linked when the number exists in this version).
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
    if (last && last.bullets === bullets && bullets) last.lines.push(line.slice(2))
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
