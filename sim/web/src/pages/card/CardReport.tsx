import { CircleCheck } from "lucide-react"
import { type SubmitEvent, useEffect, useId, useRef, useState } from "react"
import { useTranslation } from "react-i18next"

import { Button } from "../../components/ui/Button"
import { Dialog } from "../../components/ui/Dialog"
import { type CardReportResult, sendCardReport } from "../../data"
import {
  CARD_REPORT_REPOSITORY,
  CARD_REPORT_TYPES,
  type CardReportMetadata,
  REPORT_DESCRIPTION_LIMIT,
  type ReportType,
  TEST_TURNSTILE_SITE_KEY,
} from "../../domain/card-report"
import { ReportTurnstile } from "./ReportTurnstile"

const SITE_KEY =
  import.meta.env.VITE_TURNSTILE_SITE_KEY ?? (import.meta.env.DEV ? TEST_TURNSTILE_SITE_KEY : "")
const FIELD =
  "w-full rounded-control border border-border-strong bg-surface-1 px-3 py-2.5 text-16 text-text-1"

export interface CardReportDraft {
  readonly initialType: ReportType
  readonly metadata: CardReportMetadata
}

export function CardReportDialog({
  draft,
  onClose,
}: {
  readonly draft: CardReportDraft | null
  readonly onClose: () => void
}) {
  const { t } = useTranslation()
  return (
    <Dialog
      open={draft !== null}
      onClose={onClose}
      title={t("cardReport.title")}
      className="overflow-hidden"
    >
      {draft && <CardReportForm {...draft} />}
    </Dialog>
  )
}

function CardReportForm({ initialType, metadata }: CardReportDraft) {
  const { t } = useTranslation()
  const id = useId()
  const descriptionField = useRef<HTMLTextAreaElement>(null)
  const issueLink = useRef<HTMLAnchorElement>(null)
  const [type, setType] = useState(initialType)
  const [token, setToken] = useState<string | null>(null)
  const [widgetError, setWidgetError] = useState(false)
  const [widgetAttempt, setWidgetAttempt] = useState(0)
  const [sending, setSending] = useState(false)
  const [result, setResult] = useState<CardReportResult | null>(null)
  const issueNumber = result && "issueNumber" in result ? result.issueNumber : null

  useEffect(() => {
    if (issueNumber !== null) issueLink.current?.focus()
  }, [issueNumber])

  const renewVerification = () => {
    setToken(null)
    setWidgetError(false)
    setWidgetAttempt((value) => value + 1)
  }

  const submit = async (event: SubmitEvent<HTMLFormElement>) => {
    event.preventDefault()
    if (!token || sending) return
    const field = descriptionField.current
    if (!field) return
    const description = field.value.trim()
    if (!description) {
      field.setCustomValidity(t("cardReport.descriptionRequired"))
      field.reportValidity()
      return
    }
    setSending(true)
    setResult(null)
    const response = await sendCardReport({
      ...metadata,
      type,
      description,
      token,
    })
    setSending(false)
    setResult(response)
    if ("error" in response) renewVerification()
  }

  if (issueNumber !== null) {
    return (
      <div role="status" className="flex flex-col items-center gap-2 px-4 py-9 text-center">
        <CircleCheck className="size-9 text-accent-text" aria-hidden="true" />
        <p className="text-18 font-semibold">{t("cardReport.sent")}</p>
        <a
          ref={issueLink}
          href={`https://github.com/${CARD_REPORT_REPOSITORY}/issues/${String(issueNumber)}`}
          target="_blank"
          rel="noreferrer"
          className="p-2 text-14 text-accent-text underline"
        >
          {t("cardReport.viewIssue")}
        </a>
      </div>
    )
  }

  const fields = [
    [t("cardReport.cardNo"), metadata.cardNo ?? t("cardPage.noCardNo")],
    [t("cardReport.edition"), metadata.edition.toUpperCase()],
    [t("cardReport.uiLanguage"), t(`options.language.${metadata.uiLanguage}`)],
    [t("cardReport.dataVersion"), metadata.dataVersion],
  ]
  const error = !SITE_KEY
    ? "unavailable"
    : result && "error" in result
      ? result.error
      : widgetError
        ? "verification"
        : null
  return (
    <form
      onSubmit={(event) => {
        void submit(event)
      }}
      className="space-y-4"
    >
      <dl className="grid grid-cols-3 gap-x-4 gap-y-2 rounded-control border border-border bg-surface-2 px-3 py-2.5">
        {fields.map(([label, value], index) => (
          <div key={label} className={index === 3 ? "col-span-full" : "min-w-0"}>
            <dt className="text-12 text-text-2">{label}</dt>
            <dd className="font-mono text-13 font-medium wrap-anywhere">{value}</dd>
          </div>
        ))}
      </dl>
      <fieldset disabled={sending} className="space-y-4">
        <div>
          <label htmlFor={`${id}-type`} className="mb-1 block text-14 font-semibold">
            {t("cardReport.type")}
          </label>
          <select
            id={`${id}-type`}
            className={FIELD}
            value={type}
            onChange={(event) => {
              setType(event.target.value as ReportType)
            }}
          >
            {CARD_REPORT_TYPES.map((value) => (
              <option key={value} value={value}>
                {t(`cardReport.types.${value}`)}
              </option>
            ))}
          </select>
        </div>
        <div>
          <label htmlFor={`${id}-description`} className="mb-1 block text-14 font-semibold">
            {t("cardReport.description")}
          </label>
          <textarea
            ref={descriptionField}
            id={`${id}-description`}
            rows={3}
            required
            maxLength={REPORT_DESCRIPTION_LIMIT}
            placeholder={t("cardReport.descriptionPlaceholder")}
            aria-describedby={type === "translation" ? `${id}-source` : undefined}
            onInvalid={(event) => {
              if (event.currentTarget.validity.valueMissing) {
                event.currentTarget.setCustomValidity(t("cardReport.descriptionRequired"))
              }
            }}
            onInput={(event) => {
              event.currentTarget.setCustomValidity("")
            }}
            className={`${FIELD} block resize-y`}
          />
          {type === "translation" && (
            <p id={`${id}-source`} className="mt-1 text-13 text-text-2">
              {t("cardReport.translationHint")}
            </p>
          )}
        </div>
      </fieldset>
      {SITE_KEY && (
        <ReportTurnstile
          key={widgetAttempt}
          siteKey={SITE_KEY}
          language={metadata.uiLanguage}
          onToken={(value) => {
            setToken(value)
            if (value) setWidgetError(false)
          }}
          onError={() => {
            setToken(null)
            setWidgetError(true)
          }}
        />
      )}
      <div className="flex flex-wrap items-center justify-end gap-2">
        {error !== null && (
          <p role="alert" className="me-auto text-14 text-danger">
            {t(`cardReport.errors.${error}`)}
          </p>
        )}
        {widgetError && (
          <Button onClick={renewVerification} disabled={sending}>
            {t("cardReport.retryVerification")}
          </Button>
        )}
        <Button
          variant="primary"
          type="submit"
          disabled={!token || sending}
          className="w-full md:w-auto"
        >
          {sending ? t("cardReport.sending") : t("cardReport.submit")}
        </Button>
      </div>
    </form>
  )
}
