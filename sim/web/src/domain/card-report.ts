import type { UiLanguage } from "../i18n/languages"

export const CARD_REPORT_PATH = "/api/card-reports"
export const CARD_REPORT_TYPES = ["data", "mapping", "translation", "rules", "other"] as const
export const REPORT_DESCRIPTION_LIMIT = 2000
export const REPORT_CONTACT_LIMIT = 100
export const REPORT_TURNSTILE_ACTION = "card_report"
export const TEST_TURNSTILE_SITE_KEY = "1x00000000000000000000AA"

export const CARD_REPORT_REPOSITORY = "gbaian10/sve-kit"

export type ReportType = (typeof CARD_REPORT_TYPES)[number]

export interface CardReportMetadata {
  readonly cardNo: string | null
  readonly printingIntId: number | null
  readonly edition: "jp" | "en"
  readonly uiLanguage: UiLanguage
  readonly dataVersion: string
}

export interface CardReportInput extends CardReportMetadata {
  readonly type: ReportType
  readonly description: string
  readonly contact: string
  readonly token: string
}
