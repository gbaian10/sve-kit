import type { UiLanguage } from "../../i18n"

// The official JP text of BP01-004 (name, effect, Q152) and BP01-003 (flavor) shows what each
// display setting does without needing the snapshot. The zh-TW and English wording is only
// illustrative: the preview shows the layout of a setting, not the card's adopted translation.
export interface SettingSample {
  readonly name: string
  readonly effect: string
  readonly flavor: string
  readonly question: string
  readonly answer: string
}

export const ORIGINAL_SAMPLE: SettingSample = {
  name: "リノセウス",
  effect:
    "{進化}{コスト0}：これは進化する。\n【突進】\n{ファンファーレ}これは{攻撃力}+Xする。Xは「これを含めず、このターン中に自分がプレイしたカードの枚数」である。",
  flavor: "生命とは輪廻、繰り返される循環。\n森が育んだ者たちは、やがて森を育む者へと変わる。",
  question:
    "このフォロワーの《ファンファーレ》能力で《攻撃力》+Xしたあと、このフォロワーが進化したとき、そのエボルヴフォロワーは《攻撃力》+Xを引き継ぎますか？",
  answer: "はい。引き継ぎます。",
}

const ENGLISH_SAMPLE: SettingSample = {
  name: "Rhinoceroach",
  effect:
    "{icon_evolve|[evolve]}{icon_cost00|[cost00]}: Evolve this follower.\n【Rush】\n{icon_fanfare|[fanfare]} Give this follower {icon_power|[attack]}+X. X equals the number of cards you've played this turn, excluding this card.",
  flavor:
    "Life is reincarnation, a cycle that repeats.\nThose the forest raised in time become those who raise the forest.",
  question: "Does this follower keep the +X attack from its Fanfare ability even after it evolves?",
  answer: "Yes. It is a persistent effect.",
}

const TRANSLATED_SAMPLES: Readonly<Record<UiLanguage, SettingSample>> = {
  "zh-TW": {
    name: "破魔蟲",
    effect:
      "{進化}{消費0}：這張卡進化。\n【突進】\n{入場曲}這張卡{攻擊力}+X。X為「不含這張卡，這回合你已使用的卡牌張數」。",
    flavor: "生命就是輪迴，不斷重複的循環。\n森林所養育的人，終將成為養育森林的人。",
    question: "這個從者的入場曲能力使{攻擊力}+X之後進化的話，進化從者會繼承{攻擊力}+X嗎？",
    answer: "會。會繼承。",
  },
  ja: ENGLISH_SAMPLE,
  en: ENGLISH_SAMPLE,
}

export const translatedSample = (language: UiLanguage): SettingSample =>
  TRANSLATED_SAMPLES[language]
