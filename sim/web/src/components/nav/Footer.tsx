import { useTranslation } from "react-i18next"

import { formatDataVersion } from "../../domain/dataVersion"

// Every page ends with the unofficial notice (design §13); the data version is filled in once the
// snapshot loads.
export function Footer({ dataVersion }: { readonly dataVersion?: string | undefined }) {
  const { t } = useTranslation()
  return (
    <footer className="mx-auto w-full max-w-320 px-4 py-6 text-12 text-text-3 lg:px-6">
      <p>{t("footer.unofficial")}</p>
      <p className="mt-1" title={dataVersion}>
        {t("footer.dataVersion", {
          version:
            dataVersion === undefined
              ? t("footer.dataVersionUnknown")
              : formatDataVersion(dataVersion),
        })}
      </p>
    </footer>
  )
}
