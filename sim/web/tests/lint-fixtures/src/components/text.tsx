import { useTranslation } from "react-i18next"

// case: translated text and attributes -> none
export const Translated = () => {
  const { t } = useTranslation()
  return (
    <button type="button" aria-label={t("close")} title={t("close")} data-testid="close">
      {t("close")} 3/5
    </button>
  )
}

// case: literal JSX text -> i18next/no-literal-string
export const LiteralText = () => <p>Hello world</p>

// case: literal string in a JSX expression -> i18next/no-literal-string
export const LiteralExpression = ({ on }: { on: boolean }) => <p>{on ? "Enabled" : ""}</p>

// case: literal aria-label -> i18next/no-literal-string
export const LiteralAriaLabel = () => <button type="button" aria-label="Close" />

// case: literal placeholder and aria-label -> i18next/no-literal-string, i18next/no-literal-string
export const LiteralPlaceholder = () => <input aria-label="Search" placeholder="Search cards" />

// case: literal alt -> i18next/no-literal-string
export const LiteralAlt = () => <img src="/a.png" alt="Card art" />
