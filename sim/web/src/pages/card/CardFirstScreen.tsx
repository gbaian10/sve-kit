import { ArrowLeft, Link2, Share2 } from "lucide-react"
import { useTranslation } from "react-i18next"
import { Link } from "react-router"

import { CardImage } from "../../components/card/CardImage"
import { CardText } from "../../components/card/CardText"
import { symbolIcon } from "../../components/card/symbolIcons"
import { cn } from "../../components/ui/cn"
import { Segmented } from "../../components/ui/Segmented"
import { useToast } from "../../components/ui/toast-context"
import type { CardFaceView, CardSummary, CardView, ImageIndex } from "../../data"
import { displayName, UI_TEXT_LANG } from "../../domain/nameDisplay"
import type { Region } from "../../domain/search"
import type { UiLanguage } from "../../i18n/languages"
import { usePrefs } from "../../settings"

export interface CardFirstScreenProps {
  readonly view: CardView
  readonly summary: CardSummary
  readonly images: ImageIndex | undefined
  readonly uiLanguage: UiLanguage
  /** Overlay: back closes the layer; full page: back is a link to the list. */
  readonly onBack: (() => void) | undefined
  readonly onEdition: (region: Region) => void
}

function IconButton({
  label,
  onClick,
  children,
}: {
  readonly label: string
  readonly onClick: () => void
  readonly children: React.ReactNode
}) {
  return (
    <button
      type="button"
      aria-label={label}
      title={label}
      onClick={onClick}
      className="flex size-11 shrink-0 items-center justify-center rounded-button text-text-1 hover:bg-surface-2"
    >
      {children}
    </button>
  )
}

function Stat({
  icon,
  label,
  value,
}: {
  readonly icon: string | undefined
  readonly label: string
  readonly value: number | null
}) {
  return (
    <span className="flex items-center gap-1" title={label}>
      {icon === undefined ? (
        <span className="text-12 text-text-3">{label}</span>
      ) : (
        <img src={icon} alt={label} className="size-5" />
      )}
      <span className="text-18 font-bold tabular-nums">{value === null ? "—" : value}</span>
    </span>
  )
}

function FaceSection({
  face,
  view,
  uiLanguage,
  first,
}: {
  readonly face: CardFaceView
  readonly view: CardView
  readonly uiLanguage: UiLanguage
  readonly first: boolean
}) {
  const { t } = useTranslation()
  const prefs = usePrefs()
  const uiLang = UI_TEXT_LANG[uiLanguage]
  const name = displayName(face.name, uiLanguage, prefs.nameDisplay)
  const meta = [
    face.classCode === null
      ? t("search.neutral")
      : view.vocabularyLabel("class", face.classCode, uiLang),
    view.vocabularyLabel("type", face.typeCode, uiLang),
    ...face.traits.map((code) => view.vocabularyLabel("trait", code, uiLang)),
  ]
  const metaText = meta.join("・")
  const effect = face.effect
  return (
    <section
      aria-label={name.primary.text}
      className={cn(!first && "mt-6 border-t border-border pt-4")}
    >
      {!first && (
        <h2 lang={name.primary.lang} className="text-18 font-bold">
          {name.primary.text}
        </h2>
      )}
      {!first && name.secondary && (
        <p lang={name.secondary.lang} className="text-13 text-text-2">
          {name.secondary.text}
        </p>
      )}
      {!first && <p className="mt-1 text-13 text-text-2">{metaText}</p>}
      {!first && (
        <p className="mt-2 flex gap-4">
          <Stat
            icon={face.cost === null ? undefined : symbolIcon("cost", String(face.cost))}
            label={t("cardPage.stats.cost")}
            value={face.cost}
          />
          {face.attack !== null && (
            <Stat
              icon={symbolIcon("power")}
              label={t("cardPage.stats.attack")}
              value={face.attack}
            />
          )}
          {face.defense !== null && (
            <Stat
              icon={symbolIcon("hp")}
              label={t("cardPage.stats.defense")}
              value={face.defense}
            />
          )}
        </p>
      )}
      <h3 className="sr-only">{t("cardPage.effect")}</h3>
      {effect === null ? (
        <p className="text-16 text-text-3">—</p>
      ) : (
        <>
          <CardText
            text={effect.original.text}
            lang={effect.original.lang}
            vocabulary={view.vocabulary}
            symbolLocalization={view.symbolLocalization}
            keyword={view.keyword}
            uiLang={uiLang}
            symbolLabels={prefs.symbolLabels}
            className="mt-2"
          />
          {effect.translation && (
            <div className="mt-3 rounded-block border border-border bg-surface-1 px-3 py-2">
              <p
                className={cn(
                  "mb-1 text-12",
                  effect.translation.label === "machine"
                    ? "rounded-badge bg-warning-soft px-1.5 py-0.5 text-warning"
                    : "text-text-3",
                )}
              >
                {t(`cardPage.translationLabel.${effect.translation.label}`)}
              </p>
              <CardText
                text={effect.translation.text}
                lang={effect.translation.lang}
                vocabulary={view.vocabulary}
                symbolLocalization={view.symbolLocalization}
                keyword={view.keyword}
                uiLang={uiLang}
                symbolLabels={prefs.symbolLabels}
              />
            </div>
          )}
          {effect.notices.includes("missing") && (
            <p className="mt-2 text-13 text-text-3">{t("cardPage.missingTranslation")}</p>
          )}
        </>
      )}
    </section>
  )
}

// Design 06 first screen: 52px top row (back, set chip, number, copy, share), then thumbnail 96 +
// name + class/type/traits + three stats, then the effect at full width so it starts by y≈300.
export function CardFirstScreen({
  view,
  summary,
  images,
  uiLanguage,
  onBack,
  onEdition,
}: CardFirstScreenProps) {
  const { t } = useTranslation()
  const prefs = usePrefs()
  const toast = useToast()
  const uiLang = UI_TEXT_LANG[uiLanguage]
  const front = view.faces[0]
  const name = front ? displayName(front.name, uiLanguage, prefs.nameDisplay) : undefined
  const url = () => window.location.href
  const copy = async () => {
    try {
      await navigator.clipboard.writeText(url())
      toast.show({ message: t("cardPage.copied") })
    } catch {
      // Clipboard blocked: the address bar still has the link.
    }
  }
  const share = async () => {
    if (typeof navigator.share === "function") {
      try {
        await navigator.share({ title: name?.primary.text ?? view.cardNo, url: url() })
        return
      } catch {
        // Cancelled or unsupported target: fall back to copying.
      }
    }
    await copy()
  }
  const meta = front
    ? [
        front.classCode === null
          ? t("search.neutral")
          : view.vocabularyLabel("class", front.classCode, uiLang),
        view.vocabularyLabel("type", front.typeCode, uiLang),
        ...front.traits.map((code) => view.vocabularyLabel("trait", code, uiLang)),
      ]
    : []
  const metaText = meta.join("・")
  const editionOptions = (["jp", "en"] as const)
    .filter((region) => view.editions[region] !== undefined)
    .map((region) => ({ value: region, label: t(`options.edition.${region}`) }))
  return (
    <article className="mx-auto w-full max-w-200">
      <div className="flex h-13 items-center gap-1">
        {onBack ? (
          <IconButton label={t("cardPage.back")} onClick={onBack}>
            <ArrowLeft className="size-5" aria-hidden="true" />
          </IconButton>
        ) : (
          <Link
            to="/cards"
            aria-label={t("cardPage.backToCards")}
            title={t("cardPage.backToCards")}
            className="flex size-11 shrink-0 items-center justify-center rounded-button text-text-1 hover:bg-surface-2"
          >
            <ArrowLeft className="size-5" aria-hidden="true" />
          </Link>
        )}
        <Link
          to={`/sets/${encodeURIComponent(view.setCode)}`}
          className="flex h-7 shrink-0 items-center rounded-sm bg-surface-2 px-2 text-12 font-semibold text-text-2"
        >
          {view.setCode}
        </Link>
        <span className="min-w-0 flex-1 truncate text-center font-sans text-15 font-semibold tabular-nums">
          {view.cardNo}
        </span>
        <IconButton label={t("cardPage.copyLink")} onClick={() => void copy()}>
          <Link2 className="size-5" aria-hidden="true" />
        </IconButton>
        <IconButton label={t("cardPage.share")} onClick={() => void share()}>
          <Share2 className="size-5" aria-hidden="true" />
        </IconButton>
      </div>
      {front && name && (
        <div className="mt-1 flex gap-3">
          <CardImage
            summary={summary}
            name={name.primary}
            images={images}
            alt="identify"
            sizes="96px"
            className="w-24 shrink-0"
          />
          <div className="min-w-0 flex-1">
            <h1 lang={name.primary.lang} className="text-20 leading-tight font-bold wrap-anywhere">
              {name.primary.text}
            </h1>
            {name.secondary && (
              <p lang={name.secondary.lang} className="text-13 text-text-2">
                {name.secondary.text}
              </p>
            )}
            {name.missingTranslation && (
              <p className="text-12 text-text-3">{t("card.noTranslation")}</p>
            )}
            <p className="mt-1 text-13 text-text-2">{metaText}</p>
            <p className="mt-2 flex flex-wrap gap-x-4 gap-y-1">
              <Stat
                icon={front.cost === null ? undefined : symbolIcon("cost", String(front.cost))}
                label={t("cardPage.stats.cost")}
                value={front.cost}
              />
              {front.attack !== null && (
                <Stat
                  icon={symbolIcon("power")}
                  label={t("cardPage.stats.attack")}
                  value={front.attack}
                />
              )}
              {front.defense !== null && (
                <Stat
                  icon={symbolIcon("hp")}
                  label={t("cardPage.stats.defense")}
                  value={front.defense}
                />
              )}
            </p>
          </div>
        </div>
      )}
      <div className="mt-3 flex flex-wrap items-center gap-2 text-13 text-text-2">
        {editionOptions.length > 1 ? (
          <Segmented
            label={t("cardPage.edition")}
            options={editionOptions}
            value={view.region}
            onChange={onEdition}
            size="sm"
          />
        ) : (
          <span>
            {t(`options.edition.${view.region}`)}
            {" · "}
            {view.mappingState === "confirmed_none"
              ? t("cardPage.noEnEdition")
              : t("cardPage.noMapping")}
          </span>
        )}
      </div>
      <div className="mt-2">
        {view.faces.map((face, index) => (
          <FaceSection
            key={face.faceId}
            face={face}
            view={view}
            uiLanguage={uiLanguage}
            first={index === 0}
          />
        ))}
      </div>
    </article>
  )
}
