# DSL 1.0：費用與目標形狀

本表細化[語法 §3](author-syntax-1.0.md#3-費用目標與打出程序)。
[author-nodes.schema.json](../../dsl/author-nodes.schema.json) 是機器語法權威；
Python 的 `author_costs`、`author_selection_parents` 提供對應的封閉節點型別。
節點檢查不提供完整能力頭、DSL reader、binding／來源准入或付款執行。

Q＝Quantity，T＝SelectionQuantity；[數量矩陣](author-quantity-shapes-1.0.md)的非負字面量、
安全整數、內層算式及參數位置規則不變。下表未列欄位一律拒絕；true 不接受 false、1。
所有 result 都是 BindingName，不能使用 this 等保留名稱。

## 材料與特殊 payload

| 機器定義／Python | 必填 | 可選 |
| --- | --- | --- |
| A.cost.material-group.exact／MaterialGroupExact | sel:ObjectSel、n:Q | 無 |
| A.cost.material-group.up-to／MaterialGroupUpTo | sel:ObjectSel、upto:Q | 無 |
| A.cost.material-group.any／MaterialGroupAny | sel:ObjectSel、any:true | 無 |
| A.cost.material.exact／MaterialExact | sel:ObjectSel、n:Q | 材料共通欄 |
| A.cost.material.up-to／MaterialUpTo | sel:ObjectSel、upto:Q | 材料共通欄 |
| A.cost.material.any／MaterialAny | sel:ObjectSel、any:true | 材料共通欄 |
| A.cost.material.groups／MaterialGroups | groups:MaterialGroup[]，至少兩項 | 材料共通欄 |
| A.cost.discard-random／RandomDiscardMaterial | sel:ObjectSel、n:Q、random:true | result |
| A.cost.discard-ref／DiscardRef | ref:Ref | result |
| A.cost.banish-top／BanishTop | top:DeckSlice | result |
| A.cost.shuffle-material.exact／ShuffleMaterialExact | MaterialExact 必填欄、shuffle:true | 材料共通欄 |
| A.cost.shuffle-material.up-to／ShuffleMaterialUpTo | MaterialUpTo 必填欄、shuffle:true | 材料共通欄 |
| A.cost.shuffle-material.any／ShuffleMaterialAny | MaterialAny 必填欄、shuffle:true | 材料共通欄 |
| A.cost.shuffle-material.groups／ShuffleMaterialGroups | MaterialGroups 必填欄、shuffle:true | 材料共通欄 |
| A.cost.skip-turn／SkipTurn | who:PlayerRef | 無 |

材料共通欄恰為 result、distinct_by:DistinctBy、cost_sum_ge:Q。
DistinctBy 只收 orig_cost、class、name。cost_sum_ge 是整組材料的**總原費用下限**，
不是每組各自下限；不接受材料 cost_sum_le。groups 與 sel／數量互斥，各材料組各自有 sel
及恰一種數量；不能附 result、整體限制或巢狀 groups。材料組同時選取、跨組不重用，
不是按陣列逐組貪婪；實際滿足性由付款層驗證。

A.cost.material-group 合併三支；A.cost.material、A.cost.shuffle-material 各合併四支。
RandomDiscardMaterial 只收精確數量，排除 upto、any、groups、distinct_by、cost_sum_ge、
ref、include_self；普通 Material 不收 random（包括 false）。random 的母集合、選擇者與資訊
可見性另由 C1／D0 驗。to_deck 必填 shuffle:true，指移動後洗整副牌庫。

捨手牌自身的標準形是 discard:{ref:this}；自身加其他材料用 groups，
其中自身組為 `{sel:{in:this},n:1}`。普通材料也接受該單組形狀，但不推定與 ref 形有相同 hash。
include_self 與 stack_counter_from 僅有舊名稱登錄、沒有接受形狀；不據此發明 payload。
土の秘術的護符選擇是付款決定，不是獨立 stack_counter_from 操作。

## Cost 結構與操作矩陣

| 定義／Python | 結構 | metadata |
| --- | --- | --- |
| A.cost.step／CostStep | 下表恰一操作 | 無 |
| A.cost／Cost | 單一操作，或 steps:CostStep[]（至少兩項） | 無 |
| A.cost.alternative／CostAlternative | 單一操作（排除直接 one_of），或 steps:CostStep[]（至少兩項） | 無 |
| A.cost.additional／AdditionalCost | Cost 結構 | result? |
| A.cost.optional／OptionalCost | Cost 結構 | result?、option?（正安全整數） |
| A.cost.optional-mode／OptionalModeCost | Cost 結構 | option 必填正安全整數、result? |
| A.cost.optionals／OptionalCosts | 單一 OptionalCost，或 OptionalModeCost[]（至少兩項） | 各元素獨立 |

短形恰一操作；空 Cost 不能表示免費，可明寫 pp:0。長形只可有 steps，不能混操作；
每步仍恰一操作，不收 steps／result／option 等 step metadata。
one_of 至少兩個 CostAlternative，只選一支；禁止直接 one_of 套 one_of，
分支內可用 steps，steps 內仍可有 one_of。這不表示整張卡的模式 choice。

| 操作 | payload | 真實卡例或合成 |
| --- | --- | --- |
| pp | Q | BP03-001；BP07-067；多步 BP01-016 |
| ep | Q | 合成；EP 代付規則不當卡例 |
| leader_life | Q | BP01-105、BP05-072 |
| lesson | Q | CP02-003 |
| earth_rite | Q | BP01-051；可選 BP10-050 |
| pp_max | Q | BP06-058／BP06-P16 |
| act_self | true | BP01-001 |
| self_to_cemetery | true | BP07-067；多步 BP01-016 |
| banish_self | true | BP08-041，auto 本文 pay 的費用 |
| discard | Material、RandomDiscardMaterial 或 DiscardRef | exact BP03-045；upto CP02-056；random BP03-088；ref BP08-037；groups SD08-017／BP20-SL14；any 合成 |
| banish | Material 或 BanishTop | exact BP14-023；groups BP08-037；top BP03-073／BP13-015；distinct_by BP10-003；upto／any 合成 |
| to_cemetery | Material | exact BP01-SL15；groups BP01-166／BP20-104；upto／any 合成 |
| return_to_hand | Material | exact BP01-002；any CP01-003；upto／groups 合成 |
| reveal | Material | exact BP04-022；upto／any／groups 合成 |
| flip_down | Material | exact CP03-029；其餘模式合成 |
| flip_up | Material | exact BP08-053；必要追加 BP17-116；其餘模式合成 |
| act | Material | exact CP04-104；any BP07-030／BP11-P30（cost_sum_ge）；upto／groups 合成 |
| to_ex | Material | exact BP08-072（sel.in:this）；其餘模式合成 |
| to_deck_bottom | Material | exact BP14-008／BP07-012（sel.in:this）；其餘模式合成 |
| to_deck | ShuffleMaterial | exact BP05-067；其餘模式合成 |
| counter | CounterCost | self exact BP01-062；self any BP03-001；selected exact BP14-063；兩種 upto／selected any 合成 |
| skip_turn | SkipTurn | self BP05-086／BP20-R13；其他玩家合成 |
| one_of | CostAlternative[]，至少兩支 | BP01-SL15；複合分支／steps 內位置合成 |
| stack_counter_from | 無接受形狀 | 舊登錄，不作合成正例 |

正式 lowering／runtime 的起動能力費用支援 `leader_life: N`（固定非負整數），
依 CR 10.4.5 只扣自己的領袖生命；生命不足時整筆付款失敗，不能降至負數。
可以剛好付至 0，敗北留到能力結算後的檢查時點。這是生命變動而非傷害，
付款收據使用 `QuantityUnit::Life`，並保留實際生命變動事實；BP08-084 同時驗證每回合兩次上限。

CounterCost 六個數量分支及 holders 的必要性沿[要求量矩陣](author-quantity-shapes-1.0.md#單組父變體矩陣)，
已併入 author_costs 模組。self 的 on:this 不收 holders；selected 的 on:ObjectSel 必有 holders:Q。
所有 counter 數量不接受直接 selection_count 參數，不能借用 Target 的 T。
CR 10.4.3 規定費用未明指玩家的「場の」等所屬域是要求該費用的卡／能力之 master；
Q806 確認不能以對手使用的卡付費。BP14-063／BP14-055 的「場の『竜山温泉』」
因而限自己的場，不能移除對手的神湯 counter（Q1910／Q1914 補充自身無 counter 時不能付）。
此為規則限制；語法能表達 who:each 不表示該卡能用這個域付款。

上述只列費用片段；不能因片段接受就放入任意能力頭。MaterialGroup 的 upto／any、
各材料操作未點名的約束交叉格均為合成。cost_sum_ge 只有 act.any 的 BP11-P30
有直接卡例，其他操作／數量的該約束不能據此宣稱實卡覆蓋或解除規則准入限制。

正式起動費用也支援一組固定正整數公開材料：`to_cemetery`、`return_to_hand`、
`banish`、`act`，沿用 `{sel: {who: self, zone: field | ex, kind?, trait?, name?, other?}, n: N}`。
`act` 限場上未橫置物件。先選完效果目標，再一次選定完整材料，之後依費用順序付款；
材料選擇不受效果目標權限限制。材料不足或任一後續費用失敗，整次起動還原。
BP03-118 使用橫置自身加送墓一名墮天使從者的費用；移動依材料擁有者決定目的區。
公開材料路徑目前不收多組材料、自由數量、總費用限制、隱藏材料或帶此費用的 Quick；
觸發能力的材料選擇支付需沿選擇支付宣告入口另行支援。

正式起動能力費用支援一組固定正整數手牌棄牌：
`discard: {sel: {who: self, zone: hand, kind?, trait?, name?}, n: N}`。
選完模式與效果目標後，先以只有付款者可見的暫時 handle 選定完整材料，再依原費用順序付款。
棄牌費用必須足量；不沿用效果棄牌「不足時盡量棄」的行為。任何後續費用失敗都還原整次起動，
包括 PP、橫置、棄牌、已公開的資訊與事件。棄牌仍保留既有棄牌事實與計數。
SD03-008／SD03-009 使用橫置自身加棄一張手牌法術的費用。
目前不含 Quick、多組、隨機或非棄牌的隱藏材料付款。

## wrapper、X 與打出費用欄

| 定義／Python | 必填／可選 | 真例與界線 |
| --- | --- | --- |
| OptionalCost | Cost＋result?＋option? | 短 CP03-029；長 BP10-019／BP10-SL05／BP10-U02（選項 4：pp:4 後 self_to_cemetery） |
| OptionalCosts 列表 | 每項 Cost＋option＋result? | BP16-081、BP19-090、BP21-090；各組獨立決定、獨立完整付款 receipt |
| AdditionalCost | Cost＋result? | BP17-116、BP15-046；必要追加費用 |
| A.cost.alt／AltCost | if:Condition、cost:Cost；may?:bool | CP03-083，may:true；may 省略／false 合成 |
| A.x.free／FreeX | min?:Q、max?:Q，空 mapping 合法 | CP04-104／CP04-115；bounds 一般組合合成 |
| A.x.payment／PaymentX | from_payment:BindingName | BP03-001；已完成單一材料／counter actual |
| A.x／XParameter | FreeX 或 PaymentX | 兩者互斥 |
| A.play.costs／PlayCosts | 全部選填，見下表 | 只驗費用欄片段，不是完整 play 頭 |

| PlayCosts 欄 | 型別 | 形狀相依 |
| --- | --- | --- |
| cost_delta | NumericExpr／{if:Condition,delta:NumericExpr}，或至少兩項的上述列表 | 增減可為負；false 不安裝修飾 |
| cost_set | Q／{if:Condition,set:Q}，或至少兩項的上述列表 | 與 delta 可共存，先 set 再 delta |
| additional_cost | AdditionalCost | 必要追加 |
| optional_cost | OptionalCosts | 語義仍須符合 enclosing play／choice |
| optional_cost_delta | 同 cost_delta | 必有單一 mapping optional_cost，不可配列表 |
| optional_cost_set | 同 cost_set | 同上；不是 alt_cost |
| alt_cost | AltCost | if 必填，替代整份原費用 |

optional_cost_delta 的真例為 CP03-029；optional_cost_set 為 EBD02-007。
「コストを+Nしてよい」標準形是 optional_cost:{pp:N,result:…}，BP04-066 以整次付款結果分支。
與固定費用／下限的規則合成不能由形狀推斷。條件式打出修正（BP02-038／BP03-097／BP04-023）與保序修飾列表（BP13-093）沿相同 PlayCosts 型別；多個 set 的適用關係由 C1／D0 驗證。

OptionalCosts 的陣列依印刷選項序保存，不排序／去重。
C1 須驗 option 嚴格遞增、互異、存在於所屬 choice；這不是節點 schema 的跨元素保證。
play 頭沒有 choice，帶 option 或列表的 play 費用是 C1 必須拒絕的上下文。
多組可選付款禁止裸 paid:optional，使用具名完整付款 receipt 定位；不能將多組改寫為
steps（一次整體決定）或 one_of（只付一支）。

## Target 變體與承載

| 定義／Python | 必填 | 可選 |
| --- | --- | --- |
| A.target.exact／TargetExact | bind、from:RecipientSel、n:T | Target 共通欄 |
| A.target.up-to／TargetUpTo | bind、from:RecipientSel、upto:T | Target 共通欄 |
| A.target.any／TargetAny | bind、from:RecipientSel、any:true | Target 共通欄 |
| A.target.grouped.exact／TargetGroupedExact | bind、from:ObjectSel、groups:TargetGroupFilter[]（至少兩項）、n:T | Target 共通欄 |
| A.target.grouped.up-to／TargetGroupedUpTo | 同上，upto:T 取代 n | Target 共通欄 |
| A.target.grouped.any／TargetGroupedAny | 同上，any:true 取代 n | Target 共通欄 |
| A.target／Target | 以上六支 | 無其他形狀 |
| A.targets／Targets | Target[]，至少一項；無目標時省略欄位 | 保留原始陣列順序 |
| A.target.group-filter／TargetGroupFilter | 至少一個屬性 filter | 專用封閉屬性集合 |

Target 共通欄恰為 roles:TargetRoles、option:正安全整數、cost_sum_le:Q、
distinct_by:DistinctBy、distinct_names:true；後兩者互斥，名稱互異標準形為 distinct_by:name。
cost_sum_le 是整體**總原費用上限**；與材料下限分開。

TargetGroupFilter 重用 [CountFilter 屬性值域](author-object-query-shapes-1.0.md)，
排除直接 ParamRef、power_max、where/as、who/zone/in/except；不放來源、數量、bind、result。
name／names／name_contains 互斥，token／not_token 互斥。內層算式的參數位置依自身規則，
不是遞迴禁用所有 param。from 給共同母集合，各 filter 窄化其差異，每組容量一個且跨組不重用，
外層數量約束全體；不選最大者，也不按組貪婪。grouped 不接受領袖來源。

| 位置／變體 | 真例或合成 | 責任 |
| --- | --- | --- |
| 單組 exact／up-to／any | BP01-125／BP21-PR13／BP01-001 | 適用能力頭的 targets；包含授予後的完整能力 |
| grouped up-to | BP19-080、CP03-042（三組）、ECP02-029 | 同一 targets 元素；grouped exact／any 合成 |
| roles | BP21-PR13 | Target 同層，mapping 鍵序不排序目標 |
| option | BP17-112 選項 1 | 模式存在性與作用域由 C1 |
| cost_sum_le | BP07-071 | 整體總原費用上限 |
| distinct_by:name | exact BP08-104；up-to BP03-040 | distinct_names 舊拼法、Target distinct_by:orig_cost／class 為合成 |

CP03-042 的三組是分組語義的證據；其消滅領域來源尚無DSL Zone 代碼，
依承載契約的封閉映射，不把 banished／banish 當可用 DSL 來源，不能宣稱整個 Target 已可寫。

各能力頭是否可帶 targets 仍依語法 §3 與規則登錄；完整能力頭與 delayed 的適用欄位見[能力矩陣](author-ability-shapes-1.0.md)。
本文 choose 是結算時決定，不能代替打出時 Target。roles、option、總量約束的其他交叉格
未列真例者皆合成，不因此授予任意父節點同名欄位。

## receipt、時點與跨費用欄位

payload.result 屬單一材料／counter 費用指示；wrapper.result 是整次付款的 PaymentComplete。
短形 pp 不能附 payload result，也不是 x_paid 的材料數。付款前不能讀 paid／x_paid；
from_payment 只作已完成單一材料／counter actual 的別名，不額外詢問自由 X。

CR 10.6.2 先處理必要自由 X（.2.1）、可選付款決定（.2.2）、模式（.2.3）、
其餘 X（.2.4），再選目標（.3）與決定／支付費用（.5，替代選用為 .5.1.2）。
付款決定值與完成 receipt 不同：BP21-026／PR-585 在模式數之前讀的是決定，
不能用 paid 假裝；這個決定讀值的完整能力頭拼法不在本節點契約。
BP17-056 的 X 與前置模式數也不能直接用 from_payment 解決。

Cost.steps 依卡文順序支付、保留重複（CR 10.4.2.1）；targets 依卡文原序。
CR 10.6.2.5 要求先決定完整費用，再全部支付；任何一項付不起便不能付整份
（10.4.2.2），並回復 play 前（10.6.2.5.4）。BP15-046 文字先列公開手牌、後列 X PP，
依 10.4.2.1 以此順序執行（Q2006 另確認 X 於打出時指定）。印刷費用與文字追加費用的相對
次序未另有明文，不由 YAML 欄位順序推斷；上述先定全費用及文字順序不是待查證事項。
沒有跨欄位支配證明時，
C1／D0 不得讓前一欄任意讀另一欄的 receipt。one_of 分支內 result 為分支 scope，
只有全部分支同型輸出才能合流；靜態檢查不猜玩家會選哪一支。
未宣告、拼錯、前向、自參照、跨 execution 的 receipt 都須拒絕。

這些 scope／型別／時點、材料 disjoint、random 資訊、counter holders、負數與不足量、
完整支付／回滾由 C1／D0 驗證。土の秘術護符選擇、+N 與 set 合成等規則問題
不能因 shape 合法就擅自解除准入限制，也不在這裡更改公開 IR。

## 證據與機械驗收

卡例來自 [card-data/jp](../../card-data/jp) 全部凍結卡檔，遞迴涵蓋 302 檔、7383 歷史 faces
及其 text／sections，未將 Q&A 當卡例。上述是節點片段，非整卡 DSL；未找到直接真例的格
明列合成，不能把文字搜尋零命中當不存在的證明。官方卡文權利依 [LICENSING](../../LICENSING.md)。

所有新增具名定義均對照 Python 型別漂移，並突變 required、額外欄位、操作互斥、相依、
最小長度、數值下限與 group filter 約束。代表正例／錯上下文／錯型別／互斥反例
同時驗 schema 及解碼；Target 數量聯集驗單一分支命中。

以真正 Cost／Targets 節點解碼再 canonical：payload mapping 換鍵序 bytes 相同，
steps／targets 對調 bytes 不同，重複步驟不能去重，單元素 steps 形狀拒絕。
不改 body recipe、不在 hash 前語義正規化，也不聲稱完整DSL reader 已整合此驗收。

## 正式選擇支付窄片

Spell 與入場（Fanfare）觸發能力的能力頭可寫單一固定 PP 費用，例如
`optional_cost: {pp: 2, result: extra}`，以 `paid: {receipt: extra}` 控制付款成功才適用的本文。
此宣告先詢問是否支付，再依原順序決定目標，最後與原費用在同一交易內完整付款；
付不起取消整次打出，拒付則得到未完成的付款結果。即使拒付或付款後改為集合效果，
卡文原本要求的目標仍須選擇。BP04-066 驗證追加 5 PP 與單體／集合消滅；
Fanfare 使用相同宣告入口，在其待機項被選中後詢問，不改變一般入場與本文 choose 的時點。

IR 使用根本文開頭單一 `K.pay(at=PlayParameter)`，空的 on_paid 與本地付款結果；
runtime 在付款前剝離宣告，付款完成後才開始執行本文。此窄片不含多組、選項費用、替代費用、
材料或手牌付款，也不接受巢狀宣告、起動能力宣告或與卡片 exchange 並存。
