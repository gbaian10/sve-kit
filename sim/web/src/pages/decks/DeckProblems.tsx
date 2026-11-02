import { CircleAlert, CircleCheck, Info, TriangleAlert } from "lucide-react"
import { useState } from "react"
import { useTranslation } from "react-i18next"

import { cn } from "../../components/ui/cn"
import type { RestrictionStatus } from "../../data"
import type { DeckProblem } from "../../domain/deck/rules"
import { useDeckEditor } from "./deck-editor"
import { useDeckLabels } from "./deck-labels"

const TONE = {
  error: { Icon: CircleAlert, className: "text-danger" },
  warning: { Icon: TriangleAlert, className: "text-warning" },
  info: { Icon: Info, className: "text-info" },
} as const

const SHOWN = 6

/** The deck's construction problems with their CR clause; one about a card scrolls to it. */
export function DeckProblems({
  restrictionStatus,
}: {
  readonly restrictionStatus: RestrictionStatus | undefined
}) {
  const { t } = useTranslation()
  const editor = useDeckEditor()
  const labels = useDeckLabels()
  const [expanded, setExpanded] = useState(false)
  const problems = editor.problems
  const describe = (problem: DeckProblem): string => {
    const values: Record<string, string | number> = { ...problem.values }
    for (const key of ["cardClass", "deckClass", "leaderClass"]) {
      const code = values[key]
      if (typeof code === "string")
        values[key] = labels.basisLabel({ kind: "class", classCode: code })
    }
    if (typeof values["title"] === "string") values["title"] = labels.titleName(values["title"])
    if (values["section"] === "main" || values["section"] === "evolve")
      values["section"] = t(`decks.section.${values["section"]}`)
    // Each kind has its own values; the default value selects the overload that takes any of them.
    return t(`decks.problems.${problem.kind}`, { ...values, defaultValue: problem.kind })
  }
  const errors = problems.filter((item) => item.severity === "error").length
  const shown = expanded ? problems : problems.slice(0, SHOWN)
  return (
    <div className="flex flex-col gap-1.5 rounded-button border border-border bg-surface-1 p-2">
      <div className="flex flex-wrap items-center justify-between gap-2">
        <span className="flex items-center gap-1.5 text-12 font-semibold text-text-2">
          {errors === 0 ? (
            <>
              <CircleCheck className="size-4 text-success" aria-hidden="true" />
              {problems.some((item) => item.severity === "warning")
                ? t("decks.status.legalWithWarnings")
                : t("decks.status.legal")}
            </>
          ) : (
            <>
              <CircleAlert className="size-4 text-danger" aria-hidden="true" />
              {t("decks.status.problems", { count: errors })}
            </>
          )}
        </span>
        <span className="text-11 text-text-3">
          {restrictionStatus === undefined || restrictionStatus.state === "unknown"
            ? t("decks.restrictionsUnknown")
            : restrictionStatus.state === "partial"
              ? t("decks.restrictionsPartial", { date: restrictionStatus.onDate })
              : t("decks.restrictionsDate", { date: restrictionStatus.onDate })}
        </span>
      </div>
      {problems.length > 0 && (
        <ul className="flex flex-col gap-1">
          {shown.map((problem, index) => {
            const { Icon, className: tone } = TONE[problem.severity]
            const target = problem.cardIds[0]
            const content = (
              <>
                <Icon className={cn("mt-0.5 size-4 shrink-0", tone)} aria-hidden="true" />
                <span className="min-w-0 flex-1 text-13 text-text-1">{describe(problem)}</span>
                <span className="shrink-0 rounded-badge bg-surface-3 px-1.5 text-11 text-text-2 tabular-nums">
                  {problem.clause === "restriction"
                    ? t("decks.clauseRestriction")
                    : t("decks.clause", { clause: problem.clause })}
                </span>
              </>
            )
            return (
              <li key={`${problem.kind}-${String(index)}`}>
                {target === undefined ? (
                  <span className="flex items-start gap-2 px-1.5 py-1">{content}</span>
                ) : (
                  <button
                    type="button"
                    onClick={() => {
                      editor.focusCard(target)
                    }}
                    className="flex w-full items-start gap-2 rounded-button px-1.5 py-1 text-start hover:bg-surface-2"
                  >
                    {content}
                  </button>
                )}
              </li>
            )
          })}
        </ul>
      )}
      {problems.length > SHOWN && (
        <button
          type="button"
          aria-expanded={expanded}
          onClick={() => {
            setExpanded((value) => !value)
          }}
          className="self-start px-1.5 text-12 font-semibold text-accent-text hover:underline"
        >
          {expanded
            ? t("decks.problemsLess")
            : t("decks.problemsMore", { count: problems.length - SHOWN })}
        </button>
      )}
    </div>
  )
}
