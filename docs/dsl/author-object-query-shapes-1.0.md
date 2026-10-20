# 作者 1.0：物件來源與計數形狀

本表細化[撰寫語法 §5](author-syntax-1.0.md#5-表達式捕捉與選擇器)的物件來源與計數，
機器語法以 [author-nodes.schema.json](../../dsl/author-nodes.schema.json) 為準。
`ObjectSel` 表 A.sel.object-core，只含區域物件與 binding／快照物件；
它不接受領袖 recipient，未涵蓋完整 Sel，也不提供完整作者 reader 或執行准入。

## 逐 variant 矩陣

所有 mapping 封閉，unknown key 拒絕；選填欄位省略時保持不存在，不接受 null。
`F` 是下節白名單，`C` 是同一 A.cond.numeric-core（含 did）。

| 定義 | 必填 | 選填 | 互斥／值域 |
| --- | --- | --- | --- |
| A.sel.zone-objects | who: PlayerRef／PlayerSet／ParamRef | zone: Zone／非空 Zone 列表／ParamRef、F、except、where、as | 不接受 in、any_of、領袖或切片 |
| A.sel.in-objects | in: Ref／SnapshotRead | F、except、where、as | 不接受 who、zone、any_of、領袖或切片 |
| A.sel.object-core | 上兩型的聯集 | 依各型 | 不是 Ref、數值或 Cond |
| A.expr.count-objects | who: PlayerRef／PlayerSet／ParamRef、zone: Zone／非空 Zone 列表／ParamRef | F | 不接受 in、except、where、as |
| A.expr.count-union | any_of: 非空 CountObjects 列表 | 無 | limb 只收 A.expr.count-objects，不接受 ObjectSel 或巢狀聯集 |
| A.expr.count-source | CountObjects／CountUnion | 依各型 | 保留各 limb 的 counted 角色 |
| A.expr.count | count: A.expr.count-source | 無 | 加入同一 A.expr.numeric-core |

who 必填。不限定持有者的場上物件以 `who:each` 表達（BP06-112；
另有 BP01-095 的官方 Q&A），不靠省略 who 推定雙方。
count.who 的雙方範圍有 CP01-004；PlayerSet 保留集合基數，不因引擎只有二席改為 PlayerRef。
Sel.zone 可省略（GAP-A-048 BP20-030），不在形狀層補 field；
count.zone 必填（GAP-A-001／025／040）。區域列表不接受元素 param。

`except` 是單一 ObjectSel，排除 binding 寫 `{in:looked,except:{in:chosen}}`。
差集保留左側已有順序，按相同物件身分比較；世代及快照視角須相容，
不能將不相容的 Live／快照物件自行配對。
BP07-SL09、BP14-023 的剩餘材料由這個差集表達，之後的「好きな順」仍須獨立 order；
差集本身不讓玩家選擇排列。這不新增 Sel.rest 或 first/second/rest 引用編碼，
也不把 Target.roles.remaining 當成差集別名。

`in` 單一 Ref 正規化為 singleton；字串是否物件 binding 由接線檢查。
receipt 必先使用帶 view 的 objects_of。快照式 in 只提供讀值，不取得 move 等操作權限。
same_as 也仍只接快照，不解成 Ref／follow。

count 聯集的案例為 BP06-035：自己墓場的陰陽師卡與法術之聯集。
先依物件身分去重再數張數，一張符合兩個 limb 的牌只算一次，不能降低成兩個 count 相加。
此例只建立計數位置的聯集形狀，不表示選取位置的 any_of 已定形。
區域列表與多玩家來源也依集合身分去重；計數查詢不選取、不產生有序 binding。

`where` 只在本片接受 C。提供 `as:BindingName` 必須同時有 where，
該層不再引入 item；省略 as 則綁定 item。禁止巢狀隱式遮蔽、
候選元素型別及別名有效範圍仍由 D0／lowering 檢查。
GAP-A-028 的 cost_of/item 與 bound 比較可表達；GAP-A-029 的 history 尚不在 C 片段。

## 物件屬性與參數

F 共用基本葉值域與欄位互斥；來源與計數有不同父型別，不把 count 解碼成 ObjectSel。
MaterialFilter 保留自己的三欄常量域，不隨 F 取得參數權限。
下表的 ParamRef **只適用於 ObjectSel**；CountObjects 的 kind、class、trait、
traits_all 元素及 name 全部排除 ParamRef，其餘值域不變。
計數的 counted_kind 與篩選的 filter_kind 是不同來源角色，前者尚無作者參數白名單位置。

| 欄位 | 值域 | 依據／限制 |
| --- | --- | --- |
| kind | CardKind／ParamRef／SameAs | GAP-A-057；有效種類由接線檢查 |
| class | Concept／ParamRef | A-016d、A-018；class_filter/value |
| trait | Concept／ParamRef | trait/value |
| traits_all | 非空 Concept／ParamRef 元素列表 | A-042b；每元素 trait/value，不接受整個列表 param |
| name | CardName／ParamRef／SameAs | name_filter/value；SameAs 只快照 |
| names | 非空 CardName 列表 | BP09-105 的搜尋材料；不接受 param |
| name_contains | 非空文字 | GAP-A-055 補充；不接受 param |
| other | 嚴格 true | GAP-A-025／045；排除 this |
| orig_cost_eq／orig_cost_le／orig_cost_ge | Nat 常量 | GAP-A-035／018／044／045／048；不接受 param 或算式 |
| hp_le | Nat 常量 | GAP-A-032；不接受 param 或算式 |
| acted／racing | 嚴格 bool | A-016d、A-036；false 與省略不同 |
| token／not_token | 嚴格 true | GAP-A-059、BP19-038 的融合素材；兩鍵互斥 |

name、names、name_contains 三鍵互斥；same_as 只能出現在 name 或 kind 的值。
其餘已列屬性同時存在表示全部成立。
上表比較鍵是常量片段，不宣稱其他 suffix、狀態或可運算比較都已交付，
也不將 Nat 常量片段當完整數量 Expr。

所有 ParamRef 須依[承載契約 §3](author-contract-1.0.md#3-封閉葉參數)核對。
Sel.who／zone 分別是 source_owner／source_zone；
CountObjects.who／zone 分別是 counted_owner／counted_zone。
兩者的 Player 參數只能 self／opp，Zone 參數只能單一 Zone，不能偷換整個集合。
葉值合法不代表來源角色合法。
CountObjects 只有 who／zone 接受 ParamRef，不因 `count: Sel` 的文法簡寫
就取得 Sel 屬性的參數權限；計數聯集各 limb 也遵守相同限制。

Python TypedDict 的嚴格度、未知鍵、互斥及 as→where 均有對應 Schema 約束與反例。
這些結構不是深層不可變物件，消費者不得假設可 hash。
去重、順序、binding／參數角色、scope 與操作能力都是接線義務，不是本片段已執行的行為。
