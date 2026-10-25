# DSL 1.0：引用、玩家與期限形狀

本表細化[DSL 語法 §6／§9](author-syntax-1.0.md#6-玩家引用結果)的引用原子；
機器語法以 [author-nodes.schema.json](../../dsl/author-nodes.schema.json) 為準。
每列先區分變體的必填、選填、互斥與角色，不從構造清冊的聚合鍵集合推導。
這些原子不依賴 Expr／Cond／Sel／Steps，不能當成動作的完整位置型別：
例如 `move.ref` 與 `damage.to` 還有 Sel 等合法形狀，須由各位置另組聯集。

## 名稱與保留字

`BindingName` 與 `RoleName` 的字元域是 `[A-Za-z][A-Za-z0-9_]*`，必須完整匹配，
不能帶尾端換行。前者另排除下表全部保留字，避免 `opponents` 被當成玩家 binding
而悄悄變成單一席位。大小寫不同的名稱不會自動正規化。

| 型別 | 值 | 角色 |
| --- | --- | --- |
| ReservedRefName | this、it、item、affected、found、entering、attack_target、host、cost1 | 物件引用的脈絡名稱；含物件集合與事件快照 |
| ReservedPlayerName | self、opp、turn、non_turn、current、item | 單一玩家；opp 仍依規則取唯一對手 |
| PlayerSetName | opponents、each、other_players | 玩家集合，保留原文基數 |

`item` 同時出現在前兩列，由 predicate／by_kind 分支的型別與作用域判定；
不代表同一 binding 可任意改型。`found` 只在 search 的相應 continuation 可讀，
`entering` 只在 entry_effects、`affected` 只在安裝效果的相應脈絡可讀。
其他名稱的宣告、型別、可見性、支配關係與單一物件基數仍須接線／lowering 驗證。
schema 通過不授予這些語義。

`RoleName` 不排除 binding 的保留字；它是另一個命名空間，包含 `primary`、`rest`。
角色是否由指定 binding 的 roles 宣告，須另驗證。
名稱字元域與 ParamName 相同不表示名稱是葉參數。

## 變體矩陣

`{…}` 是封閉 mapping；未列欄位一律拒絕。下表所有列都沒有選填欄位，
每個 mapping 的所列鍵全為必填，各列不能混填。

| Schema 定義／變體 | 必填形狀 | 選填 | 互斥／enum | 角色與依據 |
| --- | --- | --- | --- | --- |
| A.follow-source／binding | BindingName 字串 | 無 | 與保留字不相交 | ReceiptId 或 ReceiptList binding；§6、§14 A-016c |
| A.follow-source／mapping | A.event-move-receipt | 無 | 不接 entry_payment | 觸發移動的 after；§14 A-016b、A-016i、A-042a、GAP-A-060 |
| A.event-move-receipt | `{event: move_receipt}` | 無 | event 固定值 | 事件 header.receipt；§14 末段 |
| A.entry-payment-ref | `{entry_payment: additional}` | 無 | entry_payment 固定值 | 跨 execution 的入場付款讀值；GAP-A-054 |
| A.receipt-ref | A.follow-source 或 A.entry-payment-ref | 無 | 不接受物件引用 mapping | 合法 entry_payment 缺席不同於拼錯 binding |
| A.ref／scalar | ReservedRefName 或 BindingName | 無 | 兩域不相交 | §6、§5 item、GAP-A-055、§14 入場補記 |
| A.follow-ref | `{follow: A.follow-source}` | 無 | 不接 entry_payment | 只提供可能取得合法 after refs 的形狀；移動授權另驗 |
| A.role-selection | `{of: BindingName, name: RoleName}` | 無 | 不接任意 Ref 作 of | 宣告角色的目標 binding 與角色名；§14 A-016h |
| A.role-ref | `{role: A.role-selection}` | 無 | 與 follow／event 互斥 | 顯式角色，不從無序集合任取第一張 |
| A.event-object-ref | `{event: holder}` | 無 | event 固定 holder | 本文有明確物件讀法的事件欄位；GAP-A-058 |
| A.ref／mapping | A.follow-ref 或 A.role-ref 或 A.event-object-ref | 無 | 三種單鍵互斥 | 物件／集合引用原子，不包含領袖 recipient |
| A.player-ref／scalar | ReservedPlayerName 或 BindingName | 無 | 不收 PlayerSetName | 含 by_kind 的 item 與顯式 as 別名；§6、§14 末段 |
| A.owner-ref | `{owner_of: A.ref}` | 無 | 不接領袖 recipient | 物件 owner；不等於 controller |
| A.master-ref | `{master_of: A.ref}` | 無 | 與 owner_of 互斥 | 物件 controller；GAP-A-032 |
| A.player-ref／mapping | A.owner-ref 或 A.master-ref | 無 | 兩種單鍵互斥 | 單一物件基數、LKI 可用性另驗 |
| A.player-set | PlayerSetName | 無 | 不接 self／opp 或陣列 | 不因引擎目前二席而縮成 PlayerRef |
| A.single-leader | `{leader: self 或 opp}` | 無 | 不接其他 PlayerRef | §6 明列值域，與葉參數 Player 分開定義 |
| A.multiple-leaders | `{leaders: opponents 或 all}` | 無 | 不接單席 | GAP-A-020、§6 |
| A.leader-of | `{leader_of:A.ref}` | 無 | 不接 PlayerRef、param 或快照表達式 | Live controller 的領袖；離場依法 LKI，不是 owner |
| A.leader-recipient | A.single-leader、A.multiple-leaders 或 A.leader-of | 無 | 三種單鍵互斥，各納入一次 | 不混進 A.ref；leader_of 的單一物件基數另驗 |
| A.until／during_next_turn | during_next_turn:{who:PlayerRef} | 無 | 完整下次回合，不收 phase/edge/param | CP04-012；who 在安裝時捕捉 |
| A.until／short | turn、next_opp_turn、this_and_next_opp_turn、game | 無 | 四個字串值 | §9；不收無錨點舊別名 |
| A.phase-edge | `{who: A.player-ref, phase: Phase, edge: Edge}` | 無 | Phase=start/main/end；Edge=begin/end | 安裝時捕捉 who |
| A.turn-edge | `{who: A.player-ref, edge: Edge}` | 無 | 不收 phase | 指定玩家下回合邊界 |
| A.phase-interval | `{who: A.player-ref, phase: Phase}` | 無 | 不收 edge | 完整下一階段區間 |
| A.until-next-phase | `{next_phase: A.phase-edge}` | 無 | 單鍵 | §9 正文明示長形 |
| A.until-next-turn | `{next_turn: A.turn-edge}` | 無 | 單鍵 | §9 正文明示長形 |
| A.until-during-next-phase | `{during_next_phase: A.phase-interval}` | 無 | 單鍵 | GAP-A-063，不是到下次階段開始就到期 |
| A.until／mapping | 上述三種期限 wrapper 之一 | 無 | 不得混填 | 缺 who／phase／edge 不補預設 |

`A.follow-source` 不接受 `{entry_payment: additional}`：入場付款提供跨 execution 讀值，
不是已登錄的移動追蹤授權。普通 receipt binding 是否具有合法移動結果、ReceiptList
是否已離開迴圈、事件是否真有 move_receipt，以及新世代是否仍有效，仍須另驗。

`A.event-object-ref` 只收已有明確 DSL 形狀的 holder；
amount／value 是表達式讀值，move_receipt 是收據，不得混成物件。
holder 是否存在於該 on 事件仍須由事件型別推導。
不能把 IR 任意欄位直接當成DSL 欄位，也不能輸入自由字串路徑。

本表定義領袖葉形狀，不定義完整 Sel 的 filter 組合或 `rest`。
`first/second/rest` 的DSL 編碼須與 Target 的順序／角色視圖一併定形，不從 §6 的簡寫推斷出
`"first:t"` 或 `{first: t}` 都合法。這不刪除原語義，也不宣稱完整 Ref 語法已交付。
已定形的 Target roles 與接線責任見[角色與選取位置矩陣](author-selection-shapes-1.0.md)。

## 參數與入口邊界

依[承載契約 §3](author-contract-1.0.md#3-封閉葉參數)，本表各位置都不接受 A.param，
包含玩家、角色名稱及期限的 who／phase／edge。Phase 在事件的可參數化位置
不等於期限的 phase 也能參數化。以上原子可供 authored／expanded 共用，
不表示整棵樹的兩個遞迴入口已實作。

Schema 根仍拒絕 standalone 文件；沒有新的完整DSL reader。
Python 型別與每個定義的結構及合成正反例同步驗證。
這些驗證不提供來源准入、作用域、事件投影、receipt 授權或引擎 capability 保證。
