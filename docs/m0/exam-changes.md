# M0 期間的改題紀錄

引用與授權：局面名稱與正文中的官方卡名、日文卡文片段，以及 Q302（日／英）與 Q940 的短句是來源引用，不在本專案授權內。
決定序列、程式鍵名與中文推導屬專案；來源依 card-BP02-062 的 evidence 與文中 Q 編號定位。
來源、版本與權利界線見[文件引用說明](../quotations.md)。

分支 `m0/baseline` 上對共同題本（基準 main `b96f6d8`）做的修改。之後要同步回 main 的
`tests/rules-scenarios/` 與基準清單，並重跑兩邊或新基線。

## 1. card-BP02-062 B、C（2026-09-27）

| 項目 | 內容 |
| --- | --- |
| 題號 | card-BP02-062（must_pass） |
| 決定者 | user，2026-09-27，做法 A |
| 相關裁定 | R-0009（長寫「〜とき、…なら」一律誘發，條件只在結算時查；取代舊裁定 #1） |
| commit | `3c190e4`（改斷言與決定序列）、之後依審核 M-005 修正題目其他欄位與局面名稱 |
| setup | 不變 |

### 局面名稱

| 局面 | 舊名稱 | 新名稱 |
| --- | --- | --- |
| B | 本回合沒捨過牌 → 竜少女 3 費 → 結束階段儀式不誘發、不造成傷害 | 本回合沒捨過牌 → 竜少女 3 費 → 結束階段儀式誘發、結算時條件不成立、不造成傷害 |
| C | 結束階段手牌上限的捨牌在誘發之後 → 誘發當下不算「捨てていた」→ 不誘發、不造成傷害 | 結束階段手牌上限的捨牌在儀式結算之後 → 結算時不算「捨てていた」→ 不造成傷害 |

名稱是局面的鍵；`tests/rules-scenarios/g1-selection.yaml` 引用 B，已一併改成新名稱（G1 仍是 41 局面、內容不變）。第三輪回歸清單用 `all`，不受影響。同步回 main 時要一起帶過去。

### 舊斷言（依「誘發時判定、不誘發」）

- B 的決定：打出竜少女 → end-phase → **P2 直接 quick pass**
- C 的決定：end-phase → **P2 直接 quick pass** → end-discard
- end-phase 後的比對點：`awaiting` 精確為 P2 的 `pass`；`semantic_state.pending_triggers: []`；
  `forbidden_events` 含 `{kind: 待機}`、`{kind: プレイ, ability: {source: m1, line: 1}}` 與 `{kind: ダメージ}`

### 新斷言（依 R-0009：誘發、結算時條件不成立而不做）

- **決定序列要求有誘發**：end-phase 之後，B 的第 3 個、C 的第 2 個決定是
  `{by: P1, at: check-timing, do: choose-pending, pending: {ability: {source: m1, line: 1}}, targets: {1: [b1]}}`
  （打出時照選目標，R-0004），之後才是 P2 的 quick pass。依舊讀法（不誘發）實作的引擎會在這一步被拒絕
- 刪除 `awaiting`、`semantic_state.pending_triggers`，以及 `forbidden_events` 的 `待機` 與 `プレイ`
- 保留並加強結果斷言：`P2.field.b1: {hp: 6}`、`P2.leader.life: 20`、`forbidden_events: [{kind: ダメージ}]`；
  回合推進到 P2 main、P2 手牌（B）、手牌數與墓場（C）
- 題目其他欄位改成 R-0009 讀法：`mechanisms`、`why`、`discriminates`、`refs`（加 Q940、Q852、Q2418、10.7.1、10.7.2）、
  `evidence`（Q302 英文版的 derivation 改寫、新增 Q940）

### 理由

- 官方 Q&A 引用：Q302 日文答「プレイしません」，英文版同一則答「does not trigger」，兩者說法不同，不能直接證明「不誘發」
- 官方 Q&A 引用：Q940「2回誘発します」、Q852、Q2418 是「〜とき、…なら」一律誘發的直接證據（R-0009）
- 兩種讀法在本題的結果相同（儀式不造成傷害）

### 限制與待辦

- 共同題本的 `decisions` 是固定序列，契約沒有條件式決定；兩種讀法的輸入點不同，所以一份題目
  不能同時讓兩種讀法都走完。本題採用已裁定的 R-0009，**等於斷言有誘發**
- `status: verified` 與原 `verified_by`（astra，覆核的是改題前的 B／C）保留，並在題目中註明
  B／C 的部分已不適用。改題後的 B、C 已由 opus 在審核第 2 輪覆核通過（局面名稱、決定序列、斷言與其他欄位一致，
  並以變異確認抓得到「忽略なら」「7.4.7 捨牌算進去」「誘發時判定」三種錯誤實作），
  `verified_by` 加一筆 `by: opus`。審核建議的 `method: qa+mutation` 不在契約列舉（`qa | independent-derivation`）內，
  所以寫 `qa`，變異驗證寫在 note

### 引擎側配合

- `authored/effects/BP02.yaml` 的 BP02-062 第 1 行由 `trigger_if` 改為本文 `if`
- 閘門：706 pass，已知失敗 0
