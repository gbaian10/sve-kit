# 作者 1.0：數值、條件與收據讀值形狀

本表細化[撰寫語法 §5／§6](author-syntax-1.0.md#5-表達式捕捉與選擇器)的數值、條件與收據讀值，
機器語法以 [author-nodes.schema.json](../../dsl/author-nodes.schema.json) 為準。
`A.expr.numeric` 與 `A.cond` 是共用遞迴圖中的數值與條件共用入口型別，**不是完整 Expr／Cond**。
它們供節點契約驗證使用，不新增完整作者 reader，也不代表已通過來源或執行准入。
表中**每個 variant 定義也只在片段層封閉**；它們的 E、C、A.ref operand 目前不是完整值域。
完整型別可相容擴展這些 operand 參照，不縮減現在合法的輸入。
`cmp` 收數值比較與同型 PlayerExpr 的 eq/ne；history、paid、chosen 與捕捉形狀見
[事件與歷史矩陣](author-history-shapes-1.0.md)。其他同型值比較尚未登錄。
非 Int 的全部分支抽為同一 A.expr.numeric-operation／NumericOperation，
NumericExpr 為 Int 或 NumericOperation；Condition、ObjectSel、CountSource、CollectionSource
與 Quantity 在 Python 的 author_queries 中共同解析遞迴，不另立平行的核心圖。
[要求量矩陣](author-quantity-shapes-1.0.md)
以 Nat 配同一 operation 建立 Quantity，沒有另複製一份算式清單。

## 數值與葉參數的位置

- Int 是安全有號整數，bool 及數字字串不屬於 Int；負數要求量不在語法層截成零。
  浮點由嚴格 JSON／YAML 入口拒絕；JSON Schema 的 integer 無法區分 `1` 與 `1.0`。
- Nat 是安全非負整數。selector 的已登錄比較鍵與精確數／upto 的要求量使用 Quantity；
  它們接受共用算式，但字面量不能是負數，也不能直接套全域 `IntOrParam`。`any:true` 是另一個變體，不是數量值。
- 算術直接 operand 只在 plus 的一個偏移 operand、minus 的第二個 operand 接受 `{param:ParamName}`。
  minus 第一個 operand、times、half_up、per.unit、min/max 的直接元素及比較直接 operand
  都不接受 param；內含 plus/minus 的遞迴節點仍照自己的白名單判斷。
- plus 的兩個 operand 不得同時是 param；來源的加法偏移角色、minus 的 subtract_offset
  及實值 Int 仍須承載／接線驗證，不能由 schema 的 `{param}` 推定已授權。
  接線必須用完整 operand 路徑對照來源 canonical 構造，不能只看最近的 plus／minus；
  外層相減或縮放會影響偏移的實際方向與幅度。
- `count.who/zone` 是 counted_owner/counted_zone；一般 `Sel.who/zone` 是
  source_owner/source_zone。它們使用[不同位置型別](author-object-query-shapes-1.0.md)，
  不能以 `count: Sel` 的文法簡寫遺失來源角色；count 的葉參數另依承載白名單核對。

## 逐 variant 矩陣

下列 mapping 均封閉，所列鍵全部必填、沒有選填欄位；外層各列互斥，不能混填。
`E` 表 `A.expr.numeric`；`C` 表 `A.cond`。
數值 binding 的型別與可見性另驗，不把任意字串當可執行算式。
裸別名已有 define 與頭部 x.from_payment 的讀法；bound 已有 define 與 declare 的讀法。
define 因而有兩種已記載的拼法重疊，本表不選標準形，也不推定兩種 body 的 hash 相同。
兩者都只能在本核心解析為數值；並非任意物件 binding 都可讀成數字。

| 定義／變體 | 必填形狀 | 選填 | 互斥／值域 | 責任與依據 |
| --- | --- | --- | --- | --- |
| A.expr.numeric／常量 | Int | 無 | 排除 bool、字串整數；float 由嚴格入口拒絕 | §5 有號算式 |
| A.expr.numeric／別名 | BindingName | 無 | 與保留物件／玩家名不相交 | §5 define、GAP-A-001 的 X |
| A.expr.plus | `{plus:[E,E]}` 或一個 operand 為 A.param | 無 | 恰兩項；不得兩項皆 param | 承載 §3 加法偏移角色 |
| A.expr.minus | `{minus:[E,E或A.param]}` | 無 | 恰兩項；第一項不得直接 param | 承載 §3 subtract_offset |
| A.expr.times | `{times:[E,E]}` | 無 | 恰兩項；不接受直接 param | §5 |
| A.expr.half-up | `{half_up:E}` | 無 | 單鍵 | §5 |
| A.expr.per-payload | `{of:E,unit:正安全整數}` | 無 | unit 不為零、不為 param／算式 | §5 每 unit；不建立零除 |
| A.expr.per | `{per:A.expr.per-payload}` | 無 | 單鍵 | §5 |
| A.expr.min | `{min:[E,…]}` | 無 | 非空列表 | GAP-A-028 |
| A.expr.max | `{max:[E,…]}` | 無 | 非空列表 | GAP-A-028 |
| A.expr.bound | `{bound:BindingName}` | 無 | 不接受 ObjectRef mapping | GAP-A-028；binding 的值型別另驗 |
| A.expr.power-of | `{power_of:A.ref或A.snapshot-read}` | 無 | 不接受 Sel／領袖 recipient | §5；單一物件與 LKI 另驗 |
| A.expr.hp-of | `{hp_of:A.ref或A.snapshot-read}` | 無 | 同上 | §5 |
| A.expr.orig-cost-of | `{orig_cost_of:A.ref或A.snapshot-read}` | 無 | 不接受 Sel；單一材料與屬性可用性另驗 | BP13-053；捕捉內限 Ref，見事件與歷史矩陣 |
| A.expr.cost-of | `{cost_of:A.ref或A.snapshot-read}` | 無 | 同上 | GAP-A-028／055 |
| A.expr.resource-payload | `{who:A.player-ref,kind:pp_max或pp或ep或sep}` | 無 | who 不接受集合／param | GAP-A-005，回 Int |
| A.expr.resource | `{resource:A.expr.resource-payload}` | 無 | 單鍵 | GAP-A-005 |
| A.expr.turn-number-payload | `{who:A.player-ref}` | 無 | 不接受 param | GAP-A-056 |
| A.expr.turn-number | `{turn_number:A.expr.turn-number-payload}` | 無 | 單鍵 | 實際開始的玩家回合序數 |
| A.expr.zone-free-payload | `{who:A.player-ref,zone:field或ex}` | 無 | 不接受 param／zone 集合 | §14 欄位正規化，回 Nat |
| A.expr.zone-free | `{zone_free:A.expr.zone-free-payload}` | 無 | 不接受 Cond 的 ge／eq | §14 |
| A.expr.count | `{count:A.expr.count-source}` | 無 | 區域、in 或計數聯集；不是任意 ObjectSel | [來源矩陣](author-object-query-shapes-1.0.md) |
| A.cond.cmp-payload | `{left:E,op:eq或ne或lt或le或gt或ge,right:E}` | 無 | 不接受裸 param／物件集合 | §5；數值單位與同型性仍另驗 |
| A.cond.player-cmp-payload | `{left:A.expr.player,op:eq或ne,right:A.expr.player}` | 無 | PlayerExpr 為 `{player:PlayerRef}`；不與數值混用 | 合成：同型玩家比較 |
| A.cond.cmp | `{cmp:A.cond.cmp-payload或A.cond.player-cmp-payload}` | 無 | 兩分支互斥 | GAP-A-005／028／055／056；玩家分支合成 |
| A.cond.all | `{all:[C,…]}` | 無 | 非空列表 | §5 組合條件 |
| A.cond.any | `{any:[C,…]}` | 無 | 非空列表 | §5；不是選取數量的 any:true |
| A.cond.not | `{not:C}` | 無 | 單鍵 | §5 |

`A.expr.per-payload.unit` 的正整數限制、all/any 非空限制、half_up 的單運元標量形，
以及 plus 偏移 param 可在任一側，是本表明確定形的雙向細化。
all、any、min、max 均允許單元素列表；min/max 非空另有 GAP-A-028 的明文依據。
不從 IR 型別偷偷推出其他撰寫形狀。`A.cond` 收上表四種條件、下表的 did、非歷史條件矩陣及事件與歷史矩陣的 history／paid／chosen，
不收裸 bool，也不把讀值 `{zone_free:{who,zone}}` 視為條件。

## 快照與收據讀值矩陣

下列 mapping 封閉；材料 filter 與長形 did 的選填鍵依表。
`count_of` 加入同一個 E，`did` 加入同一個 C；算術與條件組合遞迴沿用它們。
快照讀值只進列明的單一物件屬性 operand，以及既有 Collection.in／same_as 位置；
不直接成為數值 E、A.ref、PlayerRef 或領袖 recipient，不提供操作新世代物件的授權。

| 定義 | 必填形狀 | 選填 | 互斥／值域 | 依據 |
| --- | --- | --- | --- | --- |
| A.expr.objects-of | `{objects_of:A.receipt-ref,view:before或after}` | 無 | view 必填，不接受 null；不得混入 follow、lki_of | §6、GAP-A-057 |
| A.snapshot-read | A.expr.objects-of | 無 | 快照讀值的共用入口，不是數值或可操作引用 | 同上 |
| A.same-as | `{same_as:A.snapshot-read}` | 無 | 不收裸 Ref、param 或任意 Sel | GAP-A-057 的名稱／種類對照 |
| A.material-filter | name、zone、kind、trait、not_class、orig_cost_eq、orig_cost_ge 至少一項 | 其餘屬性 | name/trait/not_class 為非空文字；zone 為 Zone、kind 為 Code；費用比較為 Quantity；多項 AND | GAP-A-035／054；BP03-045、ECP01-045、ECP01-036、BP08-103 |
| A.expr.count-of-filtered | `{receipt:A.receipt-ref,where:A.material-filter,view:before或after}` | 無 | where 不換來源，不讀 Live | §6、GAP-A-035 |
| A.expr.count-of | `{count_of:A.receipt-ref或A.expr.count-of-filtered}` | 無 | 短形讀指定單位 actual；長形數材料快照匹配成員 | §6、GAP-A-035／041 |
| A.cond.did-filtered | `{result:A.receipt-ref,where:A.material-filter}` | view:before/after | 付款／捨牌可省略為 before；其他材料 receipt 須明示 view | GAP-A-054；BP14-099、BP17-045 |
| A.cond.did | `{did:A.receipt-ref或A.cond.did-filtered}` | 無 | 純條件；不收 actual、evolved、payment_complete 等自由欄位 | §6、GAP-A-054／060 |

`objects_of` 取成功材料成員的指定 before／after 快照；「預設成功成員」不表示
可省略 view 或自動取得新區引用。`follow` 仍是另有規則授權的引用形狀。
`same_as` 提供材料快照的名稱／種類比較形狀；名稱依 CardNames 集合與規則比較，
不因後來物件移區而改讀目前特徵。單一／集合基數與可用屬性仍須接線驗證。

材料 filter 的欄位是材料快照屬性，不是來源 selector；不授予 Sel 的直接參數權限。
費用比較用 Quantity，可讀具名宣告數或內嵌算術，不能直接放 param。
省略欄位保持不存在，不補 null；空物件、未知鍵及來源切換鍵均拒絕。

### 降低時的 receipt 型別與成功責任

以下是 D0／lowering 的接線契約，**不是節點 Schema 或 Python 型別已驗證的執行行為**。
形狀只能確認 ReceiptRef；實際 receipt 類別、可見性、材料投影與操作授權取自宣告。

- 短形 did 採[撰寫語法 §6](author-syntax-1.0.md#6-玩家引用結果)的類別判準。
  「卡片移動」包含適用的捨牌、破壞、消滅等指示，不只鍵名 move。
  未登錄成功判準的 receipt 拒絕短形 did，不退回 actual>0 或 Status 的布林轉換。
- optional 的 result 在 Declined 時為 false；否則取本文最後一個指示的 typed did，
  沿[控制流程 §4](author-syntax-1.0.md#4-決定與控制流程)。
  合法 entry_payment 缺席時 did 為 false；拼錯或未宣告的 binding 是載入錯誤。
- 長形 did 是「receipt 的預設成功判準成立，且至少一個成功材料成員的指定快照符合 where」。
  付款／捨牌材料省略 view 時使用 before；其他具成功材料投影的 receipt 須明示 view。
  BP14-099／BP17-045 的移入公開區後判斷用 after；無材料投影的 receipt 拒絕。
  CP03-SL16 例因而須 PaymentComplete 且有 before 材料符合 name、zone；where 不取代付款成功。
- count_of 長形要求 receipt 有成功 ResultMember 材料快照，按明示 view 計算匹配張數。
  傷害、生命、資源等沒有材料快照的 receipt 不得使用長形，不能把原單位改成張數。
  短形仍保留 actual 的原單位，算術／比較還須驗單位與同型性。
- 所選快照無所需資訊時依 CR 1.3.6 處理，不以目前的物件或同 ID 新世代補值。

真實形狀分別由正文 BP08-SL14（GAP-A-035）、BP10-112（GAP-A-057）、
CP03-SL16（GAP-A-054）、BP07-SL20 母なる君（GAP-A-060）支持。
節點測試驗證這些片段可表達，不宣稱完整卡片或規則情境已執行。

## 集合與玩家讀值矩陣

新增 mapping 均封閉且列出的欄位必填；沒有預設 who、zone、attr 或 fn。
`S` 表 A.collection，僅區域物件、in 物件與它們的聯集，詳見來源矩陣。
所有卡例只支持對應片段，未宣稱完整卡片可執行；「合成」表示契約定形但沒有直接卡例。
卡例取自 `card-data/jp/` 全部凍結檔遞迴的 text／sections，不採 Q&A 命中。

| 定義／變體 | 必填形狀 | 互斥／值域 | 卡例或合成依據 |
| --- | --- | --- | --- |
| A.expr.hand | `{hand:PlayerRef}` | 不收 `{who:…}`、PlayerSet 或 param | BP01-007 白銀の矢、BP13-113；ECP02-020 的 master_of |
| A.expr.leader-life | `{leader_life:PlayerRef}` | 同上，回該玩家領袖生命 | 讀值形狀合成；ECP01-046 的領袖目標接線仍待定 |
| A.expr.counter-payload | `{on:Ref或LkiOf,name:CounterName}` | 不接受 Sel／param；CounterName 為非空文字 | BP13-038、BP21-060 |
| A.expr.counter | `{counter:A.expr.counter-payload}` | 普通 Ref 為 Live；明示 lki_of 讀已登錄 LKI | 同上 |
| A.expr.distinct-names | `{distinct_names:S}` | 依規則名稱等價類去重，非字串數量相加 | BP09-001 ユグドラシル、BP09-007 |
| A.expr.distinct-orig-costs | `{distinct_orig_costs:S}` | 計算不同原始費用的種類數 | BP10-011 喝采の獣使い、BP11-036 |
| A.expr.agg-payload | `{of:S,attr:cost或orig_cost或power或hp,fn:sum或min或max}` | 不收任意屬性／函數或 param；hp 三格合成 | 下列矩陣與 history shapes 的 hp 補充 |
| A.expr.agg | `{agg:A.expr.agg-payload}` | 空集合 sum=0；min/max 為 AbsentByRules | 同上；空值傳播由 lowering 驗證 |

| agg.attr | sum | min | max |
| --- | --- | --- | --- |
| cost | 合成：共用函數域 | BP04-070 ケートス | 合成：共用函數域 |
| orig_cost | ECP02-005 | 合成：共用函數域 | 合成：共用函數域 |
| power | CP04-022 | 合成：共用函數域 | BP12-034 |

CollectionSource 的 who／zone 不收直接 param；包括 agg.of、distinct 及 count.except。
這些位置沒有承載 §3 的 counted_owner／counted_zone 白名單。
但 where 內巢狀 count 是另一個 A.expr.count 節點，仍有自己的 counted 參數權限；
Quantity 內的合法算術偏移也不受外層來源角色影響。
名稱等價、單位、空集合與 AbsentByRules 傳播均是降低責任，schema 不執行聚合。

## 非歷史條件矩陣

所有外層單鍵 mapping 互斥，不接受裸 bool。Ref 的 scope、基數、世代及視角須另驗。
以下欄位均必填且封閉，未列參數位置不接受 param。

| 定義 | 必填形狀 | 值域／互斥 | 卡例或合成依據 |
| --- | --- | --- | --- |
| A.cond.exists | `{exists:ObjectSel}` | 不收任意 Ref／領袖 recipient | CSD01-010、BP04-034、BP21-060 |
| A.cond.on-field | `{on_field:Ref}` | 物件是否仍在場 | BP18-073 |
| A.cond.in-zone-payload | `{ref:Ref,zone:Zone}` | zone 只收單一區域 | BP07-058、BP10-043 |
| A.cond.in-zone | `{in_zone:A.cond.in-zone-payload}` | 不收來源 selector | 同上 |
| A.cond.standing | `{standing:Ref}` | 站立物件 | BP15-102 |
| A.cond.acted | `{acted:Ref}` | 不是 filter 的 bool | 合成：已登錄條件 |
| A.cond.evolved | `{evolved:Ref}` | 同上 | 合成：已登錄條件 |
| A.cond.my-turn | `{my_turn:true}` | 嚴格 true | CP03-003 |
| A.cond.opp-turn | `{opp_turn:true}` | 嚴格 true | 合成：已登錄條件 |
| A.cond.construction | `{construction:Title}` | Title 是非空作品標題；主詞為自己，不收 who／param | CP02-T01、CSD03a-016 |
| A.cond.from-payload | `{ref:Ref,from:Zone}` | 單一區域；不收 of／zone 別名 | 下兩列共用 |
| A.cond.entered-from | `{entered_from:A.cond.from-payload}` | 入場來源 | BP03-090、BP07-038；非手牌以 not 包裹，BP03-028 |
| A.cond.played-from | `{played_from:A.cond.from-payload}` | 打出來源 | BP10-062 |
| A.cond.entered-by-payload | `{ref:Ref,by:ability或play}` | 不收 effect 等別名 | BP17-SL25 |
| A.cond.entered-by | `{entered_by:A.cond.entered-by-payload}` | 僅兩種登錄來源 | 同上 |
| A.cond.is-payload | `{ref:Ref,kind:CardKind}` | 不收 param | BP01-SL16、BP01-126 |
| A.cond.is | `{is:A.cond.is-payload}` | 單鍵條件 | 同上 |
| A.cond.combo | `{combo:正安全整數}` | 不收零、bool、算式或 param | BP01-014 |
| A.cond.spellchain | `{spellchain:正安全整數}` | 同上 | BP01-065 |
| A.cond.necro | `{necro:正安全整數}` | 同上；非付款指示 | BP02-080 |
| A.cond.awakened | `{awakened:true}` | 嚴格 true | ETD01-018 |
| A.cond.crimson | `{crimson:true}` | 嚴格 true | BP01-112 |
| A.cond.player-eq-threshold | `{who:PlayerRef,eq:Quantity}` | 三種比較鍵恰一個 | 下表 |
| A.cond.player-le-threshold | `{who:PlayerRef,le:Quantity}` | 同上 | 下表 |
| A.cond.player-ge-threshold | `{who:PlayerRef,ge:Quantity}` | 同上 | 下表 |
| A.cond.player-threshold | 上三型之一 | who 必填，不收 PlayerSet／param | hand 與 leader_life 共用 |
| A.cond.hand | `{hand:A.cond.player-threshold}` | 與 Expr.hand 的標量 payload 不同 | 下表 |
| A.cond.leader-life | `{leader_life:A.cond.player-threshold}` | 與 Expr.leader_life 不同 | 下表 |
| A.cond.zone-free-eq-threshold | `{who:PlayerRef,zone:field或ex,eq:Quantity}` | 不收 ge／le 共存 | 合成：已登錄 eq |
| A.cond.zone-free-ge-threshold | `{who:PlayerRef,zone:field或ex,ge:Quantity}` | 不收 eq／le 共存 | BP13-043 |
| A.cond.zone-free-threshold | 上兩型之一 | 不收 le、其他區域或 zone 列表 | §14 |
| A.cond.zone-free | `{zone_free:A.cond.zone-free-threshold}` | 不把純讀值當條件 | 上兩列 |

| 條件 | eq | le | ge |
| --- | --- | --- | --- |
| hand | BP14-072 | BP03-020 | BP05-096 |
| leader_life | BP15-076 | BP10-109 | 合成：共用比較域 |

entered_from／played_from 的 ref 與 1.0 entered_by／is 的物件欄位一致，from 明示來源，
不沿用其他上下文的 of 或用 zone 同時表示目前位置與歷史來源。
Title 與 Concept 即使共用非空文字的 wire domain，也不是同一語義角色或參數入口。
比較門檻使用 Quantity，負值算式的規則求值不由 schema 截零。

## 片段邊界

A.expr.numeric 與 A.cond 的名稱表示單一遞迴入口，不表示完整 E/C 或全卡可寫。
history、事件窄化、LKI／捕捉、付款／骰值、Player 同型 cmp 與 orig_cost_of
已在[事件與歷史形狀](author-history-shapes-1.0.md)登錄，使用同一 E/C。
事件讀值沒有任意 payload 路徑，history.where 依事件拆型，接線責任仍另驗。
本表不定義完整 Sel、first／second／rest 或有序／角色視圖的接線驗證。
單組父節點與[角色選取位置](author-selection-shapes-1.0.md)沿各自矩陣，不代表完整動作已接入。

快照式 CountIn（A.collection.in）與 exists 可以使用 ObjectSel／Count F，
但這只是形狀合法，**不是 count_of／did 材料例的標準寫法，也不推定同 hash**。
材料的 kind／trait／not_class 與宣告費用比較已在上表登錄；BP19-094 的破壞數量
使用 count_of 短形。獨立材料 filter 及 view 規則不由一般快照查詢代替。

Schema 根繼續拒絕 standalone 文件；本表不提供完整 authored／expanded profile。
本圖可遞迴含合法葉參數，不能當成 expanded profile；Rust 解碼與執行由 D0 接續。
裸 BindingName 與未來字串關鍵字須維持不相交，變更保留字也會影響共用引用原子。
