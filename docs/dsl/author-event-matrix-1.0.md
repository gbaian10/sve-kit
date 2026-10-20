# 作者事件 payload 與 history.where 矩陣 1.0

這兩張表展開[事件與歷史契約](author-history-shapes-1.0.md)，沒有新增事件語義。
語法權威為 [`author-nodes.schema.json`](../../dsl/author-nodes.schema.json) 的
`A.event`、`A.history-query`。所有物件封閉；未列欄位拒絕。
Zone=field/hand/deck/evolve_deck/cemetery/ex；Phase=start/main/end。
During={turn:PlayerRef}，By={ability_of:EventObjectSel}。SubjectFilter／EventRecipient 的形狀見事件與歷史形狀文件。
主體 ref 與 sel 互斥；一般空事件 payload 合法，不表示任意 it／讀值可用。
Ref／PlayerRef 見[參照形狀](author-reference-shapes-1.0.md)，不繼承 selector 的直接參數權限。
`A.nat-threshold` 恰一鍵 `{eq|le|ge: Nat}`；`A.die-threshold` 恰一鍵
`{eq|le|ge: 1..6}`，兩者都只收整數常值，不收 bool、運算式或參數。
`A.move-origin` 必填 `{who: PlayerRef, zone: Zone}`；Condition 見[運算式形狀](author-expression-shapes-1.0.md)。
直接事件與歷史的主體窄化、來源角色、讀值及 LKI 仍需 C1／D0 接線。

卡例沿事件與歷史形狀文件的凍結日文來源；本矩陣不把 Q&A 命中算為卡例。
「合成」只表示契約形狀的測試，不能據此宣稱該格已有真卡或可執行。

## 直接事件（33 鍵）

| 事件 | payload 必填 | payload 選填（型別／enum） | 卡例／合成 |
| --- | --- | --- | --- |
| enter_field | 無 | from:Zone、ref:Ref、sel:SubjectFilter、who:PlayerRef、during:During、entered_by:ability/play、by:By | BP01-033、BP10-004；entered_by/from/by 合成 |
| left_field | 無 | ref:Ref、sel:SubjectFilter、who:PlayerRef、during:During、by:By | BP01-016、BP04-108；by 合成；不收 to |
| placed_ex | 無 | from:Zone、ref:Ref、sel:SubjectFilter、who:PlayerRef、during:During、by:By | CP04-007、BP14-027；from/by 合成 |
| discarded | 無 | from:hand、ref:Ref、sel:SubjectFilter、who:PlayerRef、by:By、during:During | BP01-170、BP09-006、BP21-043 |
| banished | 無 | from:Zone、ref:Ref、sel:SubjectFilter、who:PlayerRef、during:During、by:By | BP14-074；during/by 合成 |
| destroyed | 無 | ref:Ref、sel:SubjectFilter、who:PlayerRef、during:During、by:By | BP01-106、BP03-034；by 合成 |
| damaged | 無 | recipient:EventRecipient、kind:attack/combat/ability、during:During、by:By | CP02-SL10、BP21-037、BP21-109；by 合成 |
| dealt_damage | 無 | ref:Ref、sel:SubjectFilter、who:PlayerRef、zone:Zone、recipient:EventRecipient、kind:attack/combat/ability、during:During、by:By | BP04-003、CP03-064、ECP02-057；by 合成 |
| attacked | 無 | ref:Ref、sel:SubjectFilter、who:PlayerRef、during:During | CP03-019、BP12-008；during 合成 |
| attack_target | 無 | ref:Ref、sel:SubjectFilter、who:PlayerRef | 合成 |
| evolved | 無 | ref:Ref、sel:SubjectFilter、who:PlayerRef、by:By | BP02-016、BP20-004；history BP16-018；by 合成 |
| super_evolved | 無 | ref:Ref、sel:SubjectFilter、who:PlayerRef、by:By | auto／history 均為合成 |
| phase | which:(Phase 或 ParamRef)、whose:PlayerRef | — | BP05-006、BP01-153 |
| played | 無 | from:Zone、who:PlayerRef、sel:SubjectFilter、during:During | CP02-044、BP14-036、BP04-064；from 合成 |
| ability_played | 無 | who:PlayerRef、holder:(Ref 或 EventObjectSel) | 合成 |
| life_changed | 無 | who:PlayerRef、direction:up/down、during:During、by:By | BP02-095、BP19-075；by 合成 |
| stat_changed | 無 | ref:Ref、stat:(power/hp 或 非空 [power/hp])、direction:up/down | BP11-082、BP21-100；不收 subject/property/by |
| counter_changed | 無 | ref:Ref、name:CounterName、after:A.nat-threshold、direction:up/down | 合成；不把 Stack 提醒文另造誘發 |
| acted | 無 | ref:Ref、sel:SubjectFilter、who:PlayerRef | CP04-102、BP11-069 |
| stood | 無 | ref:Ref、sel:SubjectFilter、who:PlayerRef | 合成 |
| raced | 無 | ref:Ref、sel:SubjectFilter、who:PlayerRef | CP01-082、CP01-032 |
| drive_gained | 無 | ref:Ref、sel:SubjectFilter、who:PlayerRef | auto 形狀合成 |
| ub_activated | 無 | who:PlayerRef、holder:(Ref 或 EventObjectSel) | CP04-SL15 |
| fused | 無 | who:PlayerRef、material:SubjectFilter | BP20-SL05、BP20-029；至少一成功材料符合為推論 |
| targeted | 無 | ref:Ref、by:By | BP03-071；by 合成 |
| state | Condition | — | BP05-092；不是歷史事實 |
| dice_rolled | 無 | who:PlayerRef | BP21-SL19；on 不收骰值門檻 |
| drive_checked | 無 | holder:(Ref 或 EventObjectSel) | CP03-P27、CP03-065 |
| drive_trigger | 無 | who:PlayerRef | CP03-P81、CP03-110 |
| moved | 無 | from:(Zone 或 A.move-origin)、ref:Ref、sel:SubjectFilter、to:Zone、during:During、by:By | BP05-029、BP01-012、BP20-025；by 合成 |
| drew | 無 | who:PlayerRef、not_phase:Phase、by:By | BP05-094；by 合成 |
| equipped | 無 | equipment:(Ref 或 EventObjectSel) | CP04-PR22；selector 合成 |
| fused_as_material | 無 | ref:Ref、by:By | BP19-048 |

counter_reached 是額外巨集：ref:Ref／name:CounterName／value:Nat 三者必填，沒有選填欄位。
加上此巨集共有 34 個可直接寫的鍵；它以 counter_changed 的 after 比較展開，不新增歷史事實。
any_of 是非空事件列表組合，可含 counter_reached，但不含 state／巢狀 any_of。

## 歷史（32 個非 state 事件與 payment）

每列頂層均必填 event 與 window；window=this_turn/game/{since:BindingName}。
共通選填 distinct=facts/subjects、exclude=this_execution；condition 另有 at_least:Quantity、at:event。
有 where 時必非空。除 phase／payment 外 where 選填；下表「where 必填」是提供 where 時的 required 欄位。

| event | 頂層額外必填 | 頂層事件選填 | where 必填 | where 選填（型別／enum） |
| --- | --- | --- | --- | --- |
| enter_field | 無 | from:Zone、sel:SubjectFilter、who:PlayerRef | 無 | ref:Ref、during:During、entered_by:ability/play、by:By |
| left_field | 無 | sel:SubjectFilter、who:PlayerRef | 無 | ref:Ref、during:During、by:By |
| placed_ex | 無 | from:Zone、sel:SubjectFilter、who:PlayerRef | 無 | ref:Ref、during:During、by:By |
| discarded | 無 | from:hand、sel:SubjectFilter、who:PlayerRef | 無 | ref:Ref、by:By、during:During |
| banished | 無 | from:Zone、sel:SubjectFilter、who:PlayerRef | 無 | ref:Ref、during:During、by:By |
| destroyed | 無 | sel:SubjectFilter、who:PlayerRef | 無 | ref:Ref、during:During、by:By |
| damaged | 無 | — | 無 | recipient:EventRecipient、kind:attack/combat/ability、during:During、by:By |
| dealt_damage | 無 | sel:SubjectFilter、who:PlayerRef、zone:Zone | 無 | ref:Ref、recipient:EventRecipient、kind:attack/combat/ability、during:During、by:By |
| attacked | 無 | sel:SubjectFilter、who:PlayerRef | 無 | ref:Ref、during:During |
| attack_target | 無 | sel:SubjectFilter、who:PlayerRef | 無 | ref:Ref |
| evolved | 無 | sel:SubjectFilter、who:PlayerRef | 無 | ref:Ref、by:By |
| super_evolved | 無 | sel:SubjectFilter、who:PlayerRef | 無 | ref:Ref、by:By |
| phase | where | — | which:Phase、whose:PlayerRef | — |
| played | 無 | from:Zone、who:PlayerRef、sel:SubjectFilter | 無 | during:During |
| ability_played | 無 | who:PlayerRef | 無 | holder:(Ref 或 EventObjectSel) |
| life_changed | 無 | who:PlayerRef | 無 | direction:up/down、during:During、by:By |
| stat_changed | 無 | — | 無 | ref:Ref、stat:(power/hp 或 非空 [power/hp])、direction:up/down |
| counter_changed | 無 | — | 無 | ref:Ref、name:CounterName、after:A.nat-threshold、direction:up/down |
| acted | 無 | sel:SubjectFilter、who:PlayerRef | 無 | ref:Ref |
| stood | 無 | sel:SubjectFilter、who:PlayerRef | 無 | ref:Ref |
| raced | 無 | sel:SubjectFilter、who:PlayerRef | 無 | ref:Ref |
| drive_gained | 無 | sel:SubjectFilter、who:PlayerRef | 無 | ref:Ref |
| ub_activated | 無 | who:PlayerRef | 無 | holder:(Ref 或 EventObjectSel) |
| fused | 無 | who:PlayerRef | 無 | material:SubjectFilter |
| targeted | 無 | — | 無 | ref:Ref、by:By |
| dice_rolled | 無 | who:PlayerRef | 無 | value:A.die-threshold |
| drive_checked | 無 | — | 無 | holder:(Ref 或 EventObjectSel) |
| drive_trigger | 無 | who:PlayerRef | 無 | —（不接受 where） |
| moved | 無 | from:(Zone 或 A.move-origin)、sel:SubjectFilter、to:Zone | 無 | ref:Ref、during:During、by:By |
| drew | 無 | who:PlayerRef | 無 | not_phase:Phase、by:By |
| equipped | 無 | — | 無 | equipment:(Ref 或 EventObjectSel) |
| fused_as_material | 無 | — | 無 | ref:Ref、by:By |
| payment | where | who:PlayerRef | cost_kind:earth_rite | — |
