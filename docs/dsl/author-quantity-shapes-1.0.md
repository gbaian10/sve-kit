# DSL 1.0：要求量、切片與單組選取

本表細化[語法 §3–7](author-syntax-1.0.md#3-費用目標與打出程序)的數量位置，
機器語法以 [author-nodes.schema.json](../../dsl/author-nodes.schema.json) 為準。
Target、choose、search 的**單組數量父變體**包含來源與必要的 binding／目的地；
Target.groups 與其餘 Cost 另見[費用與目標矩陣](author-cost-target-shapes-1.0.md)。
不是可放進任意父節點的數量 mapping。這些型別仍不提供完整DSL 入口或執行准入。

## 共用數量與參數

| 定義／Python | 值域 | 限制 |
| --- | --- | --- |
| A.expr.numeric-operation／NumericOperation | [NumericExpr](author-expression-shapes-1.0.md) 除 Int 的全部分支，含 BindingName | 算術、別名與讀值只列一次；不代表僅有算術 |
| A.expr.numeric／NumericExpr | Int 或 NumericOperation | 保留有號常量與原有遞迴 operand |
| A.quantity／Quantity | Nat 或 NumericOperation | inline 常量非負；不是 Nat 或 Int 的重疊聯集 |
| A.selection-quantity／SelectionQuantity | Quantity 或 ParamRef | 直接 param 只供 Target／choose／search 的 n、upto |
| A.deck-slice／DeckSlice | `{who:PlayerRef或PlayerSet,n:Quantity}` | 兩鍵必填、封閉；who 與 n 均拒絕直接 param |
| CounterName | 非空文字 | counter 種類的名稱；登錄存在性另驗，不借 Concept 參數權限 |
| DistinctBy | orig_cost、class、name | 封閉屬性列舉，不接受 param |

Quantity 共用於 DeckSlice.n、counter 的 n／upto／holders、cost_sum_le、
已登錄的物件比較 filter 與 hand／leader_life／zone_free 條件門檻。
它限制的是**直接字面量**，不將算式結果截零。負算式與來源不足的規則處理不由 shape 決定。
bool、數字字串與超過安全整數界的常量拒絕；float 由嚴格入口及 Python 解碼拒絕。

SelectionQuantity 的直接參數須為 Nat／selection_count／constant；
n 匹配來源 exact、upto 匹配 up_to。來源是 X、bound 或 expression 時，
不能藉 `{param}` 把整個結構偽裝成常量。這是[承載 §3](author-contract-1.0.md#3-封閉葉參數)
的接線義務，節點型別不驗來源 frame。
search.to 的直接參數仍是 Zone／destination_zone／value，不是 selection_count。

各位置內嵌 plus／minus 時，既有 arithmetic_offset 的合法 operand 仍可使用 param，
例如 `n:{half_up:{minus:[X,{param:offset}]}}`。times 的直接 operand、
minus 第一項與兩項都是 param 的 plus 仍拒絕。
接線須比對完整 operand 路徑、來源方向與幅度，不能只看最近的運算子。

## 單組父變體矩陣

以下 mapping 全部封閉；未列欄位拒絕，選填省略不補 null。
Q 表 Quantity，T 表 SelectionQuantity；各 exact／up-to／any 是獨立分支。

| 定義 | 必填形狀 | 選填共用欄 |
| --- | --- | --- |
| A.target.exact | bind、from:RecipientSel、n:T | Target 欄 |
| A.target.up-to | bind、from:RecipientSel、upto:T | Target 欄 |
| A.target.any | bind、from:RecipientSel、any:嚴格 true | Target 欄 |
| A.act.choose.exact | bind、from:ObjectSel、n:T | choose 欄 |
| A.act.choose.up-to | bind、from:ObjectSel、upto:T | choose 欄 |
| A.act.choose.any | bind、from:ObjectSel、any:嚴格 true | choose 欄 |
| A.act.search.exact | sel:ObjectSel、to:SearchDestination、n:T | search 欄 |
| A.act.search.up-to | sel:ObjectSel、to:SearchDestination、upto:T | search 欄 |
| A.cost.counter.self.exact | on:this、name:CounterName、n:Q | result |
| A.cost.counter.self.up-to | on:this、name:CounterName、upto:Q | result |
| A.cost.counter.self.any | on:this、name:CounterName、any:嚴格 true | result |
| A.cost.counter.selected.exact | on:ObjectSel、name:CounterName、holders:Q、n:Q | result |
| A.cost.counter.selected.up-to | on:ObjectSel、name:CounterName、holders:Q、upto:Q | result |
| A.cost.counter.selected.any | on:ObjectSel、name:CounterName、holders:Q、any:嚴格 true | result |

A.target、A.act.choose、A.act.search 各合併自己的分支；
A.cost.counter.self／selected 各合併三支，再由 A.cost.counter 合併兩族。
bind、result 均為 BindingName；result 是收據宣告，不是物件 binding。
search 沒有 any；counter 沒有省略數量形。
on:this 的 holder 唯一且禁止 holders；on:ObjectSel 必填 holders。
holder 個數與每個 holder 的 counter 要求量是不同位置，兩者都不收直接 selection_count 參數。

| 共用選填欄 | 型別與局部限制 |
| --- | --- |
| Target | roles:TargetRoles、option:正安全整數、cost_sum_le:Q、distinct_by:DistinctBy、distinct_names:嚴格 true |
| choose | by:PlayerRef、may:bool、reveal:bool、order:bool、position:top_or_bottom、groups、cost_sum_le:Q、distinct_by:DistinctBy |
| search | result、precondition、cost_sum_le:Q、distinct_by:DistinctBy、entry_effects、instead；不收 bind |

Target 的 distinct_by 與 distinct_names 互斥。名稱互異的DSL 標準形為 distinct_by:name；
既有 distinct_names:true 保留，但兩者不推定相同 body hash。
cost_sum_le 是總原費用的上限位置，直接常量是 Nat，不收直接 param；
合法算式不在 shape 層求值或截零。
may 拒絕整項與 upto／any 選零不同；false 與省略仍保留不同 DSL bytes。

choose 的本片來源為 ObjectSel。結算時接受者選取須另有明確規則依據，
不能把打出時 Target 改成 choose；本表不宣告永遠排除領袖。
choice.upto 的 1..N 與 create.n 的 Nat/count/value 參數維持原規則，
不使用本表的 selection_count 位置取代它們。

### 卡例

以下均取自 `card-data/jp/` 遞迴凍結卡檔的 text／sections，涵蓋歷史 documents 與 faces；
不以 Q&A 命中作為卡例。只驗所列節點，不宣稱整張卡已能執行。

| 變體 | 卡例與來源 |
| --- | --- |
| Target exact | BP01-125：領袖或從者一個接受者 |
| Target up_to、roles | BP21-PR13：至多三體，primary 為非空一項、rest 為餘者 |
| Target any | BP01-001：EX 的妖精從者任意枚數 |
| choose exact | BP19-110：看過八張後，從其中選一張加入手牌 |
| choose up_to | BP03-108：看過的童話卡至多兩張，可拒絕整項 |
| choose any | BP04-SL08：手牌從者任意枚數，可拒絕整項 |
| search exact | BP14-055：牌庫中的竜山温泉一張 |
| search up_to、cost_sum_le | BP18-P10：至多兩張透京從者，總原費用至多三 |
| counter self exact | BP01-062：移除自身三個スペル counter |
| counter self exact 的數值別名 | CP04-115 section:0：自身 X 個バッテリー counter |
| counter self any | BP03-001：移除自身任意數量童話 counter |
| counter selected exact | BP14-063：場上的竜山温泉一個 holder、兩個神湯 counter |

counter up_to 與 selected any 是語法已列的數量變體，目前凍結卡文未找到直接例；
holders 算式也沒有卡例，僅做契約與合成邊界驗證，不能宣稱實卡覆蓋。
BP14-063 原文「場の」在來源片段以 who:each 保留；實際付款的控制者限制由規則准入判定。

## 牌庫切片的消費位置

DeckSlice 是保序牌庫片段，不額外選身份，也不自行授權看隱藏資訊。
who 為 PlayerSet 時，n 表每位玩家各 n 張；不從玩家列舉建立跨玩家的卡片順序，
也不把 opponents 縮成 opp。各玩家分別執行自身指示的 each_player 仍是另一構造。

| 已登錄位置 | 標準形與例 |
| --- | --- |
| move.top | 動作同層 top:DeckSlice；ETD01-005、BP06-028 |
| banish.top | 同上；BP05-020 的上半副牌庫 |
| look.top | 同上；BP06-108 |
| reveal.top | 同上；CP03-105 |
| cost.banish.top | 同上；GAP-A-010，已接入 Cost 的 BanishTop |

DeckSlice 已接入 cost.banish 及 move/banish/look/reveal 父動作。
後續父節點不可默收 `ref:{top:…}` 或 `ref:{bottom:…}` 別名。
ObjectSel、RecipientSel、Ref 目前都拒絕 top／bottom；不新增一套排除切片的 selector。
不接受獨立 Sel.top/bottom 或動作 bottom 來源。
BP06-108 的「その中から」應讀 `in:looked`，不能重新查當下牌庫頂端。
bottom 來源沒有凍結卡例；`position:bottom` 是目的地，不能冒充其證據。

BP05-020 的 DeckSlice.n 可表達為：

```yaml
half_up:
  count: { who: self, zone: deck }
```

依 [X.math](ir-vocabulary-1.0.md) 與 BP04-116 的非負卡例，
half_up 是除以二後向上取整；本表不以原型的飽和運算決定負數或溢位規則。
ECP01-038 的 DeckSlice 為 `{who:opponents,n:20}`；
跨玩家批次、收據與事件責任由 lowering 驗證，shape 不決定批次數。

## 接線與完整父節點的邊界

以下是完整 reader／lowering 的可測契約，**不是本片已新增的 scope 驗證 API**：

- Target 為打出時選取、choose 為結算時選取；binding 保留元素種類與集合基數。
  n=1 也不把 binding 自動降為一般 Ref 字串。
- RoleRef.of 必須指同能力已宣告、帶 roles 的 Target，name 必須存在。
  choose 或其他 binding 不因同名而取得 Target 角色。
- if_nonempty 角色在空集為零項、非空為一項，保留所選物件或領袖種類；
  remaining 是集合。多個 remaining 在扣除語義定案前，接線必須拒絕。
- 只在宣告之後的本文使用；不重名、不向前引用，不跨分支或 execution 洩漏。
  角色 map 的順序不建立集合順序，不能任取 first／second。
- 參數來源 role、component、mode 與完整算式 operand 路徑仍須核對；
  shape 合法不表示來源、資訊可見性或操作能力已准入。

完整 choose/search 群組、位置、search.to.partition、entry_effects、instead、precondition 及 move/banish/look/reveal 已由[動作矩陣](author-step-shapes-1.0.md)承載。
本表保留數量角色邊界；完整能力入口、來源／型別／scope／ports／receipt/capability 檢查仍另行實作。
