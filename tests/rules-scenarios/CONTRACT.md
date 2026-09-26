# 情境資料契約 v2.1（**凍結**，2026-09-25）

- 結構：第 0～8 節是基本格式，第 9 節是增補，第 10 節統一細節寫法；**後面的節與前面衝突時，以後面為準**
- 目的：讓同一題只有一種合法的正確答案；不同的規則引擎實作可以執行同一份題目、比對結果
- 原則：**中立**——用規則的詞彙描述局面、玩家決定與結果，**不能**寫成任何一種 DSL 的語法

## 0. 題目的層級

- 題目是**局面語意測試**：setup 是給定的局面，**不要求能由合法對局走到**；不驗證牌組構築是否合法
- 同一題的各局面**各自獨立**
- 題目中的物件 id 是**裁判用的代號**，不能交給受測的玩家或 AI 用來追蹤未公開的卡

## 1. 一題一檔

```yaml
schema: sve-exam/2.1
sources:
  rules: cr-1.27.0
  cards_jsonl_sha256: <完整 64 碼，或小寫十六進位前 16 碼>
id: card-BP20-P12             # card-<卡號> | rule-<條號>-<序號> | flow-<主題>-<序號>
kind: card                    # card | rule | flow
must_pass: true               # 是否列入必過門檻。與 kind 無關
mechanisms: [可選費用與範圍, "10.4.7"]
cards: [BP20-P12, BP21-016]   # 題目用到的**所有**卡號（含牌庫、墓場裡的真實卡）
why: <考什麼>
discriminates: <錯誤實作會得到什麼不同的結果>   # 必填；寫不出來代表局面還沒設計好
refs: {rules: ["10.4.7.1"], qa: ["Q2633"]}
evidence:
  - {ref: Q2633, quote: <原文>}
  - {ref: cr 10.4.7.1, quote: <原文>, derivation: [<推導步驟>]}
status: draft                 # draft | verified | disputed
author: fable
verified_by: []               # [{by: astra, method: qa | independent-derivation, note: …}]
scenarios: [...]
```

## 2. 局面（scenario）

```yaml
- name: A 付了追加費用
  inherit: <另一個局面的 name>    # 選填，見 2.6
  setup:
    turn:
      active: P1
      first_player: P1
      elapsed_turns: {P1: 4, P2: 3}   # 3.3.1：經過回合數是每位玩家各自的
      phase: main                     # start | main | end；戰鬥中加 step: "8.4.7"
    history: none                     # none | explicit（見第 3 節）
    players:
      P1:
        construction: class           # class | title（加 title: <作品名>）| crossover（加 classes: [..]）
        leader: {class: エルフ, life: 20}
        pp: {current: 4, max: 4}
        ep: 0                         # 預設 0
        sep: 0                        # 預設 0
        zones: {...}
  card_facts: {}                      # 見 2.5；放在局面層，不在 setup 裡
  decisions: [...]                    # 見第 4 節
  random: {}                          # 見第 5 節
  expected: [...]                     # 見第 6 節
```

### 2.1 卡片物件

- `id` 指**實體卡**。換區域後規則上是新物件（4.1.4），但 `id` 不變
- **進化後仍用原本的 id**（5.16.2），進化卡用 `evolved_with: <進化卡 id>` 關聯
- `state`（只寫與預設不同的；與 `history` 的預設衝突時**以 state 為準**）：

| 欄位 | 預設 | 說明 |
| --- | --- | --- |
| `acted` | false | アクト狀態 |
| `hp` | 卡面體力 | **目前體力，以它為權威** |
| `damage` | 0 | 語法糖：只用在沒有「体力＋N」「体力を N にする」等效果的物件上，＝卡面體力（含持續效果）− 目前體力 |
| `power` | 卡面攻擊力 | 目前攻擊力 |
| `counters` | `{}` | `{<計數器名>: n}` |
| `evolved` | false | 已進化；`evolved_with` 指向進化卡 |
| `entered_this_turn` | false | 本回合進場 |
| `face` | 0 | 雙面卡目前朝上的面（`faces[]` 索引） |
| `under` | `[]` | 疊放在下面的卡（由上到下） |
| `equipped_to` | null | 裝備對象 |

### 2.2 區域

| 鍵 | 規則區域 | 比對方式 |
| --- | --- | --- |
| `deck` | デッキ置き場 4.5 | **有順序**（由上到下），列出全部 |
| `evolve_deck` | エボルヴデッキ置き場 4.6 | 多重集合；表向き的加 `face_up: true` |
| `hand` | 手札 4.7 | 多重集合 |
| `field` | 場 4.4 | 多重集合 |
| `ex` | EX エリア 4.8 | 多重集合 |
| `cemetery` | 墓場 4.9 | 多重集合 |
| `banish` | 消滅領域 4.10 | 多重集合 |
| `resolution` | 解決領域 4.11 | 多重集合 |
| `evolution` | 進化領域 4.12 | 多重集合 |
| `race` | 出走領域 4.13 | 多重集合 |
| `drive` | ドライブ領域 4.14 | 多重集合 |
| `trigger` | トリガー領域 4.15 | 多重集合 |
| `equipment` | イクイップメント領域 4.16 | 多重集合 |

- 沒列的區域＝空。**setup** 裡 `[]` 與沒列相同；**expected** 裡寫了區域就比對整個區域，`[]`＝必須為空，沒列＝不比對
- 非公開區域（牌庫、手牌）的數量要完整（用 filler 補足）

### 2.3 filler（佔位卡）

- `{filler: N}` 只能放在**牌庫中不會被觀察的位置**與**手牌**；手牌裡的 filler 只允許被**計數**（手札の枚数）
- **場上的卡、目標、費用、捨棄、誘發來源、會被計數或比對條件的區域（例如墓場）一律用真實卡號**
- filler 被任何效果觀察到（抽到、翻開、搜尋、比對）＝**題目不完整**
- `expected` 的區域內容可以含 `{filler: N}`，比對時只比數量與位置；filler 不能出現在 `events`
- 需要「沒有能力的從者」時，選沒有能力文字的單面從者（可從 `cards.jsonl` 篩出）

### 2.4 token

- token 移到**不能存在的區域**後，依 9.1.4.4 **立即在該區域消去**（不等檢查時點，也在效果的後續處理之前）
  - 從者／護符 token 能存在的區域：EX、場、解決領域（9.1.4.1）；クレスト：EX；スペル：EX、解決領域
- 所以 expected **不能**斷言墓場等區域裡有這類 token；要考就用事件 `消去`
- 「消去」不是「消滅」（5.7），不能寫成移到 `banish`

### 2.5 卡片事實補丁（card_facts）

```yaml
card_facts:
  CP03-016: {trigger_icon: draw, source: "注釈文推認", verified: false}
```

- **以卡號為鍵**，同卡號的每一張都適用 → **同一個卡號不可能有兩種圖示**；需要不同圖示就用不同卡號
- 只補資料缺的欄位，不改寫既有欄位
- `verified: false` 是**暫定假設**，可以先寫題；但依賴它的題目**不能 `verified`、不能當必過題**，要等最小卡片事實契約補上真值
- 括號注釋（2.6.2 不影響遊戲）可以當推認來源，但只能標 `verified: false`
- `inherit` 會一併複製 `card_facts`

### 2.6 inherit

- 先複製被繼承局面的 `setup` 與 `card_facts`，再**深層合併**本局面寫的鍵：字典逐鍵合併，**列表整個取代**
- 不支援刪除鍵（不能用 `null` 刪）；不能循環繼承；`decisions`、`random`、`expected` **不繼承**

## 3. 歷史與規則狀態

| `history` | 意思 |
| --- | --- |
| `none` | **本回合**到目前為止沒有發生任何事件（開始階段的例行處理視為完成，但不產生任何回合計數）；沒有持續效果、延遲誘發、待機能力；每回合次數全部未使用；場上的卡都不是本回合進場 |
| `explicit` | 在 `semantic_state:` 明列；**沒列出的一律視同 `none`** |

```yaml
semantic_state:
  continuous_effects:
    - {id: e1, source: a5, text: "ターン終了まで、+2/+0", applies_to: [a6], until: end-of-turn, order: 1}
  delayed_triggers:
    - {id: d1, source: a5, text: "…", fires_at: "このターン終了時"}
  used_this_turn:
    - {ability: {source: a6, line: 2}, count: 1}
  counters_this_turn: {P1.ub_activated: 1}
  pending_triggers:
    - {id: t1, controller: P1, ability: {source: a7, line: 1, keyword: ラストワード}, event: {left_field: a7}}
  check_timing: {in_progress: true, rules_processed: true}
```

### 3.1 能力的指稱

```yaml
ability: {source: a1, card: CP04-104, face: 0, line: 1, keyword: UB}
```

| 欄位 | 必填 | 說明 |
| --- | --- | --- |
| `source` | 是 | 擁有這個能力的物件 id |
| `card` | 否 | 提供這段文字的卡號；預設是 `source` 的卡。**進化後**的能力來自進化卡（5.16.1.2），要寫進化卡的卡號 |
| `face` | 否 | `faces[]` 索引，預設 0 |
| `line` | 是 | 該卡該面 `text` 以換行切開後的第幾行（從 1 起算）；`sections` 裡的寫 `section: i` 加 `line` |
| `keyword` | 視情況 | **同一行有多個能力時必填**（12.1.2：「{ファンファーレ}{ラストワード}」是兩個能力；「【疾走】【ツインドライブ】」是兩個）；寫卡面上的關鍵字原文，不含括號 |
| `rule` | 視情況 | 由規則展開出的能力（例：【スタック】依 13.3.2.2 展開）寫條號 |

- 待機能力在**誘發時**就記住它的能力指稱；之後來源離場或換面，仍用誘發時的指稱
- 同一能力同時待機多次（10.7.2.1）時，用**能力指稱＋誘發事件**區分：`{ability: {...}, event: {left_field: a2}}`。
  setup 裡明列的待機另外有 `id`

## 4. 決定（decisions）

平鋪、有順序；`n` 從 1 連號。**結算中途的決定也佔一個 `n`**。

```yaml
decisions:
  - {n: 1, by: P1, at: main, do: attack, attacker: a1, target: P2.leader}
  - {n: 2, by: P1, at: check-timing, do: choose-pending, pending: {ability: {source: a1, line: 2, keyword: ツインドライブ}}}
  - {n: 3, by: P1, at: resolve, of: {decision: 2, occurrence: 1}, do: resolve-choice, choice: execute}
```

| 欄位 | 值 |
| --- | --- |
| `by` | P1 \| P2 |
| `at` | `main` \| `check-timing` \| `quick`（8.4.7、7.4.5）\| `resolve` \| `start` \| `end` |
| `of` | 只在 `at: resolve`：`decision` 指向**直接開始這次卡／能力結算的那個決定**（`play`、`activate`、`evolve`、`choose-pending`；**不是**引發它的 `attack`）；`occurrence: k`＝**這一次結算內**第 k 個需要玩家輸入的點，不計之後另外結算的待機能力 |

### 4.1 `do` 詞彙

| `do` | 規則 | 參數 |
| --- | --- | --- |
| `play` | 10.6 | `card`、`targets`、`costs`、`optional_costs`、`x`、`options` |
| `activate` | 起動能力 | `ability`、`targets`、`costs`、`optional_costs`、`x`、`options` |
| `evolve` | 12.2 | `source`、`ability`（有多個進化能力時必填）、`evolve_card`、`costs`、`pay: {pp: n, ep: 0\|1, sep: 0\|1}` |
| `attack` | 8.4 | `attacker`、`target`（物件 id 或 `P2.leader`） |
| `choose-pending` | 10.5.2.2／.3 | `pending`（setup 的 id，或 `{ability, event}`）、`targets`、`costs`、`optional_costs`、`x`、`options` |
| `resolve-choice` | 結算中途 | `choice: execute \| decline`、`select: [...]`（`[]`＝不找到）、`order: [...]`、`distribute: {id: n}`、`continue: true \| false` |
| `pass` | 放棄該出手時點 | — |
| `end-phase` | 結束目前階段 | — |

- **`evolve`**：進化能力的費用＝公開 `evolve_card`（12.2.2）＋卡面寫的費用。卡面費用含 PP 時，`pay` 處理 PP 部分：
  12.2.3 其中**最多 1 點**改用 EP，所以 `pp + ep`＝所選進化能力（套用修正後）的 **PP 費用**、`ep ≤ 1`；12.2.4 SEP 是**另外**付 1 點（`sep ≤ 1`）。
  卡面費用是其他處理（例：「手札3枚を捨てる」）時用 `costs`，此時 `pay: {pp: 0, ep: 0}`
- **`options`**：チョイス選中的**卡面選項序號**，一律寫列表（單選也寫 `[1]`），不可重複；
  依 5.18.3 在打出時決定；結算順序依卡面，不依列表順序
- **`targets`**：鍵是這個能力**效果文字**中第幾個「選ぶ」，依卡面文字順序編號（チョイス的各選項、條件分支裡的都算進編號）；
  沒有發生的不寫；「まで」選 0 個**必須寫** `{k: []}`（省略＝題目漏寫決定）
- **`costs`**：必要費用中需要玩家選的項目（例：「手札1枚を捨てる：」「場のアミュレット1つを墓場に置く：」），
  鍵是**費用文字**中第幾個需要選擇的項目，依卡面順序：`costs: {1: [a2]}`
- **拒付必要費用**寫 `costs: decline`（`activate`、`choose-pending`）：玩家選擇不支付這個能力的費用 →
  不能合法打出 → 起動能力 `cannot-activate`、待機能力 `pending-cancelled`。
  它和 `x: 0`（執行 0 單位，10.6.2.5.3 視為已支付）是**不同的決定**：例如 UB 拒付不算發動（Q2471）、X=0 算發動（Q2520）
- **`optional_costs`**：可選的追加費用。付就寫內容 `{additional: {discard: [a2]}}`，**不付寫 `{additional: decline}`**
- 只有一個候選的待機能力**仍要寫** `choose-pending`
- **不合法的嘗試可以寫進 decisions**，expected 必須對應 `cannot-*`（見 6.2）
- 自動能力的費用何時支付（打出時 10.1.1.2.2，或結算時 10.4.7.3）**依卡個別核對**，有 Q&A 就依 Q&A；
  不能只憑冒號的字形判斷

## 5. 受控亂數

```yaml
random:
  shuffles: [{player: P1, zone: deck, result: [a3, a9, {filler: 18}]}]
  random_selections: [{n: 1, from: P2.hand, result: [b4]}]
  dice: [3]
```

發生了卻沒寫到的亂數＝題目有瑕疵。

## 6. 預期結果（expected）

```yaml
expected:
  - at: after-decision-1
    view: omniscient
    outcome: resolved
    assert: {...}
    events: [...]
    events_exact: [プレイ]
    forbidden_events: [...]
    awaiting: {by: P1, choices: [...]}
```

### 6.1 比對點

**只有一種**：`after-decision-N`＝第 N 個決定送出後，引擎自動推進，停在**下一個需要玩家輸入的點**或**遊戲結束**。

- 包含：之後的規則處理、檢查時點裡不需要玩家選擇的部分
- 要比對某個檢查時點**結束後**的狀態，就用該檢查時點**最後一個決定**的 `after-decision-N`（它會推進到檢查時點結束後的下一個輸入點）
- 同一個 `at` 可以有多筆（不同 `view`），它們是**同一個比對點**

### 6.2 `outcome`

| 值 | 意思 |
| --- | --- |
| `resolved` | 第 N 個決定合法並已執行完：打出／能力已結算完；`attack` 則 8.4 的流程已推進到下一個輸入點；`pass`、`end-phase` 已生效 |
| `paused` | 停在第 N 個決定所屬結算的中途（等 `resolve-choice`）；**必須**寫 `awaiting` |
| `cannot-play` \| `cannot-activate` \| `cannot-evolve` \| `cannot-attack` | 玩家主動的行動不合法（10.6.2.3.3 等），**局面回到嘗試之前**；`assert` 寫回溯後的局面 |
| `pending-cancelled` | `choose-pending` 選的待機能力無法合法打出（沒有必要目標、費用付不出或拒付），依 10.7.3.2 **取消一次**；之前已發生的事（例如進化）**不回溯** |
| `game-end` | 遊戲在這段推進中結束（1.2.1：在那個時點立即結束，不再處理任何待機） |

### 6.3 `assert`

- 路徑：`P1.hand`、`P1.pp.current`、`P2.field.b1: {hp: 1}`、`P2.leader.life`、`semantic_state.pending_triggers`、
  `game: {ended: true, winner: P1, by: "11.2.1"}`
- **只比對有寫的鍵**；區域的比對方式見 2.2
- `view: P1／P2`：只能 assert 該玩家看得到的東西；對方的非公開區域用 `hand_count`、`deck_count`；
  `knowledge: {knows: [id], does_not_know: [id]}` 的最低語意＝「在這個比對點，該玩家**能識別出該區域裡的這張卡**」

### 6.4 `events`

- **比對範圍**：從**前一個不同的比對點**（第一個則從局面開始）到這個比對點。同一個 `at` 的各 `view` 共用同一個範圍
- **預設子序列**：列出的事件依序出現即可，中間可以有其他事件
- **`events_exact: [kind, ...]`**：只看這些種類的事件時，必須**完全等於**列出的那些（數量與順序都要一樣）。例：檢查時點的處理順序寫 `events_exact: [プレイ]`
- **`forbidden_events`**：這段範圍內**不能出現**的事件。「什麼都沒發生」要用它寫，不能用 `events: []`（子序列下空列表不代表沒有事件）
- **同時事件**：每個事件寫單一物件，同時發生的加相同 `group`，組內順序不比對
- `by`：`effect`（預設，帶 `source`）或 `rule-<條號>`（規則處理，10.12.1.2 沒有發生源）

| `kind` | 欄位 | 規則 |
| --- | --- | --- |
| `プレイ` | `object` 或 `ability` | 10.6 |
| `解決` | `object` 或 `ability` | 10.6 |
| `費用成立` | `ability`、`replaced: true`、`zero: true` | 10.6.2.5.2／.3 |
| `移動` | `object`、`from`、`to`、`position: top \| bottom` | 4.1.4 |
| `場に出す` | `object`、`from` | 5.5 |
| `引く` | `player`、`object`（有 id 時） | 5.10 |
| `捨てる` | `player`、`object` | 5.12 |
| `ダメージ` | `source`、`target`、`amount` | 5.14 |
| `破壊` | `object` | 5.6 |
| `消滅` | `object` | 5.7 |
| `消去` | `object` | 9.1.4.4（token） |
| `アクト` \| `スタンド` | `object` | 5.4 |
| `カウンター` | `object`、`name`、`delta`（＋置く／−取り除く） | |
| `体力増加` | `target`、`amount` | 5.27 |
| `回復` | `player`、`amount` | 5.15（PP） |
| `進化` | `object`、`with` | 5.16 |
| `攻撃` | `attacker`、`target` | 8.4.5 |
| `待機` | `ability`、`event` | 10.7.3 |
| `待機取消` | `ability`、`event` | 10.7.3.2 |
| `取代` | `object`、`original`、`replacement`（原文片段） | 10.10 |
| `敗北` | `player`、`by` | 11.2 |

要新增 kind 必須先寫進本契約與驗證器，不要各自發明。

### 6.5 `awaiting`

- 描述比對點**當下**停住的那個決定：`{by: P1, choices: [...]}`，列出**這一個決定階段**的**完整**合法選項
- 選項用 `decisions` 的形狀（不寫 `n`、`by`、`at`）：`{do: choose-pending, pending: {ability: {...}}}`
- `outcome: paused` 時必填；其他情況選填（沒寫＝不比對）
- 主要階段的完整行動集合很大，只在題目要考「某行動不合法」時才寫（例：選不到目標的法術不在選項裡）

## 7. 驗證規則

- `disputed`、`draft`、或依賴 `verified: false` 卡片事實的題目，**不能當必過題的驗收依據**
- 有直接 Q&A：與規則原文交叉核對；沒有 Q&A：另一方從同一日文原文**獨立推導**、寫下步驟，一致才算 `verified`
- 驗證時檢查：亂數寫全、filler 沒被觀察、token 沒被斷言在不能存在的區域、`history` 沒漏、預期結果唯一、`discriminates` 成立
- `validate.py` 是**中立的格式驗證器**（schema、卡號存在、`cards` 完整、id 唯一、決定連號、參照都指得到、`of` 指向合法、
  `do`／`kind` 在詞彙表內、同一卡號的 card_facts 不衝突）

## 8. 已知限制

- 5.10.3「N 枚まで引く」、5.11.2「N 枚まで見る」**目前沒有任何實卡**，沒有題目涵蓋。
  驅動檢查（14.4.5）考的是類似的「揭露 → 決定 → 繼續」流程，但**不等於**已測過這兩條
- 觸發圖示等卡片事實要逐張用官方卡圖確認後才能標 `verified: true`；已確認的見 `card-facts-verified.md`

---

## 9. 增補

實際出題時發現前面寫不出、或有兩種寫法的情況；★＝兩位出題者各自遇到。
**本契約不涵蓋**：手動／輔助模式、回放分支、永久循環（15.2）。

### 9.1 新建物件的代號 ★

- 局面中**新建立的物件**（token、生成的卡）依建立順序自動命名 `new-1`、`new-2`…（整個局面共用一個計數，從 1 起算）
- 同一個事件同時建立多個時，依效果文字的順序；順序無法由文字決定時，題目要改寫成可唯一決定的局面
- expected 可以用 `new-n` 引用：`P1.field: [a1, new-1]`、`P1.ex.new-2: {counters: {...}}`、事件 `{kind: 場に出す, object: new-1, to: P1.field}`（新建 token 不附 `card`，身分用 `name` 斷言，見第 10 節第 19、32 條）
- 區域數量：`P1.field_count: 3`（任何區域都可以加 `_count`）
- `new-n` 在**之後的決定與所有物件參照**中都能使用，包括能力指稱的 `source`（例：`ability: {source: new-1, card: EBD02-T01, rule: "13.3.2.2"}`）

### 9.2 規則程序中的玩家選擇 ★

不屬於任何卡或能力結算的選擇，用下列 `do`（`at` 照發生時點）：

| `do` | 規則 | 參數 |
| --- | --- | --- |
| `end-discard` | 7.4.7 手牌上限捨牌 | `select: [...]` |
| `guard-act` | 7.4.3 結束階段讓守護アクト | `select: [...]`（`[]`＝都不做） |
| `place-acted` | 12.8.2 守護等「可以アクト狀態放上場」 | `object`、`acted: true \| false`；在結算中發生時加 `at: resolve` 與 `of`，**算一個 occurrence** |
| `choose-first` | 6.2.1.6 先後攻 | `first: P1 \| P2`（見 9.13） |
| `mulligan` | 6.2.1.8 換牌 | `redo: true \| false`、`order: [...]`（放到牌庫底的順序，見 9.8） |
| `choose-start-amulet` | 14.4.3.1 | `object`；兩位玩家各自裏向選擇、互相看不到，decisions 依 P1、P2 的順序寫 |

### 9.3 打出時的分配 ★

- `distribute` 加進 `play`／`activate`／`choose-pending` 的參數（10.6.2.4：分配在打出時、費用之前決定）
- 形狀：`distribute: {k: {<目標 id>: n, ...}}`，`k` 是效果文字中第幾個「割り振る」指示（依卡面順序，同 `targets` 的編號規則）
- `resolve-choice.distribute` 只用在**結算時**才分配的效果

### 9.4 攻擊引起的選擇 ★

- `of.decision` 可以指向 `attack`：`occurrence` 計算**這次攻擊的 8.4 流程中**、不屬於另外結算的待機能力的玩家輸入點
  （例：攻擊傷害 8.4.9 的取代效果排序）
- 攻擊時觸發的自動能力照舊：由 `choose-pending` 開始結算，其中的輸入點 `of` 指向那個 `choose-pending`

### 9.5 多個取代效果的排序 ★

- `do: order-replacements`，`by`＝受影響的玩家（10.10.2），`order: [<能力指稱>, ...]`（先套用的在前）
- `at`／`of` 指向引起該事件的結算或攻擊

### 9.6 進化選面 ★

- `evolve` 加 `face: i`（雙面卡的進化卡要選哪一面時必填；`faces[]` 索引）

### 9.7 作品機制的付款與關聯 ★

- `activate` 也可以帶 `pay: {pp: n, ep: 0|1}`：規則允許以 EP 代付 PP 的起動（食事 14.2.2.4、憑依 14.4.9.2、アドバンス）
- 作品關聯寫在**被關聯的那張從者**的 `state.links`，值是關聯到它的卡的 id 列表；只有兩個鍵：
  - `出走: [<にんじん id>, ...]`：食事（14.2.1.1）把『にんじん』放到出走領域並關聯到這張從者；**沒有另外的「食事」鍵**，食事是動作、出走關聯是結果
  - `憑依: [<ドライブポイント id>, ...]`：憑依（14.4.9）後關聯的ドライブポイント
  - 從者離場時關聯解除（14.2.1.3、14.4.9.4）；裝備沿用 `equipped_to`
  - 例：`P1.field.a1: {links: {出走: [r1, r2]}}`、`P1.race: [r1, r2]`

### 9.8 `order` 的方向與複合選擇 ★

- **列表由前到後＝放完之後由上到下的相對順序**（放到牌庫底時也是：列表第一張在放完後最靠上）
- 一個指示同時要求選擇與排序（例：「N 枚選んで…残りを好きな順にデッキの下に置く」）時，**同一個** `resolve-choice` 帶 `select` 與 `order`；
  不同指示是不同的輸入點（不同 `occurrence`）
- **輸入點的切分**：以句號「。」切句，**每句最多一個輸入點**（句中沒有玩家選擇的就沒有，例：「残りをシャッフルし、デッキの下に置く」）；同一句內「選んで…置く」這類選擇加排序合併成一個 `resolve-choice`
  （例：ネフティス「4枚まで場に出してよい。残りを好きな順にデッキの下に置く。」是兩個輸入點）
- **只有一個合法選項也是輸入點**（包括只剩 `select: []` 或只剩 `decline`），比照 `choose-pending`；不要省略
- **能不能 `decline`／`select: []`** 依規則判斷，不是比對字串：
  - 可選費用（10.4.7）：依條文與直接 Q&A 決定何時可拒付（例：ダークアリス 的 LW 費用，Q492）
  - 必要費用：付不起完整費用就完全不付（10.4.2.2），**不能**只付一部分
  - `select: []`：「まで」、「好きな枚数」（10.6.2.3.2）、有枚數以外條件的「探す」找不到（4.1.2.2、5.8.1.2，例：Q2242）
- 「能做多少做多少」（1.3.2）只適用於**不是費用的強制效果**

### 9.9 語意狀態與事件欄位字典 ★

`semantic_state.counters_this_turn`（回合開始時歸零）：

| 鍵 | 意思 |
| --- | --- |
| `<P>.cards_played` | 本回合打出的卡張數（コンボ，含 0 費 token） |
| `<P>.evolve_played` | 本回合打出過的**進化能力與進化相當能力**次數（8.3.2.1 每回合 1 次的額度；食事 14.2.2.5、憑依 14.4.9.5、アドバンス 12.16.3 都算） |
| `<P>.evolutions` | 本回合實際發生的進化事件次數（含效果造成的進化；不代表額度） |
| `<P>.leader_damaged` | 本回合自己主戰者體力被減少的次數（真紅 13.5.2） |
| `<P>.ub_activated` | UB 發動次數（14.5.1.3） |
| `<P>.discarded` | 本回合捨棄的張數 |
| `<P>.dice` | 本回合擲出的骰值列表 |
| `<P>.attacks` | 本回合攻擊次數（被 Quick 破壞也算，Q1375） |
| `<id>.attacks` | 該物件本回合攻擊次數 |

物件的上場方式：`state.entered_from: hand | ex | cemetery | deck | evolve_deck | other` 與 `state.entered_by: play | effect | rule`
（「手札以外から場に出ていたなら」「能力によって場に出た」等條件用）

- `entered_from` 是 **5.5.3 的規則來源**：打出時記**打出前所在的區域**，不是緊鄰的解決領域；`resolution` 不是合法值
- 例：手牌打出 → `{entered_from: hand, entered_by: play}`；從 EX 打出 → `{entered_from: ex, entered_by: play}`；能力從墓場放上場 → `{entered_from: cemetery, entered_by: effect}`

待機的誘發事件鍵（`event`）：`left_field`、`entered_field`、`entered_ex`、`evolved`、`raced`、`attacked`、`damaged`、`discarded`、`drew`、`healed`，
值是相關物件 id；**同一物件多次滿足條件**時加 `n`（該物件在這個局面第 n 次，從 1 起算），例：出走 2 次 `{raced: a1, n: 1}`、`{raced: a1, n: 2}`

**新增的鍵必須先寫進本表**，不要各自發明。

### 9.10 可選追加費用的內容

`optional_costs.additional` 的鍵：`discard`、`banish`、`act`（アクト哪些）、`pp`、`stack_counter_from`、`return_to_deck`、`reveal`；
其他寫 `other: "<原文片段>"`。拒付仍是 `decline`。

### 9.11 事件補充

| `kind` | 欄位 | 規則 |
| --- | --- | --- |
| `公開` | `object`、`to: all \| P1 \| P2`、`source` | 5.21 |
| `場に出す` | 可帶 `card`（新建物件的卡號）、`to` | 5.5 |
| `抑制` | `ability`、`reason`（卡面原文片段） | 誘發條件成立但被效果阻止時**必發**（例：「ファンファーレは働かない」） |

### 9.12 其他

- **和局**：`game: {ended: true, winner: null, by: "1.2.2"}`；未結束是 `ended: false`
- **延遲誘發的能力指稱**：`{source: <建立它的物件>, line: n, delayed: true}`，在建立時保存；來源之後離場仍用它
- **card_facts 的欄位名**：`trigger_icon: critical | draw | stand | heal | none`（14.4.5.1.3.1～.4）、`rules_name`（作品卡在對戰中的名稱，例：魔法のアイテム，Q897）
- **玩家的非法決定**（例：`options: [1, 1]`）可以寫進 decisions 當反例，expected 必須是 `cannot-*`；驗證器只擋題目本身的語法錯誤
- **打出程序中由另一位玩家輸入**（例：「相手プレイヤー1人はチョイスする」，5.18.3 要求在打出時決定）：
  `resolve-choice` 的 `by` 是那位玩家、`options: [...]` 承載所選模式，`of` 指向正在打出的那個決定，**算一個 occurrence**；
  該決定之前的比對點 `outcome: paused`，`awaiting.by` 是做選擇的玩家
  - **限制**：目前只支援模式內沒有「選ぶ」的情況（實卡モートン BP05-006 三個模式都沒有目標）；模式內有目標的寫法尚未定義
- **關鍵字 N 選一**：`resolve-choice.keyword: 必殺`；**放牌庫頂或底**：`resolve-choice.position: top | bottom`
- **assert 路徑補充**：`turn.active`、`turn.phase`、`turn.elapsed_turns.P1`、任何區域的物件 `P1.<zone>.<id>: {...}`、`<zone>_count`；
  路徑可以寫到子樹（`P1.pp: {current: 2, max: 5}` 等同兩個鍵）
- **被給予的能力、規則給的能力**：沿用 3.1，`card` 寫提供文字的卡號、`keyword`、`rule`
- **knowledge**：「能識別」包含從公開資訊可推得的（例：墓場的卡回到手牌，對手知道手牌裡有它）
- **能力文字**：卡的能力＝`text` **加上全部 `sections`**（有被棄時／墓場能力的法術常把主效果放在 sections）

### 9.13 開局程序（6.2、14.3.1.2、14.4.3）

題目可以從**遊戲開始前**開始：

```yaml
setup:
  pregame: true                 # 從 6.2.1.1 開始，局面裡不寫 turn、pp 等（由程序決定）
  players:
    P1:
      construction: title
      title: カードファイト!! ヴァンガード
      leader: {class: ..., card: <卡號>}
      deck_list: [{id: a1, card: CP03-...}, ...]        # 主牌組全部（順序由 random.shuffles 決定）
      evolve_deck_list: [...]
random:
  first_chooser: P2               # 6.2.1.6 隨機決定由誰選先後
  shuffles: [{player: P1, zone: deck, result: [...]}]   # 6.2.1.4 的洗牌結果（完整順序）
decisions:
  - {n: 1, by: P1, at: pregame, do: choose-start-amulet, object: a7}
  - {n: 2, by: P2, at: pregame, do: choose-first, first: P2}
  - {n: 3, by: P2, at: pregame, do: mulligan, redo: false}
  - {n: 4, by: P1, at: pregame, do: mulligan, redo: true, order: [a3, a1, a9, a2]}
```

- `at: pregame`；決定的順序依規則（14.4.3.1 起始護符 → 6.2.1.6 先後 → 6.2.1.8 先攻者先換牌）
- 偶像大師的『魔法のアイテム』5 個（14.3.1.2）由規則自動放置，expected 用 `new-n` 引用
- 最後一個 pregame 決定之後，引擎完成 6.2.1.9～6.2.1.14（PP、EP、SEP、體力、開局後處理的能力）並照第 7 章推進到先攻玩家第 1 回合的第一個輸入點
- `deck_list` 的張數與構築是否合法**不驗證**（第 0 節：題目不是構築檢查）；只需包含題目用到的卡與足夠張數

---

## 10. 釐清（不改架構，只統一寫法）

下列細節曾有兩種寫法，以下為統一寫法，**與前面衝突時以本節為準**。

1. **沒有候選時不是輸入點**：規則程序的選擇（`guard-act`、`end-discard` 等）只有在**至少有一個可選對象**時才是輸入點。
   例：結束階段沒有直立的守護從者 → 不寫 `guard-act`。（對照：新上場的守護從者有「直立／アクト」兩個真實選項，**一定要寫** `place-acted`）
2. **9.4 `of: attack` 的 occurrence** 只計算帶 `of` 指向該攻擊的輸入點；攻擊流程中的 Quick 出手（`at: quick` 的 `pass` 或打出）**不算**
3. **10.7.2.2 同時滿足多個誘發條件、只待機一次**時，主人選擇「哪個條件成立」：以該能力的 `choose-pending` 表示，`awaiting.choices` 列出各個 `event`；
   這是把「待機形成時的選擇」合併到下一個輸入點的簡化，**只在待機形成到檢查時點之間沒有其他玩家輸入時使用**
4. **選擇尚未建立的物件**（例：4.4.4.2 場上不足時選要建立哪些 token）：`select` 用**卡號**；建立後依 9.1 命名 `new-n`
5. **「相手に示す」（例：5.8.1.2 搜尋結果）**寫成事件 `{kind: 公開, object, to: <對手>}`；**5.21「公開する」**寫成 `to: all`
6. **一個「選ぶ」裡含多組對象**（例：「相手のフォロワー1体と自分のフォロワー1体まで」）：`targets` 用同一個鍵、**平坦列表**列出全部所選物件，不拆成兩個鍵
7. **同一句裡不同玩家各自的選擇**（1.3.4 等）是**各自一個輸入點**，依規則的先後寫（「每句最多一個輸入點」只限同一位玩家）
8. **新建物件放到 EX、裝備區等場以外的區域**：事件寫 `{kind: 移動, object: new-n, to: P1.ex}`（新建 token 不附 `card`，見第 19 條），**不要**用 `場に出す`；`場に出す` 只用於真的放到場上
9. **換牌（6.2.1.8）與開局發牌（6.2.1.7）**是「移動」，不是 5.10 的「引く」：事件寫 `移動`，不寫 `引く`
10. **佔位卡不能被抽到**（2.3）也包括**推進到下一回合時的開始階段抽牌**：局面會跨回合時，被抽的那一方牌庫頂要放真實卡；不想跨回合就不要寫會跨回合的決定
11. **第 1 條只管規則程序本身的選擇**（7.4.3、7.4.7 等）。**卡片效果結算中的選擇照 9.8**：即使唯一合法選項是 `select: []`（例：4.4.4.2 場上已滿、選 0 個要建立的 token），仍是輸入點
12. **「探す」找到後的「相手に示す」（5.8.1.2）一律寫 `公開 to: <對手>`**，不論找到的卡之後放到手牌、場上或 EX（規則沒有依去向區分）
13. **同一筆費用裡的多個動作**（10.6.2.5「すべてのコストを支払います」，例：アクト自己＋アクト其他卡、兩次食事的にんじん）事件用同一個 `group`
14. **`view: P1／P2` 的 `awaiting`**：等的是**另一位玩家**的決定時，只寫 `by`，**不寫 `choices`**（選項可能是對方的私有資訊，例：對方私下看過的牌）。
    `choices` 寫在 `view: omniscient` 或做決定那位玩家的 view。runner 會掃描該 view 的完整投影，`knowledge.does_not_know` 的物件出現在任何地方都算洩漏
15. **結算中宣告數字或名稱**（例：「好きな数を1つ指定する」）：在該輸入點用 `resolve-choice` 的 `declare: <值>`。
    `awaiting.choices` 無法列盡任意值時，只寫 `by` 並在 `assert` 或事件驗證宣告後的結果；打出時就宣告的（10.6.2）寫在 `play` 的 `declare`
16. **效果中打出另一張卡或另一個能力**（例：「それのコストを0にしてプレイする」「【進化時】能力1つをプレイする」）：內層打出所需的選擇
    （`targets`、`costs`、`optional_costs`、`x`、`options`、`distribute`、`declare`）寫在內層的輸入點，用 `resolve-choice`，`of` 指向外層的決定；
    內層沒有任何選擇時不產生輸入點
17. **同一能力、同一誘發事件同時待機多次**（例：「追加で1回誘發する」）：`awaiting.choices` 把相同的 `choose-pending` **重複列出**（多重集合比對），
    決定照樣寫該指稱，引擎處理其中任一個；不另加識別欄位
18. **只洗牌庫的一部分**（例：「残りのカードをシャッフルしてデッキの下に置く」）：`random.shuffles[].result` 只列**這次被洗的卡**洗完後由上到下的排列，
    不含沒有受洗牌指示影響的其他牌；洗完放回後的完整牌庫順序另用 `expected` 的 `<P>.deck` 比對。
    引擎要檢查 `result` 的元素集合恰等於實際被洗的卡，且不改變其他牌的相對順序。例：牌庫原為 `[tail, …]`，公開的 `n2`、`large` 洗到底 →
    `shuffles: [{player: P1, zone: deck, result: [n2, large]}]`、`P1.deck: [tail, n2, large]`
19. **新建 token 的卡號**：卡文只以名稱指定 token 時（例：「大地の魔片」有多種印刷），具體印刷卡號沒有規則依據。
    `場に出す`／`移動` 事件對新建 token **不附 `card`**，改用 `new-n`、區域、數量、數值與 `name`（第 32 條）斷言；禁止事件也不要只擋某一種印刷。
    同名 token 的任何印刷都算正確；舊題已同步刪除，例外是依第 4 條用卡號選擇要建立哪個 token 的題目（rule-4.4.4-01）
20. **`awaiting.choices` 裡的 `choose-pending` 要展開參數組合**（6.5「完整合法選項」）：同一個待機能力，打出時每一種合法的
    `targets`、`costs`（含拒付 `costs: decline`）、`optional_costs`、`options`、`x` 等組合**各列一項**；
    沒有任何參數選擇的待機能力才只寫 `{do: choose-pending, pending: {...}}`。例：對象可選 `b1` 或主戰者 →
    `{..., targets: {1: [b1]}}`、`{..., targets: {1: [P2.leader]}}` 兩項
21. **事件 `解決` 發在解決完成時**：10.6.2.8 的效果全部執行、10.6.2.8.3 把卡移到墓場（或把能力移除）之後。
    在它之前支付的費用、打出事件，以及效果造成的事件，都排在 `解決` 前面
22. **計數器名稱寫卡面原文的全名，含「カウンター」**：`スタックカウンター`、`融合カウンター`、`返戻カウンター`。
    setup 與 expected 的 `counters` 鍵、事件 `カウンター` 的 `name` 都用全名，不寫簡稱（15.1.2.1 的「名称」與全名指同一種計數器，題目只用全名）
23. **缺少的 `links` 視為沒有關聯**：沒有關聯的局面，用關聯區域（`P1.drive: []`、`P1.race: []`）斷言，不寫 `links: {<鍵>: []}`。
    這不禁止其他空關聯斷言；但 runner 的 subset 比對無法用 `{}` 驗證「沒有關聯」，要直接驗證空 map 須另定精確比對
24. **「〜てよい」的結算中選擇**（例：「その中から…1枚を手札に加えてよい」）：做 → `{do: resolve-choice, choice: execute, select: [...]}`；
    不做 → `{do: resolve-choice, choice: decline}`（不帶 `select`）。`select: []` 只用在 9.8 的「選 0 個」（「まで」、「好きな枚数」、找不到），不能當成拒絕
25. **`awaiting.choices` 裡的 `resolve-choice` 也展開參數組合**：列盡該輸入點所有**可列舉**的合法參數組合各一項，含第 16 條的內層參數
    （`targets`、`costs`、`optional_costs`、`x`、`options`、`distribute`、`declare`）與 `select`、`position`、`order`、`keyword`、`choice`；
    第 15 條無法列盡的宣告只寫 `by`。例：「手札1枚をデッキの上か下に置く」、手牌 {h1, d1} → 4 項
26. **`place-acted`（12.8.2）是放上場之前的輸入點**：「置く際」的選擇在放置之前，`場に出す`（以及選 `acted: true` 時的 `アクト`）發生在該決定**之後**。
    在那之前的比對點可以用將要建立的 `new-n` 指稱該物件；這只是事先保留的指稱，不表示已上場
27. **expected 的 `semantic_state.*` 與第 3 節 setup 同形**：`used_this_turn` 是 `[{ability, count}]`，`pending_triggers` 是 `[{controller, ability, event}]`（`id` 只在 setup），
    `counters_this_turn` 是扁平字典。列表中的每筆物件可以多帶欄位，但**列表長度與重數仍精確比對**；新建物件（9.1）的 `entered_from` 是 `other`；
    【超進化時】（12.2.4.2）的誘發事件鍵也寫 `evolved: <id>`，與同一次進化的【進化時】由 `ability.line` 區分（例：rule-12.2.4-01、card-CP04-SL09）
28. **`抑制.reason` 寫造成禁止的完整一句原文**：以「。」切句（同 9.8），從句首到句號、**含句號**；`{…}`、【…】照 `cards.jsonl` 原樣，不截取片段
29. **規則處理事件的 `by`** 寫直接規定這個動作的**最細條號**（例：0 體力破壞 `rule-11.3.1`、token 移到不能存在的區域後消去 `rule-9.1.4.4`、
    裝備 token 失去關聯後消去 `rule-11.11.1`、advance 回進化牌庫 `rule-9.2.2`）；寫了就精確比對，不做前綴比對。和局只寫在 `game.by: "1.2.2"`，不寫進 `敗北.by`；同一次規則處理中多位玩家敗北時，`敗北` 事件共用同一個 `group`
30. **開局發牌（6.2.1.7）事件的 `group` 不比對**：規則沒有規定各玩家發牌的先後與是否同時，題目不斷言這些事件的 `group` 與玩家間的先後，
    改用手牌與牌庫區域驗證發牌結果。換牌（6.2.1.8）照規則「先攻プレイヤーから順に」，每位玩家的每次整批移動各自一組
31. **Quick 出手點（8.4.7／7.4.5）一律是輸入點**，即使唯一合法選項是 `pass`（「何もしない」也是玩家的決定；依手牌有沒有 Quick 卡決定要不要問會洩漏資訊）。
    這與第 1 條「沒有候選時不是輸入點」的規則程序選擇不同
32. **新建物件可以斷言 `name`**：`P1.ex.new-1: {name: ロイド}`。`name` 是卡片**當前朝上的面**的規則名稱，受 6.3 的可見性限制（隱藏物件看不到名稱）。
    同名多印刷的 token 用 `name` 斷言身分，事件不附 `card`（第 19 條）
