const zhTW = {
  app: {
    title: "sve-kit",
  },
  nav: {
    home: "首頁",
    cards: "查卡",
    decks: "牌組",
    account: "帳號與設定",
    primary: "主要導覽",
    skipToContent: "跳到主內容",
    logoAlt: "sve-kit",
  },
  account: {
    notSignedIn: "尚未登入",
    cardEdition: "卡片版本",
    nameDisplay: "卡名顯示",
    uiLanguage: "介面語言",
    appearance: "外觀",
    accent: "強調色",
    allSettings: "所有設定",
  },
  options: {
    edition: { jp: "日版", en: "英版" },
    nameDisplay: { translated: "譯名", original: "原文", both: "兩者" },
    language: { "zh-TW": "繁體中文", ja: "日本語", en: "English" },
    theme: { system: "跟隨系統", light: "淺色", dark: "深色" },
    accent: { amber: "琥珀", teal: "青綠", red: "紅" },
    region: { jp: "日本", en: "英文圈" },
  },
  settings: {
    title: "帳號與設定",
    display: "顯示",
    effectLanguage: "效果文字",
    symbolLabels: "圖示旁附名稱",
    symbolLabelsHint: "只有關鍵字類會顯示名稱",
    banRegion: "禁限地區",
    dataSaver: "省流量",
    dataSaverHint: "不自動載入卡圖，點了才載入",
  },
  pages: {
    home: "主頁",
    cards: "查卡",
    card: "單卡",
    sets: "卡包",
    decks: "牌組",
    decksStub: "建牌器還在製作中，之後會在這裡。",
    notFound: "找不到這一頁",
    notFoundHint: "網址可能打錯了，或這張卡還沒收錄。",
  },
  search: {
    placeholder: "卡名或卡號",
    label: "搜尋卡片",
    clear: "清除",
    filters: "篩選",
    filtersSoon: "篩選面板即將推出",
    suggestions: "建議",
    seeAll: "看全部 {{count}} 張",
    noSuggestions: "沒有符合的卡",
    recent: "最近查看",
    clearRecent: "清除",
    noRecent: "還沒看過任何卡。打卡號的一部分或卡名的兩個字就會出現建議。",
    matched: "符合：{{field}}",
    field: { cardNo: "卡號", name: "卡名", alias: "別名" },
    classes: "職業",
    neutral: "中立",
  },
  results: {
    countUnit: "張卡片",
    loadMore: "載入更多",
    empty: "沒有符合的卡",
    emptyHint: "試試放寬條件，或改用卡號、別的寫法。",
    clearAll: "清除全部條件",
    loadFailed: "載入失敗",
    loadFailedHint: "請檢查連線後重試。",
    retry: "重試",
    loading: "載入中",
  },
  card: {
    imageAlt: "{{name}}（{{cardNo}}）",
    noImage: "尚無卡圖",
    imagePending: "卡圖待確認",
    withdrawn: "卡圖已撤下：{{reason}}（來源 {{host}}）",
    noTranslation: "尚無譯名",
    loadImage: "載入卡圖",
  },
  footer: {
    unofficial:
      "sve-kit 是非官方的粉絲網站，與 Cygames、Bushiroad 無關。卡片圖片與文字的版權屬各權利人，卡圖依官方指引轉載。",
    dataVersion: "資料版本：{{version}}",
    dataVersionUnknown: "尚未載入",
  },
  dialog: {
    close: "關閉",
  },
  dev: {
    root: { cdn: "正式", preview: "預覽" },
    status: {
      idle: "未載入",
      loading: "載入中：{{phase}}",
      error: "錯誤：{{kind}}",
      updateFailed: "{{version}}（更新失敗：{{kind}}）",
      updating: "{{version}}（更新中：{{phase}}）",
    },
    phase: { index: "版本索引", manifest: "快照清單", bootstrap: "啟動包" },
    error: { network: "網路失敗", incompatible: "網站需要更新", corrupt: "資料損毀" },
    retry: "重試",
    usePreview: "改用預覽資料",
    useCdn: "改回正式資料",
  },
} as const

type Widen<T> = { readonly [K in keyof T]: T[K] extends string ? string : Widen<T[K]> }

/** zh-TW is the reference shape; the other locales must have exactly the same keys. */
export type Messages = Widen<typeof zhTW>

export default zhTW satisfies Messages
