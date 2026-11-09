import { useTranslation } from "react-i18next"

export function CardReleaseDate({ date }: { readonly date: string | undefined }) {
  const { t } = useTranslation()
  return date === undefined ? null : (
    <p className="mb-3 text-13 text-text-2" data-card-release-date="">
      {t("cardPage.releaseDate")} <time dateTime={date}>{date}</time>
    </p>
  )
}
