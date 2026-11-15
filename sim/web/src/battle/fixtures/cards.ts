// Real JP printings the fixture scenes are built from: two decks (a Dragon list around the Garmille
// leader and a Royal Levion list). Only identities and the facts a contract message carries live
// here; names shown on screen, card text and images come from the pinned snapshot.
import type { CardRef } from "../contract"

type FixtureKind = "follower" | "evolved" | "spell" | "amulet" | "token" | "leader"

interface FixtureCard {
  readonly printing: string
  readonly name: string
  readonly kind: FixtureKind
  readonly cost: number | null
  readonly stats: readonly [attack: number, life: number] | null
}

const card = (
  printing: string,
  name: string,
  kind: FixtureKind,
  cost: number | null,
  stats: readonly [number, number] | null = null,
): FixtureCard => ({ printing, name, kind, cost, stats })

const CARDS = {
  // Dragon
  "BP20-067": card("p:b8ffe3ef65d251e7ae5ea4e025bf9ef5", "侮蔑の国", "amulet", 1),
  "BP05-058": card("p:8721ce6047db5c3fbc8e2b7e2e6d7d9e", "侮蔑の炎爪", "spell", 1),
  "BP15-064": card("p:18eb38db94465bcc92e93eb884729bb7", "灯火の烈絶", "spell", 1),
  "BP08-068": card("p:fe26f6c486d65e11bf416cd151c2c997", "侮蔑の嘲笑", "spell", 1),
  "BP20-064": card("p:77e1d1a2c58458d694cda7d66d49eec7", "侮蔑の祈祷者", "follower", 2, [2, 2]),
  "BP08-063": card("p:b772e27071a65a5982a9a3296aa848c0", "侮蔑の狂信者", "follower", 2, [2, 2]),
  "SD04-002": card("p:ccbab1cc90dc50fb8a88367755e727d2", "竜の託宣", "spell", 2),
  "BP20-063": card("p:10c5937dcece5ec6965bdfb8d647e0e4", "蒼炎の猛威", "spell", 2),
  "BP15-113": card(
    "p:25e3e66859435fda8a9d45483a9c7708",
    "干絶の飢餓・ギルネリーゼ",
    "follower",
    2,
    [2, 2],
  ),
  "BP20-062": card("p:d3a99e49fb055eaa826e55ff3e74d70d", "侮蔑の団結者", "follower", 3, [3, 4]),
  "BP20-056": card(
    "p:8d333c2bfda45d80b1201bf4aee4d996",
    "烈絶の顕現・ガルミーユ",
    "follower",
    3,
    [3, 3],
  ),
  "BP15-066": card("p:f8d9a74325f1588588bea41263bbe568", "烈絶の崇拝者", "follower", 3, [3, 3]),
  "BP05-055": card("p:b65e0c1c59f557a18c86ac28bab1050d", "侮蔑の使徒", "follower", 3, [2, 4]),
  "BP20-057": card(
    "p:6dd1060b37a458329d9f2deb637a778b",
    "侮蔑の継承者・アジュラフリート",
    "follower",
    4,
    [3, 5],
  ),
  "BP15-057": card(
    "p:8f510474b21b5376ba2790d8c3b79101",
    "烈絶の侮蔑・ガルミーユ",
    "follower",
    5,
    [5, 5],
  ),
  "BP05-052": card(
    "p:8bc6c0124bc4532d9b72f9573eb7c910",
    "侮蔑の絶傑・ガルミーユ",
    "follower",
    5,
    [5, 5],
  ),
  "BP05-053": card(
    "p:53c4c19f9949568fa6737018ff6bafd1",
    "侮蔑の絶傑・ガルミーユ",
    "evolved",
    null,
    [6, 6],
  ),
  "BP20-065": card("p:68e449d5a0e25dd1ab2a518885e4c82e", "侮蔑の祈祷者", "evolved", null, [3, 3]),
  "BP15-114": card(
    "p:9954e3cdbc0d58b2bd7147a195061fcd",
    "干絶の飢餓・ギルネリーゼ",
    "evolved",
    null,
    [3, 3],
  ),
  "BP15-067": card("p:95f8f672c0a75c57bb6bac1ce0056726", "烈絶の崇拝者", "evolved", null, [4, 4]),
  "BP20-058": card(
    "p:8dee62a082595e7db640240a7228d46d",
    "侮蔑の継承者・アジュラフリート",
    "evolved",
    null,
    [4, 6],
  ),
  "BP05-056": card("p:a943993a32a85175990376ac4995b150", "侮蔑の使徒", "evolved", null, [3, 5]),
  "BP20-T05": card(
    "p:07a164ac38695df882ee79a5eec700d0",
    "クレスト 烈絶の顕現・ガルミーユ",
    "token",
    null,
  ),
  "BP20-T18": card("p:d4110954863d53b992ddd02bd5b2446a", "干絶の甘露", "token", 5),
  "SD04-T01": card("p:a453f592808f5105abafa0a23f482ccf", "ドラゴン", "token", 4, [5, 5]),
  "BP10-LD05": card("p:05ceb0c854405a8389934540a94c2201", "第五の絶傑・ガルミーユ", "leader", null),
  // Royal
  "BP13-026": card(
    "p:b9bb0db47fe25251acc59a5e4d3dd02a",
    "レヴィオンの見習い・ルネス",
    "follower",
    1,
    [1, 1],
  ),
  "BP13-032": card(
    "p:605262c62e4b59b7be5f2256ce479b4c",
    "レヴィオンの魔術師・ミイム",
    "follower",
    1,
    [2, 2],
  ),
  "BP16-026": card("p:7d3c917f9d2658e0ba9b360b44892480", "テンタクルバイト", "spell", 1),
  "BP13-035": card("p:3b56869f5dd159aeba98213068f521a3", "レヴィオンシスターズ登場！", "spell", 1),
  "BP16-031": card(
    "p:9a4d3705a192552abe72f45086dc8d06",
    "刹那のクイックブレイダー",
    "follower",
    1,
    [1, 1],
  ),
  "BP18-023": card(
    "p:125b7ca0914759bbbd3631c737514a64",
    "約束の栄光・ガウェイン",
    "follower",
    2,
    [3, 3],
  ),
  "SD07-012": card(
    "p:cf4c076872a55d0c8363cc25d2c3713e",
    "レヴィオンアックス・ジェノ",
    "follower",
    2,
    [3, 2],
  ),
  "BP13-030": card(
    "p:b632f105a40b56b8b334239cf11df8a2",
    "レヴィオンの副団長・マイム",
    "follower",
    2,
    [2, 2],
  ),
  "BP13-033": card(
    "p:2653c92fd508586984763fa3e077dabe",
    "レヴィオンの武闘家・メイム",
    "follower",
    2,
    [3, 2],
  ),
  "BP21-034": card(
    "p:820d599a0e3d59aabdaab0fc092fb299",
    "レヴィオンの弓使い",
    "follower",
    2,
    [2, 2],
  ),
  "SD07-016": card(
    "p:57c720f29bef57ad969c3700769cad89",
    "レヴィオンデューク・ユリウス",
    "follower",
    2,
    [0, 4],
  ),
  "BP21-022": card(
    "p:12f0285d05b955c2b114f85ffb30b338",
    "レヴィオンの超越者・ユリウス",
    "follower",
    3,
    [3, 4],
  ),
  "BP16-021": card(
    "p:53e4e81ebc9d532bb36ecd86d46bd329",
    "レヴィオンの迅雷・アルベール",
    "follower",
    4,
    [4, 5],
  ),
  "BP13-028": card(
    "p:754fe4951faa5213a4fc10f175959a79",
    "暴威の武人・ジェノ",
    "follower",
    4,
    [4, 3],
  ),
  "BP13-019": card(
    "p:d6ae070cecd9511c8c2387855afc00ad",
    "雷滅卿・アルベール",
    "follower",
    6,
    [3, 5],
  ),
  "BP16-032": card(
    "p:cf65f952cfcb581496f3f88ea307eac2",
    "刹那のクイックブレイダー",
    "evolved",
    null,
    [2, 2],
  ),
  "BP13-027": card(
    "p:ec76910c6029561dbf70a6fb9a0eff8a",
    "レヴィオンの見習い・ルネス",
    "evolved",
    null,
    [2, 2],
  ),
  "BP13-031": card(
    "p:c90dbe152144553599d6a06f94543225",
    "レヴィオンの副団長・マイム",
    "evolved",
    null,
    [3, 3],
  ),
  "BP13-020": card(
    "p:55d8974137f95504823785264eb2b727",
    "雷滅卿・アルベール",
    "evolved",
    null,
    [4, 6],
  ),
  "BP05-LD02": card("p:7c1a697f0ea4559483093bc83434df41", "雷迅卿・アルベール", "leader", null),
} as const satisfies Record<string, FixtureCard>

export type CardNo = keyof typeof CARDS

const CARD_TYPE: Record<FixtureKind, string> = {
  follower: "フォロワー",
  evolved: "フォロワー・エボルヴ",
  spell: "スペル",
  amulet: "アミュレット",
  token: "トークン",
  leader: "リーダー",
}

export const cardRef = (no: CardNo): CardRef => ({ printing_id: CARDS[no].printing, face: 0 })
export const fixtureCard = (no: CardNo): FixtureCard => CARDS[no]
export const cardTypeOf = (no: CardNo): string => CARD_TYPE[CARDS[no].kind]
