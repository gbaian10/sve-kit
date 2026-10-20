# 作者 1.0：角色宣告與選取位置

本表細化[撰寫語法 §3／§5／§14](author-syntax-1.0.md#3-費用目標與打出程序)的角色與數量位置。
機器語法以 [author-nodes.schema.json](../../dsl/author-nodes.schema.json) 為準。
節點定義包含下列角色宣告、[領袖引用](author-reference-shapes-1.0.md#變體矩陣)
及獨立的[物件來源／計數片段](author-object-query-shapes-1.0.md)；
本表另定義具名 `RecipientSel`；單組 Target／choose／search 與 counter 的數量父變體
見[要求量與切片矩陣](author-quantity-shapes-1.0.md)，其餘父動作仍未完成。
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
多個 `remaining` 的扣除語義定案前，接線須拒絕；形狀驗證通過不授予這項語義。
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
| cost.counter 的 counter 數 | n／upto／any:true | 必填一種，見要求量矩陣 | 無 |
| cost.counter.holders | holder 數量 | on 為 Sel 時須明示；on:this 唯一，不另問 holder | 無 |
| A.act.create | n／upto／any:true | 可省略；預設每名 1 | 只有 n：Nat/count/value，不是 selection_count |
| A.choice | n／upto | 必填一種；upto 的實際選擇 1..N | 無 |

上述葉參數仍須按[承載契約 §3](author-contract-1.0.md#3-封閉葉參數)核對來源角色與 component。
`selection_count.constant` 不代表能用 param 取代 X、bound、expression 或整個數量結構。
cost.counter 的 counter 個數與 holder 個數是兩個不同位置。

一般 n／upto 與 selector 比較位置保留正文支持的 X、bound 及算式，不能只以 Nat 常量
或含 param 的常量子集代表完整值域。數值表達式的有號 Int 也不是所有位置的通用型別：
inline 常量的域、負要求量的規則處理，以及比較與選取量的不同責任，要由各位置分別驗證。
形狀驗證不把有號算式先截為零。
`count.who/zone` 的來源角色是 counted_owner/counted_zone；
`Sel.who/zone` 是 source_owner/source_zone，不因表面同型而合併。

## 領袖與聯集

`{leader:self|opp}` 共用 A.single-leader，`{leader_of:Ref}` 共用 A.leader-of；
A.leader-recipient 只各引用一次，另含 A.multiple-leaders。
`leader_of` 讀物件的 Live controller，離場依法讀 LKI，不是 owner。
既有 single-leader 不因此擴成任意 PlayerRef。

`RecipientSel` 是物件與領袖的具名 sum type；`ObjectSel` 始終只含物件。
下表 mapping 封閉，未列欄位拒絕；選填省略不補 null。

| 定義／Python | 必填 | 選填 | 互斥／值域 |
| --- | --- | --- | --- |
| A.sel.leader-or／LeaderOr | who:PlayerRef、kind:follower、leader_or:嚴格 true | zone:field | who 不收 PlayerSet／param；不收 F、except、where、as，zone 不收列表 |
| A.sel.recipient-atom／RecipientAtom | ObjectAtom／LeaderRecipient／LeaderOr | 依各型 | 不收頂層 any_of；三種領袖原子經 A.leader-recipient 各納入一次 |
| A.sel.recipient-union／RecipientUnion | any_of: 非空 RecipientAtom 列表，至少一個領袖類 limb | 無 | 純物件列表不屬本型；不收巢狀 any_of 或 wrapper 的 filter |
| A.sel.recipient／RecipientSel | ObjectSel／LeaderRecipient／LeaderOr／RecipientUnion | 依各型 | 各 shape 只命中一個葉分支 |

領袖類 limb 指 SingleLeader、MultipleLeaders、LeaderOf、LeaderOr。
RecipientUnion 在 any_of 陣列層以 `contains` 要求至少一個領袖類；
純物件列表交給 ObjectUnion，因此兩種 any_of 不會重複命中。
單 limb、重複 limb 均允許；limb 本身仍須通過完整封閉型別。
節點聯集採 anyOf，測試遞迴攤平 root 聯集，查重複葉分支並驗每個正例恰命中一支。

### 語義與作者選用規則

`leader_or` 表示單一 PlayerRef 的領袖與該玩家場上的從者；zone 省略也只表示 field。
BP01-125 鋭利な一裂き、BP02-062 竜巫女の儀式對應省略形，
BP05-057 ゴッドバレットゴーレム對應明示 field；數量由父 Target 等節點指定。
SingleLeader 的 self／opp 不因這個新型別擴為任意 PlayerRef。

單一玩家領袖加上同玩家、無 filter 的場上從者，作者寫 leader_or：
包含 BP03-093 ホワイトスワン・オデット的 Last Words 與 BP04-103 アンドロメダ。
領袖集合、跨玩家或從者 limb 帶 filter 時寫 RecipientUnion，從者 limb 明寫 zone:field，
不能把 ZoneObjects 的省略 zone 當成固定 field。

| 卡例 | 接受者來源 |
| --- | --- |
| BP02-052 インペリアルドラグーン、BP03-076 ブラックスワン・オディール、BP04-012 忌刃鬼 | 相手領袖全體及相手從者全體；保留 opponents 的集合基數 |
| BP03-093 ホワイトスワン・オデット的 Fanfare | 自己領袖及自己其他從者；從者 limb 的 other:true 不可加在 leader_or |
| BP21-037 的 token 段『痛苦』 | 跨玩家、含名稱 filter 的接受者聯集 |

例如相手全體的來源：

```yaml
any_of:
  - { leaders: opponents }
  - { who: opponents, zone: field, kind: follower }
```

any_of 按相同身分去重後供整個父節點使用，不是每個 limb 各執行一次。
領袖與物件的身分須保留種類標記，不能因字串 ID 相同合併。
聯集不從 limb 次序產生物件順序；世代與視角不相容時拒絕。
全體傷害仍是同一批次，不能改成每 limb 一個傷害動作。

Schema 仍接受與 leader_or 等義的 any_of，作者選用偏好尚未由 C0 lint 強制。
**不推定等義拼法有相同 body hash**：包括 leader_or 與聯集、
省略與明示 zone:field、limb 排列或重複，以及物件／計數聯集與
[集合形欄位](author-object-query-shapes-1.0.md#選取位置的物件聯集)的等義拼法。
canonical JSON 不重寫上述結構；執行成員去重不表示作者 bytes 去重。

### 消費位置與接線邊界

物件來源位置只能使用 ObjectSel，例如 move／search／choose 的物件來源與費用材料；
count 使用獨立 CountSource，in 使用 Ref／SnapshotRead。
接受領袖的 Target、damage.to、cannot.attack.target 等位置的選擇器分支才使用 RecipientSel，
不得再另加同形領袖支線。單組 Target.from 已使用 RecipientSel，
choose.from／search.sel 只使用 ObjectSel；其他父動作的接入仍是後續實作義務。
來源角色、binding 基數、Live／快照可用性、世代、去重與批次仍由接線與 lowering 驗證；
形狀驗證不授予操作能力，也不等於完整 Sel 或執行准入。
