# 牌組構築檢查規格

本文定義牌組驗證器的輸入、輸出與每條規則（[#1597](https://github.com/gbaian10/sve-kit/issues/1597) 構築規則、[#1598](https://github.com/gbaian10/sve-kit/issues/1598) 禁限）。
驗證器是純函式：**牌組＋構築索引＋賽制＋日期 → 違規清單**，不放進引擎。
同一套規則用在前端建牌器（即時提示、離線局開局）、對戰大廳（線上正式判定）與對局 Durable Object（用大廳驗過的那份）；
各處的實作語言可以不同，但都必須通過 [`deck-construction-cases.json`](deck-construction-cases.json) 的全部案例。

條號依官方綜合規則（総合ルール）ver. 1.28.0（2026-09-28）。英文版綜合規則與日文版條號相同處不另列；
條文沒寫清楚或日英有出入的地方列在 [§7 待確認](#7-待確認)，實作照本文暫定處理，不另行推測。

## 1. 輸入

### 1.1 牌組

| 欄位 | 內容 |
| --- | --- |
| `basis` | `{kind:"class", class?}` 或 `{kind:"title", title}`（6.1.1.5）。`class` 是建牌器記下的職業，可省略 |
| `leader` | 主戰者的卡片 ID，或 `null` |
| `main`、`evolve` | `[卡片 ID, 張數]` 的清單；同一 ID 不重複 |

牌組以永久卡片 ID 記卡，印刷別（printing）不影響檢查。候補區不送進驗證器。

### 1.2 構築索引（邏輯形）

構築索引是 carddb 匯出時另產的最小判斷資料，每個地區 jp／en 各一份，版本號與內容 hash 跟該次卡表快照綁在一起；
傳輸編碼（整數代號、字典編碼）由後續匯出契約定義。驗證器讀的是解碼後的邏輯形：

| 每張卡的欄位 | 內容 |
| --- | --- |
| `name` | 構築名稱。依第 1 面的卡名（6.1.1.4.1），不同卡片 ID 同名的卡同一組 |
| `class` | 職業代碼；中立是 `neutral` |
| `titles` | 作品名代碼清單；沒有作品名是空清單 |
| `zone` | 可放區：`leader`、`main`、`evolve`、`none`（token 等不能放進牌組的卡） |
| `copies` | 選填。卡片本文另定的張數，例如 `{main:50}`、`{evolve:10}`（6.1.2） |
| `singleton_except` | 選填。放進主牌組時，其他構築名稱各只能 1 張；清單內的構築名稱不受影響（6.1.2） |
| `start_amulet` | 選填。有【スタートアミュレット】能力（14.4.4） |

| 索引層級的欄位 | 內容 |
| --- | --- |
| `title_rules` | 有自己構築條件的作品名代碼 → 規則代號（目前只有 `vanguard`，14.4.2） |
| `formats` | 賽制代碼 → `{restrictions, coverage}`，見 [§5](#5-禁限1598) |

可放區由匯出端判定：種類リーダー → `leader`；特殊種類エボルヴ、アドバンス → `evolve`；特殊種類トークン → `none`；
其餘フォロワー、スペル、アミュレット → `main`。驗證器不重新解析種類文字。

### 1.3 情境

| 欄位 | 內容 |
| --- | --- |
| `format` | 賽制代碼，見 [§6](#6-賽制) |
| `date` | 判定日（`YYYY-MM-DD`），決定哪些禁限生效 |

## 2. 輸出

```json
{
  "legal": false,
  "violations": [{ "code": "sameNameOver", "clause": "6.1.1.4", "cards": ["…"], "values": {} }],
  "notes": [{ "code": "restrictionsUnknown", "clause": "restriction" }]
}
```

- `legal` 只看 `violations` 是否為空；`notes` 是無法檢查或資料不足的提示，不影響合法性
- `cards`：違規相關的卡片 ID，不重複、不排序（比對時當集合）；每種代碼幾筆、列哪些 ID 見下表
- `values`：顯示用的數值（張數、上限等），各代碼的欄位見 §3、§5
- 違規之間不排序；介面自行排序；同一代碼不會對同一分組重複出現

| 代碼 | 一筆的分組 | `cards` |
| --- | --- | --- |
| `leaderMissing`、`mainTooFew`、`mainTooMany`、`evolveTooMany`、`startAmuletMissing`、`formatUnsupported` | 整副牌一筆 | 空 |
| `leaderNotLeader`、`leaderClassMismatch`、`leaderTitleMismatch` | 整副牌一筆 | 主戰者 ID |
| `mainWrongCard`、`evolveWrongCard` | 該區每個卡片 ID 一筆 | 該 ID |
| `classMismatch`、`titleMismatch` | 每個卡片 ID 一筆（同 ID 在兩區也只一筆） | 該 ID |
| `unknownCard` | 每個卡片 ID 一筆（主戰者與兩區合併） | 該 ID |
| `sameNameOver` | 每個（區、構築名稱）一筆 | 該區中屬於該構築名稱的 ID；另一區的同名卡不列 |
| `startAmuletNames` | 整副牌一筆 | 兩區所有有【スタートアミュレット】的 ID |
| `banned`、`restricted`、`choiceRestricted` | 每筆禁限一筆 | 依成員 `scope` 計入張數的 ID |

## 3. 構築規則

| 代碼 | 條號 | 違規條件 | `values` |
| --- | --- | --- | --- |
| `leaderMissing` | 6.1.1.1 | 沒有主戰者 | |
| `leaderNotLeader` | 6.1.1.1 | 主戰者欄的卡可放區不是 `leader` | |
| `mainTooFew` | 6.1.1.2 | 主牌組少於 40 張 | `count, min` |
| `mainTooMany` | 6.1.1.2 | 主牌組多於 50 張 | `count, max` |
| `mainWrongCard` | 6.1.1.2 | 主牌組有可放區不是 `main` 的卡（主戰者、エボルヴ、アドバンス、トークン） | |
| `evolveTooMany` | 6.1.1.3 | 進化牌組多於 10 張 | `count, max` |
| `evolveWrongCard` | 6.1.1.3 | 進化牌組有可放區不是 `evolve` 的卡 | |
| `sameNameOver` | 6.1.1.4／6.1.2 | 同一區內構築名稱的張數超過上限（見 §3.1） | `name, section, count, limit` |
| `leaderClassMismatch` | 6.1.1.5.1 | 職業基準：牌組記下的職業不是中立，且和主戰者職業不同 | `deckClass, leaderClass` |
| `classMismatch` | 6.1.1.5.1 | 職業基準：卡的職業不是中立、也不是基準職業（見 §3.2） | `cardClass, deckClass` |
| `leaderTitleMismatch` | 6.1.1.5.2 | 作品名基準：主戰者沒有該作品名 | `title` |
| `titleMismatch` | 6.1.1.5.2 | 作品名基準：卡沒有該作品名 | `title` |
| `startAmuletMissing` | 14.4.2.1.1 | ヴァンガード作品名基準：主牌組沒有【スタートアミュレット】的卡 | |
| `startAmuletNames` | 14.4.4.1.1 | 主牌組與進化牌組的【スタートアミュレット】卡不只一個構築名稱 | |
| `unknownCard` | 6.1.1 | 卡片 ID 不在索引裡；這張卡不再做其他檢查 | |

卡片的可放區錯誤（`mainWrongCard`、`evolveWrongCard`）仍照常計入張數與同名檢查。

### 3.1 同名張數上限

主牌組與進化牌組分開計算（6.1.1.4：各 3 張、合計 6 張）。某區某構築名稱的上限依序決定：

1. 主牌組有任何一張帶 `singleton_except` 的卡，且該分組不在它的清單內 → 1（6.1.2）
2. 該分組任一張卡的 `copies` 有這一區 → 該值（6.1.2）。同一構築名稱、同一區的 `copies` 必須一致；
   不一致時由匯出端拒絕產出索引，驗證器不另訂合併規則
3. 其他 → 3（6.1.1.4）

上限來自 1、2 時 `clause` 寫 `6.1.2`，否則寫 `6.1.1.4`。

現有會改上限的卡（JP 卡號，EN 卡號不一定相同）：

| 卡 | 本文 | 索引 |
| --- | --- | --- |
| BP09-049 オニオン軍団 | 『オニオン軍団』は自分のデッキに50枚まで入れることができる。 | `copies:{main:50}` |
| BP11-045 ラピッドファイア | 『ラピッドファイア』は自分のデッキに6枚まで入れることができる。 | `copies:{main:6}` |
| CP01-085 にんじん、CP03-127 ドライブポイント、ECP01-058～062 等 | これはエボルヴデッキに10枚まで入れることができる。 | `copies:{evolve:10}` |
| BP19-110 機鋒の咎人・カットスロート | カード名に『カットスロート』を含まないカードは、メインデッキやエボルヴデッキにそれぞれ1枚までしか入れられない。 | `singleton_except:[名稱含カットスロート的分組]` |

依據：Q1184（オニオン軍団不受 3 張限制、不能放進化牌組）、Q407（にんじん 10 張）、
Q2609～Q2612（カットスロート在主牌組時，其他卡各 1 張，連 にんじん、オニオン軍団也只能 1 張；名稱含カットスロート的卡各 3 張；
只放進化面 BP19-111 不觸發，它沒有這段本文）。

「名稱含カットスロート」在匯出端依該地區的卡名展開成構築名稱清單，驗證器不比對文字。

### 3.2 基準

- **職業基準**（6.1.1.5.1）：基準職業是主戰者的職業；沒有有效的主戰者（缺少、不在索引或不是主戰者卡）時用牌組記下的 `class`，
  兩者都沒有就不檢查職業。
  牌組記下的職業是中立時（建牌器的「中立」選項，只用來篩卡池）不報 `leaderClassMismatch`，卡片仍依主戰者職業檢查
- **作品名基準**（6.1.1.5.2）：主戰者與兩副牌組的每張卡都要有該作品名；卡有多個作品名時只要包含即可
- 主戰者同時有職業與作品名時，由 `basis` 表示宣告哪一種（6.2.1.3）

### 3.3 作品名自己的構築條件（14）

- **ヴァンガード**（`title_rules` 為 `vanguard` 的作品名基準牌組）：
  - 14.4.2.1.1 至少 1 張【スタートアミュレット】→ `startAmuletMissing`
  - 14.4.2.1.2 有トリガー圖示的卡，職業要和主戰者相同 → **目前不檢查**：卡片資料還沒有トリガー圖示欄位，
    輸出 note `triggerClassUnchecked`（條號 14.4.2.1.2）
- **【スタートアミュレット】**（14.4.4.1.1）是卡本身的永續能力，與基準無關：牌組中有這類卡時，它們必須同名
- 其他作品名（ウマ娘、アイドルマスター シンデレラガールズ等）目前沒有構築條件；14.3.1.2 等是遊戲準備程序，不屬於構築

## 4. 不在範圍

- 6.2 之後的遊戲準備程序（引擎負責）
- 卡片的印刷別、語言版本、是否實際持有
- 「同名同種、表記不同」的卡：屬於同一張卡，由卡片 ID 處理

## 5. 禁限（#1598）

每個賽制有自己的禁限表與覆蓋區間，日版／英版各用自己的資料（`authored/construction-adoptions/`，匯出時投影到索引）。
匯出端依[既有採納契約](../schema/domains/construction-adoption.md)只投影已採納且限制本身 `state=confirmed` 的禁限；
保留其已知歷史與未來生效區間，不按匯出當日裁掉。`announced`、`withdrawn` 不投影成可套用的限制。
`coverage` 只投影已確認完整的日期範圍，不含 partial、未確認限制造成缺口或超出來源 `as_of` 的日期；
採納決定已確認，不代表限制已生效，也不代表禁限資料覆蓋未來。

| 欄位 | 內容 |
| --- | --- |
| `restrictions[]` | `{id, kind, from, until?, max?, max_groups?, members:[{name, scope, option?}]}` |
| `coverage[]` | `{from, until?}`：已確認禁限表的日期區間 |

- 生效：`from ≤ 判定日`，且 `until` 為空或 `判定日 < until`（`until` 不含當天）
- `scope`：`main`、`evolve` 或 `all`，指計算哪一區的張數
- `copy_limit`：成員構築名稱在 `scope` 內的張數合計超過 `max` → 違規。`max` 為 0 是 `banned`（禁止），
  其他是 `restricted`（殿堂入り 1 張）；`values: name, count, max, from`
- `choice_group`：牌組中出現的不同 `option` 超過 `max_groups` → `choiceRestricted`（英版 Gloryfinder 的「二擇一」；
  目前只有英版非 Standard 賽制有這種限制）；`values: count, max, from`
- 判定日不在任何 `coverage` 區間內時，照常套用已知的禁限並輸出 note `restrictionsUnknown`：沒有覆蓋不代表沒有禁限

條號欄寫 `restriction`（官方「カードの使用制限に関するルール」，不是綜合規則條文）。

## 6. 賽制

依[日版規則頁](https://shadowverse-evolve.com/rules/)、[日版禁限頁](https://shadowverse-evolve.com/card_limit/)、
[英版禁限頁](https://en.shadowverse-evolve.com/restrictions/)與 DeckLog 建牌頁（2026-11-03 查證）：

| 地區 | 賽制 | 官方禁限頁 | DeckLog | 本驗證器 |
| --- | --- | --- | --- | --- |
| jp | スタンダード | 有 | `N` スタンダード | `standard`：本文全部規則＋禁限 |
| jp | クロスオーバー | 另一頁 | `X` クロスオーバー | 未支援 |
| jp | グローリーファインダー | 未另確認 | 未確認 | 未支援 |
| en | Standard | 有 | `N` Standard | `standard`：本文全部規則＋禁限 |
| en | Cross Craft | 有 | 無 | 未支援 |
| en | Gloryfinder | 有（含 Choice-Restricted） | `G` Gloryfinder | 未支援 |

- クロスオーバー／Cross Craft 是附錄 B-2：主戰者 2 張、職業不同，只能職業基準，主牌組要有兩個職業各 1 張以上；
  需要牌組模型能記兩位主戰者，之後另開
- 日版也有[グローリーファインダー](https://shadowverse-evolve.com/rules/multiplayer/)；其主牌組固定 50 張、進化牌組至多 20 張，另選 1 張グローリーカード。
  日英 Gloryfinder 的完整構築規則（卡池範圍等）尚未整理進本文
- 附錄 B-1 シールド戦不是構築賽制，不檢查
- 情境的 `format` 不在索引的 `formats` 裡時，只回 `formatUnsupported`（`values: format`），不做其他檢查

## 7. 待確認

| 項目 | 內容 | 暫定 |
| --- | --- | --- |
| 殿堂入り的計算範圍 | 日版頁面寫「デッキに1枚だけ」，沒說主牌組與進化牌組是否合計 | 依禁限資料每筆成員的 `scope`；目前資料全是 `main` |
| 英版禁限「decks of the designated class」 | 英版頁面寫「when using decks of the designated class」，字面像是只限某職業的牌組；日版沒有此限定 | 不分職業，禁限一律套用 |
| トリガー圖示 | 14.4.2.1.2 需要卡片的トリガー圖示，資料還沒有 | 不檢查，輸出 note |
| Gloryfinder | 日英都有此玩法；完整構築規則與卡池未整理 | 未支援 |

## 8. 共用測試案例

[`deck-construction-cases.json`](deck-construction-cases.json) 每條規則至少一筆合規、一筆違規。格式：

| 頂層欄位 | 內容 |
| --- | --- |
| `cases_format` | 案例檔格式版本（目前 1） |
| `cr_version` | 依據的綜合規則版本 |
| `index` | 共用的邏輯索引：`cards`、`title_rules`、`formats` |
| `fillers` | 填充卡組：名稱 → `{class, titles, count}` |
| `cases[]` | `{id, rule, note, deck, context, expect}` |

- 填充卡：每組產生 `count` 張可放區 `main` 的卡，卡片 ID `FILL-<組名大寫>-<兩位序號>`、卡名 `合成：<組名> 填充 <兩位序號>`
  （例如 `FILL-ELF-01`、`合成：elf 填充 01`），先加進 `index.cards` 再跑案例。填充卡彼此不同名，也不和其他卡同名
- `deck` 同 §1.1；另可帶 `fill: {set, count}`：從該組第 1 張起依序每張放 3 張（最後一張放餘數），
  加在 `main` 的後面，直到加滿 `count` 張
- `expect.violations`、`expect.notes`：比對時不看順序；每筆比對 `code`、`clause`、`cards`（當集合比）；
  有寫 `values` 的只比對寫出的欄位
- 案例的索引是為測試寫的最小資料：卡號與卡名取自真實卡以便對照，欄位只填規則需要的
