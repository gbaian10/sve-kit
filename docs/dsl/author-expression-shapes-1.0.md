# 作者 1.0：數值表達式核心形狀

本表細化[撰寫語法 §5](author-syntax-1.0.md#5-表達式捕捉與選擇器)的數值核心，
機器語法以 [author-nodes.schema.json](../../dsl/author-nodes.schema.json) 為準。
`A.expr.numeric-core` 與 `A.cond.numeric-core` 是明示的片段型別，**不是完整 Expr／Cond**。
它們供節點契約驗證使用，不新增完整作者 reader，也不代表已通過來源或執行准入。
表中**每個 variant 定義也只在片段層封閉**；它們的 E、C、A.ref operand 目前不是完整值域。
完整型別可相容擴展這些 operand 參照，不縮減現在合法的輸入。
`cmp` 目前只定數值比較，其他同型值的比較形狀不在本表。

## 數值與葉參數的位置

- Int 是安全有號整數，bool 及數字字串不屬於 Int；負數要求量不在語法層截成零。
  浮點由嚴格 JSON／YAML 入口拒絕；JSON Schema 的 integer 無法區分 `1` 與 `1.0`。
- Nat 是安全非負整數。一般 selector 的 `cost_*` 等比較常量與精確數／upto 的數量位置
  不能直接套本核心或全域 `IntOrParam`。`any:true` 是另一個變體，不是數量值。
- 此核心只在 plus 的一個偏移 operand、minus 的第二個 operand 接受 `{param:ParamName}`。
  minus 第一個 operand、times、half_up、per.unit、min/max 的直接元素及比較直接 operand
  都不接受 param；內含 plus/minus 的遞迴節點仍照自己的白名單判斷。
- plus 的兩個 operand 不得同時是 param；來源的加法偏移角色、minus 的 subtract_offset
  及實值 Int 仍須承載／接線驗證，不能由 schema 的 `{param}` 推定已授權。
  接線必須用完整 operand 路徑對照來源 canonical 構造，不能只看最近的 plus／minus；
  外層相減或縮放會影響偏移的實際方向與幅度。
- `count.who/zone` 是 counted_owner/counted_zone；一般 `Sel.who/zone` 是
  source_owner/source_zone。這些構造不在本核心，必須使用不同位置型別，不能以
  `count: Sel` 的文法簡寫遺失來源角色。

## 逐 variant 矩陣

下列 mapping 均封閉，所列鍵全部必填、沒有選填欄位；外層各列互斥，不能混填。
`E` 表 `A.expr.numeric-core`；`C` 表 `A.cond.numeric-core`。
數值 binding 的型別與可見性另驗，不把任意字串當可執行算式。
裸別名已有 define 與頭部 x.from_payment 的讀法；bound 已有 define 與 declare 的讀法。
define 因而有兩種已記載的拼法重疊，本表不選標準形，也不推定兩種 body 的 hash 相同。
兩者都只能在本核心解析為數值；並非任意物件 binding 都可讀成數字。

| 定義／變體 | 必填形狀 | 選填 | 互斥／值域 | 責任與依據 |
| --- | --- | --- | --- | --- |
| A.expr.numeric-core／常量 | Int | 無 | 排除 bool、字串整數；float 由嚴格入口拒絕 | §5 有號算式 |
| A.expr.numeric-core／別名 | BindingName | 無 | 與保留物件／玩家名不相交 | §5 define、GAP-A-001 的 X |
| A.expr.plus | `{plus:[E,E]}` 或一個 operand 為 A.param | 無 | 恰兩項；不得兩項皆 param | 承載 §3 加法偏移角色 |
| A.expr.minus | `{minus:[E,E或A.param]}` | 無 | 恰兩項；第一項不得直接 param | 承載 §3 subtract_offset |
| A.expr.times | `{times:[E,E]}` | 無 | 恰兩項；不接受直接 param | §5 |
| A.expr.half-up | `{half_up:E}` | 無 | 單鍵 | §5 |
| A.expr.per-payload | `{of:E,unit:正安全整數}` | 無 | unit 不為零、不為 param／算式 | §5 每 unit；不建立零除 |
| A.expr.per | `{per:A.expr.per-payload}` | 無 | 單鍵 | §5 |
| A.expr.min | `{min:[E,…]}` | 無 | 非空列表 | GAP-A-028 |
| A.expr.max | `{max:[E,…]}` | 無 | 非空列表 | GAP-A-028 |
| A.expr.bound | `{bound:BindingName}` | 無 | 不接受 ObjectRef mapping | GAP-A-028；binding 的值型別另驗 |
| A.expr.power-of | `{power_of:A.ref}` | 無 | 不接受 Sel／領袖 recipient | §5；單一物件與 LKI 另驗 |
| A.expr.hp-of | `{hp_of:A.ref}` | 無 | 同上 | §5 |
| A.expr.cost-of | `{cost_of:A.ref}` | 無 | 同上 | GAP-A-028／055 |
| A.expr.resource-payload | `{who:A.player-ref,kind:pp_max或pp或ep或sep}` | 無 | who 不接受集合／param | GAP-A-005，回 Int |
| A.expr.resource | `{resource:A.expr.resource-payload}` | 無 | 單鍵 | GAP-A-005 |
| A.expr.turn-number-payload | `{who:A.player-ref}` | 無 | 不接受 param | GAP-A-056 |
| A.expr.turn-number | `{turn_number:A.expr.turn-number-payload}` | 無 | 單鍵 | 實際開始的玩家回合序數 |
| A.expr.zone-free-payload | `{who:A.player-ref,zone:field或ex}` | 無 | 不接受 param／zone 集合 | §14 欄位正規化，回 Nat |
| A.expr.zone-free | `{zone_free:A.expr.zone-free-payload}` | 無 | 不接受 Cond 的 ge／eq | §14 |
| A.cond.cmp-payload | `{left:E,op:eq或ne或lt或le或gt或ge,right:E}` | 無 | 不接受裸 param／物件集合 | §5；本片只有數值比較，同型值仍另驗 |
| A.cond.cmp | `{cmp:A.cond.cmp-payload}` | 無 | 單鍵 | GAP-A-005／028／055／056 |
| A.cond.all | `{all:[C,…]}` | 無 | 非空列表 | §5 組合條件 |
| A.cond.any | `{any:[C,…]}` | 無 | 非空列表 | §5；不是選取數量的 any:true |
| A.cond.not | `{not:C}` | 無 | 單鍵 | §5 |

`A.expr.per-payload.unit` 的正整數限制、all/any 非空限制、half_up 的單運元標量形，
以及 plus 偏移 param 可在任一側，是本表明確定形的雙向細化。
all、any、min、max 均允許單元素列表；min/max 非空另有 GAP-A-028 的明文依據。
不從 IR 型別偷偷推出其他撰寫形狀。`A.cond.numeric-core` 只收上表四種條件，
不收裸 bool，也不把讀值 `{zone_free:{who,zone}}` 視為條件。

## 片段邊界

本表不定義依賴 Sel、事件或 receipt 的 count、count_of 長短形、objects_of、
distinct_names、distinct_orig_costs、history_count、history_sum、lki_of、agg、event。
objects_of 是快照集合，不可作數值 operand；event 的數值欄位須按事件窄化，
不得設任意 payload 字串路徑。history.where 不可把所有事件欄位合成一個任意選用池。
hand、leader_life、counter、dice、paid、x_paid 的精確形狀亦不在本表；
其中 hand、leader_life 不依賴 Sel，但必須分清同名 Expr 與 Cond 的 payload。
本表不定義 `at` 捕捉的附掛位置；片段的封閉 mapping 不表示完整讀值語法禁止捕捉。

完整 Cond 的資源門檻、物件／規則狀態與歷史謂詞尚不在本核心。本表不定義 Sel 的 filter、
精確數／upto／any 或 first／second／rest 的編碼，也不提供有序／角色視圖的接線驗證。
完整 Sel 與動作位置的重疊仍須逐位置檢查；已定形的領袖葉分支與數量位置規則見
[角色與選取位置矩陣](author-selection-shapes-1.0.md)。
裸 BindingName 與未來可能使用的字串關鍵字須保持不相交，不能讓同一字串同時是
無條件接受的數值別名與不同構造；變更保留字也會影響共用引用原子。
現有引用矩陣的邊界不變，不宣稱整個 §13／§14 的能力已驗證。

Schema 根繼續拒絕 standalone 文件；本表不提供完整 authored／expanded profile。
本核心可遞迴含合法葉參數，不能當成 expanded profile。
