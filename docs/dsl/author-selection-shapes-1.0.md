# 作者 1.0：角色宣告與選取位置

本表細化[撰寫語法 §3／§5／§14](author-syntax-1.0.md#3-費用目標與打出程序)的角色與數量位置。
機器語法以 [author-nodes.schema.json](../../dsl/author-nodes.schema.json) 為準。
目前只有下列角色宣告及[領袖引用](author-reference-shapes-1.0.md#變體矩陣)有節點定義；
本表的數量位置規則尚未由完整 Target／Sel／動作 schema 強制。
這些片段不提供作者文件入口、作用域驗證或執行准入。

## 角色宣告矩陣

| 定義 | 必填形狀 | 選填 | 互斥／值域 | 依據 |
| --- | --- | --- | --- | --- |
| A.target.role-one | `{if_nonempty:1}` | 無 | 封閉 mapping；真整數 1，不接受 bool、字串或 param | §14 A-016h |
| A.target.role-value | A.target.role-one 或字串 `remaining` | 無 | 不接受其他字串或物件 | 同上 |
| A.target.roles | 非空 `Map<RoleName,A.target.role-value>` | 無 | 每個鍵完整符合 RoleName；不限定 primary／rest | §3、§14 A-016h |

`roles` 只屬於 Target。非空選取時，`if_nonempty:1` 的角色指定一項；選零時該角色也零。
`remaining` 表示餘者。沒有角色時可省略父節點的 roles，不以空 map 表示。
RoleName 與 BindingName 是不同命名空間；roles map 的鍵序沒有語義。
例如 `{primary:{if_nonempty:1},rest:remaining}` 宣告兩個角色，
以 `{role:{of:t,name:primary}}` 與 `{role:{of:t,name:rest}}` 引用。
不能因 primary 排在第一個鍵，就把它當成任意集合的第一項。

Schema 只檢查這些形狀；接線還須確認 `role.of` 指向宣告角色的 Target binding，
`role.name` 確實存在，且使用位置接受該角色的單一物件或集合基數。
多個 `remaining` 能否共存及其扣除語義不由本片段授權，也不因形狀驗證通過而自動成立。
`first/second/rest` 的順序引用尚未定編碼；A.ref 仍不接受 `{first:t}` 或 `"first:t"` 等候選拼法。
有角色的集合不等於有順序，無序集合不能任取第一項。

Pydantic 的 `Literal[1]` 即使 strict 也可能接受 bool 或浮點；Python 型別在字面值檢查前，
使用共用的嚴格整數邊界拒絕這些值。JSON Schema 的 integer 本身不區分 `1` 與 `1.0`，
浮點仍由嚴格 JSON／YAML 入口拒絕。

## 數量位置規則

下表區分同層的數量鍵，不是可獨立插進父節點的 count mapping。
`n` 是精確數量；普通 `upto` 是 0..N，只有 choice 的實際選擇為 1..N；
`any:true` 是 0..候選上限，與 Cond 的 `any:[…]` 不同。
同一列的數量模式互斥；`may` 的拒絕整項與選零仍分開。

| 位置 | 可用數量模式 | 省略規則 | 葉參數白名單 |
| --- | --- | --- | --- |
| A.target | n／upto／any:true | 必填一種 | n／upto：Nat、selection_count.constant，分 exact／up_to |
| A.act.choose | n／upto／any:true | 必填一種 | 同上 |
| A.act.search 單組 | n／upto | 必填一種；groups 是另一形狀 | 同上；不接受 any |
| 費用材料 | n／upto／any:true | 由各成本變體定義，不從共用鍵推導 | 無 |
| cost.counter 的 counter 數 | n／upto／any:true | 由 Cost 變體定義 | 無 |
| cost.counter.holders | holder 數量 | on 為 Sel 時須明示；on:this 唯一，不另問 holder | 無 |
| A.act.create | n／upto／any:true | 可省略；預設每名 1 | 只有 n：Nat/count/value，不是 selection_count |
| A.choice | n／upto | 必填一種；upto 的實際選擇 1..N | 無 |

上述葉參數仍須按[承載契約 §3](author-contract-1.0.md#3-封閉葉參數)核對來源角色與 component。
`selection_count.constant` 不代表能用 param 取代 X、bound、expression 或整個數量結構。
cost.counter 的 counter 個數與 holder 個數是兩個不同位置。

一般 n／upto 與 selector 比較位置保留正文支持的 X、bound 及算式，不能只以 Nat 常量
或含 param 的常量子集代表完整值域。數值核心的有號 Int 也不是所有位置的通用型別：
inline 常量的域、負要求量的規則處理，以及比較與選取量的不同責任，要由各位置分別驗證。
形狀驗證不把有號算式先截為零。
`count.who/zone` 的來源角色是 counted_owner/counted_zone；
`Sel.who/zone` 是 source_owner/source_zone，不因表面同型而合併。

## 領袖與聯集

`{leader:self|opp}` 共用 A.single-leader，`{leader_of:Ref}` 共用 A.leader-of；
A.leader-recipient 只各引用一次，另含 A.multiple-leaders。
`leader_of` 讀物件的 Live controller，離場依法讀 LKI，不是 owner。
既有 single-leader 不因此擴成任意 PlayerRef。

節點聯集採 anyOf；這些領袖葉分支的接受集合仍互不重疊。
漂移與正反例檢查會遞迴攤平聯集，查重複葉分支並驗證每個正例恰命中一個葉分支，
不只依賴 anyOf／oneOf 的結構正規化。
完整 Sel 與動作位置也必須避免把相同領袖形狀透過多條分支重複納入；
本片段的驗證不宣稱那些組合位置已實作。
