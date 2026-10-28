# 事件、歷史與捕捉節點形狀（1.0）

本表補完已登錄的 E/C 讀值形狀。機器語法唯一權威是
[`author-nodes.schema.json`](../../dsl/author-nodes.schema.json)；Python 對應在
`contracts/author_queries.py`，與 NumericExpr、Quantity、Condition、ObjectSel、RecipientSel
共用同一遞迴圖。不存在第二個歷史專用表達式核心，也不新增泛型 A.expr 消費入口。
A.define 仍只收數值；LKI、材料快照與 PlayerExpr 只能進各自列出的 operand。

Schema 根拒絕 standalone 文件。節點檢查不提供完整 authored／expanded 驗證，
不驗 binding 宣告、來源證據、enclosing on、事件世代、規則准入或 capability。
以下接線責任由 C1／D0 驗證，不能把合法形狀當成已可執行卡片。

逐事件必填、選填、enum、history 頂層與 where 的對照見[事件欄位矩陣](author-event-matrix-1.0.md)。

引擎目前接通的回合出牌計數為 `history_count: {event: played, window: this_turn}`，
可用 `who: self|opp` 限定打出者，省略時合計雙方；`distinct` 只支援 `facts`。
這條接線不含主體篩選、排除執行、其他歷史視窗或能力起動計數。
計數只在適正出牌完成後增加，換回合歸零；失敗出牌與能力起動不增加。
`combo: N` 依 CR13.2，在檢查本卡正式出牌時包含正在打出的本張，
該暫時投影不寫入歷史；完成後與其他結算讀值都只計同一張一次。

## 事件角色與欄位

`A.event-subject-filter` 是非空屬性 filter：
kind、class、not_class、trait、not_trait、traits_any、traits_all、name、names、name_contains、
not_same_name_as_this、cost_eq/le/ge、orig_cost_eq/le/ge、hp_le、keyword、has_trigger、has_evolve、
face_up、boxed、evolved、entered_this_turn、advance_follower、other、acted、racing、token、not_token，
另有 where:Condition、as:BindingName。as 必有 where；name/names/name_contains 互斥，token/not_token 互斥。
比較屬性只收 Nat，非空文字／旗標／列表沿計數 filter 的值域，但 kind/name 不收 SameAs。
不收 who、zone、in、except、any_of、power_max 或直接 param。

`A.event-object-sel` 加上選填 who:PlayerRef、zone:Zone 或非空 Zone 列表；仍至少一鍵，
不授予一般 Sel 的來源／參數權限。`A.event-recipient` 為 Ref、LeaderRecipient、EventObjectSel、
LeaderOr 或 any_of 聯集；聯集原子不含 Ref，至少有一個領袖 limb，不收巢狀聯集。

where 在事件匹配時求值，item 是事件主體快照，只承載來源文字的事件限定。
「〜とき、…なら」的「なら」條件仍放本文 if（R-0009），不能移進 on 以改變誘發時間。
where 內獨立 count／exists／算術節點保留原有的參數位置白名單；禁止的是事件角色上的直接參數。
沒有事件主體、錯誤快照或不符來源的 where 由接線拒絕。

下表「主」是 ref:Ref 與 sel:EventSubjectFilter 二擇一，可都省略。
所有 payload 封閉；除 phase 外欄位皆選填，空 payload 匹配任意該事件。
`during={turn:PlayerRef}`；`by={ability_of:EventObjectSel}`；兩者不收直接 param。
省略 who 表任意玩家，who 只收單一 PlayerRef，不收 PlayerSet。

| 事件 | on payload 欄位 | who 角色 | 卡例／合成 |
| --- | --- | --- | --- |
| enter_field | 主、who、during、entered_by:ability/play、from:Zone、by | 入場 controller | BP01-033、BP10-004；entered_by/from/by 合成 |
| left_field | 主、who、during、by | 離場前 controller | BP01-016、BP04-108；by 合成；不收 to |
| placed_ex | 主、who、during、from:Zone、by | EX 持有者 | CP04-007、BP14-027；from/by 合成 |
| discarded | 主、who、from:hand、by、during | 捨牌者 | BP01-170、BP09-006、BP21-043 |
| banished | 主、who、from:Zone、during、by | 來源區持有者 | BP14-074；during/by 合成 |
| destroyed | 主、who、during、by | 破壞前 controller | BP01-106、BP03-034；by 合成 |
| damaged | recipient、kind:attack/combat/ability、during、by | 無 | CP02-SL10、BP21-037、BP21-109；by 合成 |
| dealt_damage | 主、who、zone:Zone、recipient、kind:attack/combat/ability、during、by | 傷害來源 controller | BP04-003、CP03-064、ECP02-057；by 合成 |
| attacked | 主、who、during | 攻擊者 controller | CP03-019、BP12-008；during 合成 |
| attack_target | 主、who | 目標 controller | 合成 |
| evolved | 主、who、by | 進化者 controller | BP02-016、BP20-004；history BP16-018；by 合成 |
| super_evolved | 主、who、by | 進化者 controller | auto／history 均為合成 |
| phase | which:Phase 或 ParamRef、whose:PlayerRef，皆必填 | whose 為階段所屬玩家 | BP05-006、BP01-153 |
| played | who、sel、during、from:Zone | 打出者 | CP02-044、BP14-036、BP04-064；from 合成 |
| ability_played | who、holder:Ref 或 EventObjectSel | 能力 controller | 合成 |
| life_changed | who、direction:up/down、during、by | 生命變動玩家 | BP02-095、BP19-075；by 合成 |
| stat_changed | ref、stat:power/hp 或非空列表、direction:up/down | 無 | BP11-082、BP21-100；不收 subject/property/by |
| counter_changed | ref、name:CounterName、after:{eq/le/ge:Nat} 恰一、direction:up/down | 無 | 合成；不把 Stack 提醒文另造誘發 |
| acted | 主、who | 姿態改變時 controller | CP04-102、BP11-069 |
| stood | 主、who | 姿態改變時 controller | 合成 |
| raced | 主、who | 從者 controller | CP01-082、CP01-032 |
| drive_gained | 主、who | 從者 controller | auto 形狀合成 |
| ub_activated | who、holder:Ref 或 EventObjectSel | UB controller | CP04-SL15 |
| fused | who、material:EventSubjectFilter | 融合者 | BP20-SL05、BP20-029；至少一成功材料符合為推論 |
| targeted | ref、by | 無 | BP03-071；by 合成 |
| state | Condition | 無 | BP05-092；不是歷史事實 |
| dice_rolled | who | roller | BP21-SL19；on 不收骰值門檻 |
| drive_checked | holder:Ref 或 EventObjectSel | 無 | CP03-P27、CP03-065 |
| drive_trigger | who | 執行 trigger 的玩家 | CP03-P81、CP03-110 |
| moved | 主、from:Zone 或 {who:PlayerRef,zone:Zone}、to:Zone、during、by | from.who 為來源區持有者 | BP05-029、BP01-012、BP20-025；by 合成 |
| drew | who、not_phase:Phase、by | 抽牌者 | BP05-094；by 合成 |
| equipped | equipment:Ref 或 EventObjectSel | 無 | CP04-PR22；selector 合成 |
| fused_as_material | ref、by | 無 | BP19-048 |

每個直接事件都有 `A.event.<事件以連字號分隔>` 及 `-payload` 定義。
另有 `counter_reached:{ref,name,value:Nat}`，三鍵必填，降為 counter_changed.after.eq；
`any_of` 必須是非空直接事件原子或 counter_reached 列表，不含 state，不巢狀。
`A.event-atom` 不含 state；`A.event` 另收單一 state。這與 IR 的事件／狀態觸發分工一致。

Damaged 的主體只用 recipient；dealt_damage 的主體是來源。
只有 dealt_damage 登錄 zone，表示來源在事件快照中的區域，不能查同卡目前的新世代。
played 的種類／類型等屬性統一放 sel，不收 payload 頂層 trait/kind。

## 歷史窄化

`history` 是 Condition，`history_count` 與 `history_sum` 是 NumericOperation。
三者的 payload 均要求 event、window。window 為 this_turn、game 或 `{since:BindingName}`；
後兩者標合成，since 錨點型別由接線確認，不接受字串 `since:name`。

| 定義族 | 選填 | 限制 |
| --- | --- | --- |
| A.history-query | distinct:facts/subjects、exclude:this_execution | 32 個非 state 直接事件加 payment；不收 at_least、at |
| A.history-condition-query | 同上，另 at_least:Quantity、at:event | at 在 history payload 內；省略 at_least 表 ≥1 |
| A.history-sum-query | distinct:facts、exclude:this_execution | field 必填；不收 subjects、at_least 或 payload at |

每事件僅把上表**有登錄的** who/zone/from/to/sel 提升至歷史 payload 頂層；其餘鍵進 where。
where 若提供必非空，不接受被提升的鍵。ref（where 內）與 sel（頂層）仍互斥。
phase.where.which/whose 必填，which 只收 Phase；on 的 phase 參數權限不傳入歷史。
每個變體有 `A.history.<事件>`、`-condition`，有 where 時另有 `-where` 定義。

dice_rolled.where 額外收 `value:{eq/le/ge:1..6}` 恰一（BP21-081、PR-573）。
payment 只在歷史：who 選填，where 必填且恰為 `{cost_kind:earth_rite}`，不收 phase。
state、any_of、counter_reached 不作 history.event。歷史保存事件當時特徵，不查目前卡片。

| history_sum 變體 | field | 必要窄化 | 卡例 |
| --- | --- | --- | --- |
| damaged、dealt_damage | amount | 對應事件 | 合成 |
| life_changed | delta | where.direction 必填 up/down | SP01-039 砂浜の調べ・リテュエル；up 增量總和，不是淨變動 |
| payment | removed_stack | where.cost_kind=earth_rite | BP14-037、BP21-040 |

生命 delta 保留符號；down 不自動 abs。removed_stack 是唯一拼法，不收 removed 別名。
其餘 stat/counter payload 不因數值型態就能 sum。

| 排除／截止 | 例 | 行為 |
| --- | --- | --- |
| exclude:this_execution | CP04-003、CP04-P60 | 排除本次 UB 的 AbilityExecutionId |
| sel:{other:true} | BP01-004 | 排除本卡的 played 事實；該事實與本次 fanfare 不是同 execution |
| history payload at:event | 合成：沒有直接卡例 | 截止在本次誘發事實，包含觸發本身；門檻為 at_least |
| captured history_count 加 cmp.eq | CP02-006、CP01-029、CP03-005 | 截止在本次誘發事實，包含觸發本身；恰為第 N 張／次，不能改為 at_least |

history 與 history_count 比較、顯式 distinct:facts 與省略、顯式門檻 1 與省略均不推定同 hash。

## 事件讀值與 it

`{event:amount}`、`{event:value}` 為數值（A.expr.event-number）。另兩個既有讀值
`{event:holder}` 為 Ref、`{event:move_receipt}` 為 ReceiptRef。沒有長形 kind/field、自由 path，
也不收 from/to/delta。C1／D0 必須由 enclosing on 窄化：

| 欄位 | 可讀事件 | 證據 |
| --- | --- | --- |
| amount | damaged、dealt_damage | CP02-SL10 |
| value | dice_rolled | BP21-SL19、BP21-075 |
| holder | equipped | GAP-A-058 |
| move_receipt | 有實際移動 receipt 的 enter_field/left_field/placed_ex/discarded/banished/destroyed/moved/drew | 語法 §14；creation 不可冒充 |

any_of 只能讀所有可能事件共有的同型欄位。片段驗證不驗 enclosing on；wrong-context 測試
驗 NumericExpr／Condition／Ref 等消費位置，不宣稱已驗完整能力的事件窄化。

| 事件 | it 的物件與快照責任 |
| --- | --- |
| enter_field、placed_ex | 入場新世代 |
| left_field、destroyed、discarded、banished | 事實保存的 before 快照 |
| played | 被打出的卡 |
| damaged | 受傷物件；領袖分支不冒充 Ref |
| dealt_damage、attacked、attack_target | 依序為傷害來源、攻擊者、目標 |
| evolved、super_evolved | 進化後的同物件世代 |
| acted、stood、raced、drive_gained、counter_changed、stat_changed | 事件主體 |
| ub_activated、ability_played | holder |
| drive_checked | 發起者；holder filter 可匹配其他對象 |
| fused、fused_as_material | 依序為融合能力主體、材料 |
| moved | 須有明示 EventSnapshotPolicy 才准入 |
| equipped | 以 event:holder 讀取，不推定 it=holder |
| targeted | 缺明示快照來源時拒絕，不猜 it |
| phase、life_changed、dice_rolled、drive_trigger、state | 不給物件 it |

缺來源、世代、基數或資料可見性時接線拒絕，不從目前同 ID 物件補值。

## 付款、骰值與選定快照

| 定義 | 必填形狀 | 值域與接線責任 | 卡例 |
| --- | --- | --- | --- |
| A.cond.paid | `{paid:additional或optional或{receipt:BindingName或EntryPaymentRef}}` | Bool；關鍵字須唯一定位同卡、同次 play 的費用 wrapper（可由 play 頭宣告、spell 本文讀取）；具名 receipt 用 mapping，避免同名關鍵字歧義 | optional：BP11-068、BP20-P12；receipt：BP13-098；additional：合成 |
| A.expr.x-paid | `{x_paid:BindingName}` | 已完成、單一有確定單位的材料／counter 費用步驟 actual；不收一般動作、複合 PaymentComplete 或自由宣告 X | BP03-001，為 x.from_payment 的明示等義拼法 |
| A.expr.dice | `{dice:BindingName}` | 最終接受骰值，不是宣告數；binding 來源必驗 | BP06-111、BP12-113、BP10-020 |
| A.cond.chosen | `{chosen:{ref:BindingName,where?:EventSubjectFilter}}` | 單一具名 Target 的選定快照，至少一符合者；省略 where 表非空；不收 at | BP17-030、BP20-073 |
| A.expr.player、A.cond.player-cmp-payload | `{player:PlayerRef}`；cmp.left/right 同此型，op 只 eq/ne | 不接受裸字串、bound、數字、ordered op；不以裸 binding 繞過型別 | 合成 |

paid 的合法 optional/additional 缺席為 false；拼錯／未宣告為錯誤；付款前不可讀。
既有 did 的付款判準仍是 PaymentComplete；paid 與 did 等義不推定同 hash。
`{paid:{receipt:additional}}` 明指同名 binding，與 `{paid:additional}` 的上下文關鍵字不同。
PP 短費用沒有 result，不能宣稱可供 x_paid；自由 X 的值不因費用減免而改成實付量。
既有 bound／裸別名規則不在此偷偷刪改，D0 須保留 binding 的來源與單位。

## LKI 與捕捉

`A.lki-of={lki_of:Ref}` 只進 counter.on（Ref 或 LkiOf）；不進可操作 Ref、PlayerRef、Sel。
BP07-088、BP10-102、BP17-008 支持離場／支付後的 counter 快照；一般 counter.on:Ref 仍為 Live。

power_of、hp_of、cost_of、orig_cost_of 接 Ref 或 SnapshotRead。Ref 保留依法 LKI；不另收 lki_of。
SnapshotRead 必須是單一成功材料、指定 view 且屬性可用，不能包任意 Sel 或以 agg 冒充單物件。
BP05-057、BP04-116 支持付款材料 before；after 與 hp 的材料屬性格標合成。
orig_cost_of 支持 BP12-114、BP13-053。agg.attr 新增 hp，sum/min/max 三格皆合成。

| 捕捉定義 | 必填鍵 | 拒絕 |
| --- | --- | --- |
| A.capture.timed | at:live/play/resolution/event、value:StateNumericRead | selection、未知時點 |
| A.capture.selection | at:selection、selection:BindingName、value:StateNumericRead | 缺合法決定／Target 錨點 |

`A.expr.capture` 是上列互斥 union。`A.state-numeric-read` 恰含 15 類：
count、hand、leader_life、resource、turn_number、zone_free、distinct_names、distinct_orig_costs、agg、
power_of、hp_of、cost_of、orig_cost_of、history_count、history_sum。
其中四個屬性讀值使用不同的 `A.capture.*-of`／Captured*Of 型別，只收 Ref，不收 SnapshotRead。
沒有 counter、算術根、常量根、bound、paid、dice、x_paid、event、objects_of、lki_of 或巢狀 capture。
算術的各 operand 可獨立包捕捉；不能把任意整棵 Expr freeze。

15 類 × 5 時點均有形狀正反例；真例只確認 history_count×event 的序數截止，
其餘交叉格是合成，不表示規則已准入。AtPlay／AtResolution 的 scope、AtSelection 的錨點與
可捕捉資訊、AtEvent 的事件 context 由 C1／D0 驗證。已選特徵優先 chosen。
省略 capture 的 read 是 Live；顯式 live 與省略不推定同 hash。history 條件另可在 payload 內
放 at:event；history_count/history_sum 只接受 wrapper，不另接受 payload at。

## 證據與驗證界線

卡例取自遞迴讀取 `card-data/jp/**/*.json` 中全部卡片文件與 faces 的 text/sections，
包括歷史文件：302 卡檔、7383 faces（另有 302 evidence JSON）。Q&A 不算卡例。
表中「合成」表示有契約形狀與測試，沒有直接實卡支持該格；事件主體的 power_ge、keyword 聯集、
多人 opp 與各玩家自己的 phase 語義，均未因此獲授權。

新定義逐一對照 Python，測試移除 required、開放 extra、改錯值域與分支；獨立行為例驗
事件角色、主體互斥、phase 參數位置、history 頂層／where 分工、sum 窄化、骰值邊界、
LKI 消費、paid 關鍵字撞名、Player cmp 同型，以及捕捉矩陣。
這些檢查不取代完整 Cost／Target、父動作、DSL 入口、Rust decoder 或引擎行為測試。
