import type { Messages } from "./zh-TW"

export default {
  app: {
    title: "sve-kit",
    tagline: "Shadowverse: EVOLVE 非公式カードリスト・対戦ツール",
  },
  nav: {
    home: "ホーム",
    cards: "カード検索",
    decks: "デッキ",
    account: "アカウントと設定",
    expand: "サイドバーを開く",
    collapse: "サイドバーを閉じる",
    primary: "メインナビゲーション",
    skipToContent: "本文へ移動",
    logoAlt: "sve-kit",
  },
  account: {
    notSignedIn: "未ログイン",
    cardEdition: "カードの版",
    nameDisplay: "カード名の表示",
    uiLanguage: "表示言語",
    appearance: "外観",
    accent: "アクセントカラー",
    allSettings: "すべての設定",
  },
  options: {
    edition: { jp: "日本語版", en: "英語版" },
    nameDisplay: { translated: "訳名", original: "原文", both: "両方" },
    language: { "zh-TW": "繁体字中国語", ja: "日本語", en: "英語" },
    theme: { system: "システムに従う", light: "ライト", dark: "ダーク" },
    accent: { amber: "アンバー", teal: "ティール", red: "レッド" },
    region: { jp: "日本", en: "英語圏" },
    on: "オン",
    off: "オフ",
  },
  settings: {
    title: "アカウントと設定",
    display: "表示",
    effectLanguage: "効果テキスト",
    symbolLabels: "アイコンに名称を添える",
    symbolLabelsHint: "キーワード系のみ名称を表示します",
    banRegion: "禁止・制限の地域",
    dataSaver: "データ節約",
    dataSaverHint: "カード画像を自動で読み込まず、タップで読み込みます",
    about: "このサイトについて",
    report: "問題を報告",
  },
  pages: {
    home: "ホーム",
    cards: "カード検索",
    card: "カード",
    sets: "カードセット",
    decks: "デッキ",
    decksStub: "デッキビルダーは制作中です。完成したらここに表示されます。",
    notFound: "ページが見つかりません",
    notFoundHint: "URL が間違っているか、このカードはまだ収録されていません。",
  },
  footer: {
    unofficial:
      "sve-kit は非公式のファンサイトで、Cygames・ブシロードとは関係ありません。カード画像とテキストの著作権は各権利者に帰属し、カード画像は公式ガイドラインに従って転載しています。",
    dataVersion: "データバージョン：{{version}}",
    dataVersionUnknown: "未読み込み",
  },
  dialog: {
    close: "閉じる",
  },
} satisfies Messages
