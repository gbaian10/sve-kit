# 作者 1.0：物件來源與計數形狀

本表細化[撰寫語法 §5](author-syntax-1.0.md#5-表達式捕捉與選擇器)的物件來源與計數，
機器語法以 [author-nodes.schema.json](../../dsl/author-nodes.schema.json) 為準。
`ObjectSel` 表 A.sel.object-core，含區域物件、binding／快照物件與這些來源的物件聯集；
它不接受領袖 recipient，未涵蓋完整 Sel，也不提供完整作者 reader 或執行准入。
已定形的單組選取父節點與要求量見[數量矩陣](author-quantity-shapes-1.0.md)。
DeckSlice 只定動作同層 top 的共用 payload；ObjectSel 不因而接受 top／bottom。
不接受 Sel.top/bottom；頂部來源使用動作同層 top:DeckSlice，底部只有目的地 position:bottom。

## 逐 variant 矩陣

所有 mapping 封閉，unknown key 拒絕；選填欄位省略時保持不存在，不接受 null。
`F` 是下節白名單，`C` 是同一 A.cond（含 did 與非歷史條件）。

| 定義 | 必填 | 選填 | 互斥／值域 |
| --- | --- | --- | --- |
| A.sel.zone-objects | who: PlayerRef／PlayerSet／ParamRef | zone: Zone／非空 Zone 列表／ParamRef、F、except、where、as | 不接受 in、any_of、領袖或切片 |
| A.sel.in-objects | in: Ref／SnapshotRead | F、except、where、as | 不接受 who、zone、any_of、領袖或切片 |
| A.sel.object-atom | 上兩型的聯集 | 依各型 | 不收頂層 any_of 或領袖 |
| A.sel.object-union | any_of: 非空 ObjectAtom 列表 | 無 | 不收巢狀 any_of、領袖或 wrapper 的 F／except／where／as |
| A.sel.object-core | ObjectAtom／ObjectUnion | 依各型 | 不是 Ref、數值或 Cond；except 可遞迴接 ObjectUnion |
| A.expr.count-objects | who: PlayerRef／PlayerSet／ParamRef、zone: Zone／非空 Zone 列表／ParamRef | Count F、except、where、as | 不接受 in；except 接 CollectionSource |
| A.expr.count-atom | CountObjects／CollectionIn | 依各型 | in 來源不帶 who／zone |
| A.expr.count-union | any_of: 非空 CountAtom 列表 | 無 | 不收 wrapper 的 F／except／where／as 或巢狀聯集 |
| A.expr.count-source | CountAtom／CountUnion | 依各型 | 保留各 limb 的 counted 角色 |
| A.expr.count | count: A.expr.count-source | 無 | 加入同一 A.expr.numeric |
| A.collection.objects | who: PlayerRef／PlayerSet、zone: Zone／非空 Zone 列表 | Count F、except、where、as | who／zone 不收直接 param；except 接 CollectionSource |
| A.collection.in | in: Ref／SnapshotRead | Count F、except、where、as | 不收 who／zone；except 接 CollectionSource |
| A.collection.atom | CollectionObjects／CollectionIn | 依各型 | 無頂層 any_of |
| A.collection.union | any_of: 非空 CollectionAtom 列表 | 無 | 不收 wrapper filters 或巢狀聯集 |
| A.collection | CollectionAtom／CollectionUnion | 依各型 | agg、distinct 與計數 except 共用，保持來源角色 |

區域來源 who 必填；in 來源不收 who。不限定持有者的場上物件以 `who:each` 表達（BP06-112；
另有 BP01-095 的官方 Q&A），不靠省略 who 推定雙方。
count.who 的雙方範圍有 CP01-004；PlayerSet 保留集合基數，不因引擎只有二席改為 PlayerRef。
Sel.zone 可省略（GAP-A-048 BP20-030），不在形狀層補 field；
count.zone 必填（GAP-A-001／025／040）。區域列表不接受元素 param。

ObjectSel 的 `except` 是單一 ObjectSel。一般扣除已選集合可寫 `{in:looked,except:{in:chosen}}`；移動後「残り」須扣除成功 before：`{in:looked,except:{in:{objects_of:moved,view:before}}}`，不能扣除容量不足而未移動的 chosen。
差集保留左側已有順序，按相同物件身分比較；世代及快照視角須相容，
不能將不相容的 Live／快照物件自行配對。
BP07-SL09、BP14-023 的剩餘材料用成功 before 差集，之後的「好きな順」使用 move.order:chooser；
差集本身不讓玩家選擇排列。這不新增 Sel.rest 或 first/second/rest 引用編碼，
也不把 Target.roles.remaining 當成差集別名。

`in` 單一 Ref 正規化為 singleton；字串是否物件 binding 由接線檢查。
receipt 必先使用帶 view 的 objects_of。快照式 in 只提供讀值，不取得 move 等操作權限。
same_as 也仍只接快照，不解成 Ref／follow。

count 聯集的案例為 BP06-035：自己墓場的陰陽師卡與法術之聯集。
先依物件身分去重再數張數，一張符合兩個 limb 的牌只算一次，不能降低成兩個 count 相加。
此例只支持計數位置的聯集；選取位置另以以下卡例定形。
區域列表與多玩家來源也依集合身分去重；計數查詢不選取、不產生有序 binding。

### 選取位置的物件聯集

`ObjectUnion.any_of` 的每個 limb 是區域或 in 來源，各自保留 source 角色與 F。
父節點的選取數量作用於整個聯集，不是每個 limb 各選一次。
先按相同物件身分去重；世代或快照視角不相容時拒絕。
limb 的排列不建立物件順序，也不授予 first／second；本片不接巢狀 any_of。
原有 atom 的 except 可遞迴接受物件聯集，差集仍保留左側已有順序。

| 卡例 | 選取位置與 limb |
| --- | --- |
| BP07-050 1,2,マジック！ | 被選卡的玩家手牌：`{who:{master_of:t},zone:hand,kind:follower}` 或同來源的 kind:amulet |
| BP07-099 ブラックビショップ | 看過的牌：`{in:looked,kind:follower,trait:童話,orig_cost_le:5}` 或同來源的 kind:amulet |
| CP01-080 紡がれてゆく想い | 公開的牌：`{in:shown,kind:follower,trait:ウマ娘}` 或同來源的 kind:amulet |

例如 BP07-099 的候選來源：

```yaml
any_of:
  - { in: looked, kind: follower, trait: 童話, orig_cost_le: 5 }
  - { in: looked, kind: amulet, trait: 童話, orig_cost_le: 5 }
```

以上卡例取自 `card-data/jp/` 遞迴各子目錄的凍結卡文 text／sections，不以 Q&A 作卡例；
來源片段與單組數量另見數量矩陣；父動作及遞迴 Steps 見[動作矩陣](author-step-shapes-1.0.md)。
BP07-050 的 t 須有合法前序目標宣告，其 controller 捕捉／離場 LKI 沿既有規則另驗。
CountSource 保持獨立位置型別，新增的 in／except／where／as 依本表明確定形；
這四個計數欄位目前沒有直接卡例，只有契約與合成測試，不冒稱 corpus 已覆蓋。
CollectionSource 是 agg、distinct 與 count.except 的來源；區域 who／zone 不收 param。
CountIn 與 CollectionIn 共用 A.collection.in，except 一律接 CollectionSource，不能繼承 counted 參數權限。
相同 wire 在兩個位置各自合法時，來源角色仍不同。

作者在 limb 只一個欄位不同、且該欄已有等義集合形時，優先寫 zone 列表、names 或 PlayerSet；
kind 不同或多欄不同時寫 any_of。PlayerSet 必須保留原文基數，不以二席假設任意拆解。
物件聯集與這些集合形、計數聯集與其等義集合形，都**不推定相同 body hash**；
traits_any 也適用此原則。
canonical JSON 保留 limb 次序、重複及欄位有無；不執行排序、代數化簡或語義去重。
執行時集合成員去重與作者 body bytes 的等值是不同責任。
接受領袖的聯集另見[接受者矩陣](author-selection-shapes-1.0.md#領袖與聯集)，
不能從 ObjectSel、except 或 CountSource 進入。

### 來源 predicate 與局部別名

`where` 只在本片接受 C。提供 `as:BindingName` 必須同時有 where，
該層不再引入 item；省略 as 則綁定 item。禁止巢狀隱式遮蔽、
候選元素型別及別名有效範圍仍由 D0／lowering 檢查。
GAP-A-028 的 cost_of/item 與 bound 比較可表達；GAP-A-029 的 history 尚不在 C 片段。

## 物件屬性與參數

F 共用基本葉值域與欄位互斥；來源與計數有不同父型別，不把 count 解碼成 ObjectSel。
MaterialFilter 保留自己的三欄常量域，不隨 F 取得參數權限。
下表的 ParamRef **只適用於 ObjectSel**；Count F 的 kind、class、trait、
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
| not_class | Concept | BP04-034 ポルックス；不收 param |
| not_trait | Concept | ECP02-072 笑顔のカーテンコール；不收 param |
| traits_any | 非空 Concept 列表 | BP02-008、BP13-051、BP21-008；count BP09-117／BP21-001；不收 param |
| not_same_name_as_this | 嚴格 true | BP16-096、BP04-098、BP02-039 |
| cost_eq／cost_le／cost_ge | Quantity | 依序 BP01-039／BP01-053／BP01-100；cost_le 算式 BP03-112 |
| orig_cost_eq／orig_cost_le／orig_cost_ge | Quantity | 依序 BP05-042／BP02-115／BP08-009；le 算式 BP07-091、BP09-001、BP13-055、BP21-093 |
| power_max | 嚴格 true，只在 ObjectSel | BP09-077 ビッグソウルハンター、ECP02-066；不進 Count F／材料 |
| keyword | Concept | BP03-025、BP01-149；count BP02-105／BP12-093；不收 param |
| has_trigger | 嚴格 true | CP03-123 ラック・バード；依 TriggerIcon metadata，不等於 fanfare／lastword |
| has_evolve | 嚴格 true | BP01-035、BP03-021 |
| face_up | 嚴格 bool | true：BP06-117，count BP06-018；false：BP08-053、BP17-116 的付款來源 |
| boxed | 嚴格 bool | true：BP11-024；false 合成 |
| evolved | 嚴格 bool | false：BP02-041、ECP01-054；true 合成 |
| entered_this_turn | 嚴格 true | BP02-101 スカイスピリット |
| advance_follower | 嚴格 true | 契約登錄、合成；沒有直接布林欄位卡例，作者優先 kind:advance_follower |
| hp_le | Quantity | BP01-066、BP02-060；算式域由共用要求量定形，無直接算式卡例 |
| acted／racing | 嚴格 bool | A-016d、A-036；false 與省略不同 |
| token／not_token | 嚴格 true | GAP-A-059、BP19-038 的融合素材；兩鍵互斥 |

name、names、name_contains 三鍵互斥；same_as 只能出現在 name 或 kind 的值。
其餘已列屬性同時存在表示全部成立。
七個比較鍵共用 Quantity：Nat 字面量或 NumericOperation，不接受負字面量、bool、float、直接 param。
內層 plus／minus 偏移依自己的白名單，計算結果可為負，不在形狀層截零。
「より低い」可用 le 搭配減 1，不新增 cost_lt 等 suffix。
class 與 not_class、trait 與 not_trait 等同時出現只表示 AND，不因可能空集合而拒絕。

power_max 在同 atom 的其他 F、except 與 where 都套用後，保留有效攻擊力最大值的**全部並列者**；
空集合仍空，不要求作者選一張。聯集每個 limb 分別取極值，limb 排列不建立順序。
這個順序是契約定形，沒有直接卡例同時要求 except／where／power_max；降低驗證不能省略。
BP10-063 デュアルレイジ 是 kind:advance_follower 的例，不能充作布林欄位的直接卡例；
兩種拼法不推定同 hash。has_trigger 在來源缺 TriggerIcon 時也不宣稱已可執行。
keyword 的 OR 以 any_of 表達（例如 BP03-092），不新增 keywords_any。

所有 ParamRef 須依[承載契約 §3](author-contract-1.0.md#3-封閉葉參數)核對。
Sel.who／zone 分別是 source_owner／source_zone；
CountObjects.who／zone 分別是 counted_owner／counted_zone。
兩者的 Player 參數只能 self／opp，Zone 參數只能單一 Zone，不能偷換整個集合。
葉值合法不代表來源角色合法。
CountObjects 只有 who／zone 接受 ParamRef，不因 `count: Sel` 的文法簡寫
就取得 Sel 屬性的參數權限；計數聯集各 CountObjects limb 也遵守相同限制。
count.except 的區域來源沒有 counted 角色，故不接受參數；ObjectSel.except 保持 source 角色。
where 內若另有 count 節點，該 count 的 who／zone 仍接受自己的合法參數。

Python TypedDict 的嚴格度、未知鍵、互斥及 as→where 均有對應 Schema 約束與反例。
這些結構不是深層不可變物件，消費者不得假設可 hash。
去重、順序、binding／參數角色、scope 與操作能力都是接線義務，不是本片段已執行的行為。

快照 CountIn／exists 的廣泛 F 只代表形狀合法，不代表材料案例已解決；
材料 filter 的待定卡例及與 count_of／did 的非等價宣稱見[讀值邊界](author-expression-shapes-1.0.md#片段邊界)。
領袖接受者屬性 filter（BP05-002）仍未定形；本表不宣稱完整 Sel 或全卡可寫。
