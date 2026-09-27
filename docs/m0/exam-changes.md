# M0 期間的改題紀錄

分支 `m0/baseline` 上對共同題本（基準 main `b96f6d8`）做的修改。之後要同步回 main 的
`tests/rules-scenarios/` 與 post-d-plan §2 的基準清單，並重跑兩邊或新基線。

## 1. card-BP02-062 B、C（2026-09-27）

| 項目 | 內容 |
| --- | --- |
| 題號 | card-BP02-062（must_pass） |
| 局面 | B「本回合沒捨過牌 → 竜少女 3 費 → 結束階段儀式不誘發、不造成傷害」、C「結束階段手牌上限的捨牌在誘發之後 → 誘發當下不算「捨てていた」→ 不誘發、不造成傷害」（名稱不變，保留局面鍵） |
| 決定者 | user，2026-09-27，做法 A |
| 相關裁定 | R-0009（長寫「〜とき、…なら」一律誘發，條件只在結算時查；取代舊裁定 #1） |
| setup | 不變 |

### 舊斷言

- 局面 B 的決定：打出竜少女 → end-phase → **P2 直接 quick pass**（沒有 P1 的 choose-pending）
- 局面 C 的決定：end-phase → **P2 直接 quick pass** → end-discard
- end-phase 後的比對點：`awaiting` 精確為 P2 的 `pass`；`semantic_state.pending_triggers: []`；
  `forbidden_events` 含 `{kind: 待機}` 與 `{kind: プレイ, ability: {source: m1, line: 1}}`；
  另有 `{kind: ダメージ}` 與體力／體力值斷言

### 新斷言

- 不斷言是否誘發或待機：刪除 `awaiting`、`semantic_state.pending_triggers`、`forbidden_events` 的 `待機` 與 `プレイ`
- 保留並加強可觀察結果：`P2.field.b1: {hp: 6}`、`P2.leader.life: 20`、`forbidden_events: [{kind: ダメージ}]`；
  回合推進到 P2 main、P2 手牌（B）、手牌數與墓場（C）不變
- 決定序列依 R-0009：end-phase 後加一個 `{by: P1, at: check-timing, do: choose-pending, pending: {ability: {source: m1, line: 1}}, targets: {1: [b1]}}`
  （打出時照選目標，R-0004），之後才是 P2 的 quick pass；比對點的決定編號隨之順延

### 理由

- Q302 日文答「プレイしません」，英文版同一則答「does not trigger」，兩者說法不同，不能直接證明「不誘發」
- Q940「2回誘発します」、Q852、Q2418 是「〜とき、…なら」一律誘發的直接證據（R-0009）
- 兩種讀法在本題的結果相同（儀式不造成傷害），所以改驗結果，不驗是否誘發

### 限制

共同題本的 `decisions` 是固定序列，兩種讀法的輸入點不同（R-0009 要先由 P1 打出待機能力），
所以一份題目不能同時讓兩種讀法的引擎都走完。斷言已不依賴是否誘發，但決定序列採用已裁定的 R-0009；
依舊讀法（不誘發）實作的引擎會在第 3 個決定（C 為第 2 個）被拒絕。

### 引擎側配合

- `authored/effects/BP02.yaml` 的 BP02-062 第 1 行由 `trigger_if` 改為本文 `if`
- 閘門：706 pass，已知失敗 0
