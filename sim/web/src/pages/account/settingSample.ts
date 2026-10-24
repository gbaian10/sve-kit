import type { UiLanguage } from "../../i18n"

// A fixed copy of what the card pages show for BP01-004 (name, effect, Q152) and BP01-003 (flavor)
// on 2026-10-24: the official JP text and, in zh-TW, the adopted translation. Keep it in step when
// those translations change. The ja and en interfaces have no translation of these cards yet, so
// they show the original; that is exactly what the preview has to show too.
/** Text with the ranges the card page bolds as terms (the annotation's own `bold` flag). */
export type Segments = readonly (readonly [text: string, bold: boolean])[]

export interface SettingSample {
  readonly name: string
  readonly effect: Segments
  readonly flavor: string
  readonly question: string
  readonly answer: string
}

export const ORIGINAL_SAMPLE: SettingSample = {
  name: "リノセウス",
  effect: [
    ["{進化}{コスト0}：これは進化する。\n【", false],
    ["突進", true],
    [
      "】\n{ファンファーレ}これは{攻撃力}+Xする。Xは「これを含めず、このターン中に自分がプレイしたカードの枚数」である。",
      false,
    ],
  ],
  flavor: "生命とは輪廻、繰り返される循環。\n森が育んだ者たちは、やがて森を育む者へと変わる。",
  question:
    "このフォロワーの《ファンファーレ》能力で《攻撃力》+Xしたあと、このフォロワーが進化したとき、そのエボルヴフォロワーは《攻撃力》+Xを引き継ぎますか？",
  answer: "はい。引き継ぎます。",
}

const TRANSLATED_SAMPLES: Readonly<Record<UiLanguage, SettingSample | null>> = {
  "zh-TW": {
    name: "破魔蟲",
    effect: [
      ["{進化}{消費0}：這張卡片進化。\n【", false],
      ["突進", true],
      [
        "】\n{入場曲}這張卡片{攻擊力}+X。X為「不包含這次在內，本回合中自己使用的卡片張數」。",
        false,
      ],
    ],
    flavor: "生命即輪迴，反覆不斷的循環。\n森林孕育的生命，終將變成孕育森林的存在。",
    question:
      "這個從者因其{入場曲}能力而{攻擊力}+X之後，這個從者進化時，該進化從者會繼承{攻擊力}+X嗎？",
    answer: "是的。會繼承。",
  },
  ja: null,
  en: null,
}

/** The translation the card pages show in this interface language; null when there is none. */
export const translatedSample = (language: UiLanguage): SettingSample | null =>
  TRANSLATED_SAMPLES[language]

// BP01-031 as its card page shows it: terms sit mid-sentence here (a card name, then a keyword it
// gives to the summoned cards), so the bold switch visibly changes the line.
const TERM_ORIGINAL: Segments = [
  ["【", false],
  ["守護", true],
  ["】\n{ラストワード}『", false],
  ["スティールナイト", true],
  ["』2体を出す。それは【", false],
  ["守護", true],
  ["】を持つ。それを好きな枚数アクトしてよい。", false],
]

const TERM_TRANSLATED: Segments = [
  ["【", false],
  ["守護", true],
  ["】\n{謝幕曲}召喚2個『", false],
  ["鐵甲騎士", true],
  ["』。其具有【", false],
  ["守護", true],
  ["】。可以將其中任意數量的卡片橫置。", false],
]

export const termSample = (language: UiLanguage): Segments =>
  language === "zh-TW" ? TERM_TRANSLATED : TERM_ORIGINAL
