import type { Messages } from "./zh-TW"

export default {
  app: {
    title: "sve-kit",
  },
  nav: {
    home: "Home",
    cards: "Cards",
    decks: "Decks",
    account: "Account and settings",
    expand: "Expand sidebar",
    collapse: "Collapse sidebar",
    primary: "Primary navigation",
    skipToContent: "Skip to content",
    logoAlt: "sve-kit",
  },
  account: {
    notSignedIn: "Not signed in",
    cardEdition: "Card edition",
    nameDisplay: "Card names",
    uiLanguage: "Language",
    appearance: "Appearance",
    accent: "Accent colour",
    allSettings: "All settings",
  },
  options: {
    edition: { jp: "Japanese", en: "English" },
    nameDisplay: { translated: "Translated", original: "Original", both: "Both" },
    language: { "zh-TW": "繁體中文", ja: "日本語", en: "English" },
    theme: { system: "System", light: "Light", dark: "Dark" },
    accent: { amber: "Amber", teal: "Teal", red: "Red" },
    region: { jp: "Japanese", en: "English" },
  },
  settings: {
    title: "Account and settings",
    display: "Display",
    effectLanguage: "Effect text",
    symbolLabels: "Label symbols",
    symbolLabelsHint: "Only keyword symbols get a label",
    banRegion: "Ban list region",
    dataSaver: "Data saver",
    dataSaverHint: "Card images load only when tapped",
  },
  pages: {
    home: "Home",
    cards: "Cards",
    card: "Card",
    sets: "Sets",
    decks: "Decks",
    decksStub: "The deck builder is still in the works; it will live here.",
    notFound: "Page not found",
    notFoundHint: "The address may be wrong, or this card is not in the database yet.",
  },
  footer: {
    unofficial:
      "sve-kit is an unofficial fan site with no affiliation to Cygames or Bushiroad. Card images and text belong to their respective owners; card images are reproduced under the official guidelines.",
    dataVersion: "Data version: {{version}}",
    dataVersionUnknown: "not loaded",
  },
  dialog: {
    close: "Close",
  },
} satisfies Messages
