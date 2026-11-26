import { ArrowLeft } from "lucide-react"
import { useMemo } from "react"
import { useTranslation } from "react-i18next"
import { Link, useSearchParams } from "react-router"

import { useUiLanguageSync } from "../../app/language"
import { useThemeAttributes } from "../../app/theme-attributes"
import { battleBuild } from "../../battle/engine/assets"
import { type Scene, SCENES } from "../../battle/fixtures/scenes"
import { scriptedTable } from "../../battle/fixtures/scripted"
import { BattleScreen } from "./BattleScreen"
import { PracticeMatch } from "./PracticeMatch"

export function PlayPage() {
  useThemeAttributes()
  useUiLanguageSync()
  const [params] = useSearchParams()
  return battleBuild !== null && !params.has("scene") && !params.has("dual") ? (
    <PracticeMatch build={battleBuild} />
  ) : (
    <FixturePage />
  )
}

function FixturePage() {
  const { t } = useTranslation()
  const [params, setParams] = useSearchParams()
  const scene: Scene = SCENES.find((entry) => entry.id === params.get("scene")) ?? SCENES[0]
  const dual = params.get("dual") === "1"
  // One playing of the scene, shared by both seats so the other seat's screen follows what this
  // one answers. The sources hold nothing to release, so the page lets them go; closing one in an
  // effect cleanup would hand a closed source back when React re-runs effects.
  const seats = useMemo(() => scriptedTable(scene), [scene])
  const update = (change: (next: URLSearchParams) => void) => {
    const next = new URLSearchParams(params)
    change(next)
    setParams(next, { replace: true })
  }
  return (
    <main>
      <h1 className="sr-only">{t("play.title")}</h1>
      <BattleScreen
        key={scene.id}
        source={seats.p1}
        opposite={dual ? seats.p2 : undefined}
        leaders={{ p1: scene.leaders[0], p2: scene.leaders[1] }}
        slots={scene.slots}
        onDual={(next) => {
          update((query) => {
            if (next) query.set("dual", "1")
            else query.delete("dual")
          })
        }}
        corner={
          <>
            <Link to="/" className="bp-chip-link">
              <ArrowLeft aria-hidden="true" className="bp-chip-icon" />
              {t("play.leave")}
            </Link>
            <label>
              <span className="bp-chip-label">{t("play.dev.scene")}</span>
              <select
                value={scene.id}
                onChange={(event) => {
                  update((query) => {
                    query.set("scene", event.target.value)
                  })
                }}
              >
                {SCENES.map((entry) => (
                  <option key={entry.id} value={entry.id}>
                    {t(`play.dev.scenes.${entry.id}`)}
                  </option>
                ))}
              </select>
            </label>
          </>
        }
      />
    </main>
  )
}
