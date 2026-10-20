# 動作與 Steps 形狀矩陣

本文件對應 `author-nodes.schema.json` 的 `A.action`、`A.step`、`A.steps` 與內層定義；Python 對照為 `author_programs`。JSON Schema 是語法權威。這些是封閉片段，完整能力頭見[能力矩陣](author-ability-shapes-1.0.md)，不含作者入口、展開或執行准入。通過形狀不代表來源、作用域、物件世代或 capability 已驗證。

`A.action` 恰一動作鍵；`A.step` 恰一動作或控制鍵；`A.steps` 是保序列表，保留重複步驟和空列表。所有固定物件拒絕未知鍵，不補預設。`optional.do` 需要最後一個指示的 receipt，`simultaneous.do` 沿 IR 批次契約，這兩處要求非空。其他空 Steps 仍須通過 C1 的來源及終止檢查。

## 共用型別

以下 O=Ref 或 ObjectSel；R=Ref 或 RecipientSel；P=PlayerRef 或 PlayerSet；B=BindingName；C=Condition；Q=Quantity；N=NumericExpr；E=EffectSpec；U=Until；Z=Zone。`?` 表選填；除 Steps 外列表在表中未另註時均非空。長形共通 M=`result?:B, precondition?:C`，choose 不帶 M，短形不能附 M。

CountAmount=Q 或 ParamRef，只供 draw/discard/create.n；DamageAmount=Q 或 ParamRef，只供 damage.n；RecoveryDelta=N 或 ParamRef，只供 life.delta，直接參數須是增加方向。這些角色仍按承載契約 §3 分別驗證，不借用 SelectionQuantity。直接參數只進白名單位置，群組搜尋數量、partition、allocate.total、內層目的地不繼承參數權限。名稱葉沿 CardName／Concept／CardKind，不新增任意 JSON。

## 選擇、搜尋、順序

| 變體 | 必填 | 可選及限制 | 證據 |
| --- | --- | --- | --- |
| choose.exact/up-to/any | bind、from:ObjectSel、n/upto:SelectionQuantity 或 any:true 恰一 | by:PlayerRef、may/reveal/order:bool、position:top_or_bottom、cost_sum_le:Q、distinct_by、groups | BP19-110、BP03-108、BP04-SL08 |
| choose.groups | 上列＋至少兩個 TargetGroupFilter | 每組容量 1、一次回答且 disjoint；總數由外層決定；無每組 bind/source/to | ECP02-034、BP05-003；exact/any 群組為合成 |
| search.exact/up-to | sel:ObjectSel、n/upto:SelectionQuantity、to:SearchDestination | M、cost_sum_le、distinct_by、entry_effects:E[]、instead | 沿單組數量矩陣 |
| search.grouped | groups:[{sel,n:Q}或{sel,upto:Q}] 至少兩組、to | 無頂層 sel/n/upto；共通欄同單組 | CP04-P68 sections/0 |
| SearchDestination 區域 | Z 或 ParamRef | 直接參數只限此整個目的地葉 | BP03-002 原目的地 |
| SearchDestination 分配 | order:printed、partition:[{zone:Z,n:Q}] 至少兩項 | 印刷順序消耗剩餘 found | CP03-SL07 |
| SearchDestination 取代 | zone:Z、instead:{if:C,zone:Z,may?:bool} | 找到並公開後判斷；可讀 found；無 found 不詢問；拒絕走原區；不收 after | BP03-002 |
| SearchDestination 逐組 | per_group:Z[] 至少兩項 | 只在分組 search；與 groups 等長且同序，由 C1 驗 | BP12-041 |

Search 的 found 是私有找到集合，revealed 是公開證明，moved 是實際成功移動的 after；不能互換。found/revealed 不流出 continuation，後續用 `result`＋`follow`。search.bind／move.bind 不接受；create.bind 是新建立物件，另有契約。整副被搜尋牌庫只在尾端洗一次。群組來源的 who／牌庫一致性、等長、disjoint、容量及成功成員由 C1/D0 驗證。

Position 有三支：top/bottom/{nth:正安全整數}；`{one_of:[上述位置至少兩項],by:PlayerRef}`；`{chosen:B}`。nth:1 合法。one_of 對每個物件決定位置，by 是位置選擇者，與 move.by 執行者分開。

同一句「選卡＋上か下」標準形為 choose.position:top_or_bottom，再 move.position:{chosen:同名 binding}（BP16-022）。回答同時保存所選物件及逐物件位置，不能拆成第二次詢問。chosen 只接同 execution 的支配宣告，move.ref 必須原樣引用同一 B，不准子集或另一名稱；目的地只能 deck。這些跨節點限制由 C1 驗。它不是 A.cond.chosen 的目標快照條件。既定集合（残り、既選目標、this）使用 one_of（BP01-008、BP14-039）；此為作者標準形，不是額外 C0 拒絕規則。

move.order 為 chooser/shuffle，要求 deck 且明寫 position；chooser 由 move.by 排序，shuffle 洗指定集合。choose.order:true 在同一次選擇回答已排序，後續引用該 binding 不再次 order；false 不建立排序證據。C1 驗來源順序，未展開目的地參數延至展開後驗。

「残り」寫 `{in:seen,except:{in:{objects_of:moved,view:before}}}`，扣除成功搬移的 before，保留容量不足而未搬走者。多次搬移可用 except.any_of。差集保留觀察順序，不重新查牌庫頂；不新增 rest/first/second/底來源節點。前者／後者使用兩個 Target binding 或既有 roles；分組搜尋使用 per_group。同牌庫 move 重排降低為 Order，不變世代；其 receipt 仍須提供成功 before 成員供差集消費。獨立 order 作者鍵不接受；既有分類 ID A.order 對應 choose.order／move.order。

## 動作逐 variant

| 動作 | 必填／變體 | 可選與互斥（長形另有 M） | 真例／合成 |
| --- | --- | --- | --- |
| draw | n:CountAmount | who:P或ParamRef、instead | BP08-038；who:each CP04-072 |
| discard | n:CountAmount / all:true / down_to:Q / any:true / ref:Ref | who:P；n 支收 sel:ObjectSel、random:bool，any 支收 sel；固定 ref 不再選材料 | BP19-072、BP02-052、BP10-076、BP09-020、BP01-111；random BP06-110 |
| damage | to:R、n:DamageAmount | distribute:true、distribute_at:play/resolution、instead；distribute_at 要求 distribute | BP01-125、BP03-007；resolution 合成 |
| destroy | 短 Ref 或長 ref:O | 長形 instead | BP01-123 |
| banish | 短 Ref／長 ref:O／長 top:DeckSlice | 長形 instead；拒絕 sel+n、bottom | BP21-047、BP05-020 |
| move | ref:O 或 top:DeckSlice；to:Z或ParamRef | position、order、reveal:bool、by:PlayerRef、acted:bool、entry_effects:E[]、instead；acted 限 field，position/order 限 deck | ETD01-005、BP01-008、BP04-SL08 |
| look | ref:O 或 top:DeckSlice；bind:B | by:PlayerRef 是看牌者；random:{n:Q} 只配 ref | BP06-108；random 合成且未取得執行准入 |
| reveal | ref:O 或 top:DeckSlice；bind:B | by:PlayerRef 是執行公開者；random:{n:Q} 只配 ref，來源須證為手牌 | BP11-112、CP03-105、BP01-111 |
| reveal_until | sel:ObjectSel、n:Q、bind:B、matched:B | 觀察與匹配集合分開 | BP03-055、BP10-074 |
| act/stand/flip_down/flip_up | 短 Ref 或長 ref:O | 不接受短 selector | BP01-029、BP02-019、BP17-112；本文 flip_up 合成 |
| life | who:P、delta:RecoveryDelta | instead | BP01-013 |
| set_life | who:P、n:Q | 設定與增減分開 | BP02-075、BP05-092 |
| buff | on:O、power:N/hp:N 至少一 | until:U、instead | BP01-011；不是 static |
| set_stat | on:O、power:Q/hp:Q 至少一 | until:U | BP16-049、BP15-001 |
| grant | on:R、keywords:[Concept或ParamRef]／abilities:[GrantedAbilityHead] 至少一項，可並用 | until:U、instead | BP19-056、BP11-P27；領袖／混合／裝備見能力矩陣 |
| lose_abilities | on:O | until:U | BP05-061 |
| counter | on:O、name:CounterName、delta:N | 與費用 counter 分開 | BP20-030 |
| move_counters | from:O、to:O、name、all:true | 不接受部分量 | BP01-T10 |
| pp | who:P、delta:N 或 to_max:true | 兩支互斥 | BP03-057；to_max 合成 |
| pp_max/ep/sep | who:P、delta:N | 不接受 to_max | 此動作矩陣以合成格驗證 |
| shuffle | who:P | 整副牌庫 | BP05-054 |
| dice | bind:B | who:PlayerRef | BP06-110 |
| declare.number | bind:B、kind:number | min/max:Q、who:PlayerRef；上下界關係 C1 驗 | BP08-103、BP06-111 |
| declare.name | bind:B、kind:name | who:PlayerRef；拒絕 min/max | BP12-115 |
| equip | on:O、token:CardName | token 不收直接 param | CP04-019、CP04-001 sections/0 |
| steal/give | ref:O | to:PlayerRef或other_players；不另造玩家 wrapper | give BP13-043；steal 合成 |
| stack_plus | N 短形 | 不收 M | BP10-050 |
| box | ref:O、until:U | until 必填；之前的 act 另一步 | BP11-018 |
| race | ref:O | n:Q；省略為一次 | CP01-042 |
| gain_drive | ref:O | 與檢查程序分開 | CP03-008 |
| play_card | ref:O | cost 尚不接受 | BP13-036 |
| play_ability | of:O；trigger 或 ability:{ub:true,n:正安全整數} | cost_policy:normal/waive_original；兩支互斥 | BP14-023、A-016e |
| apply | on:O、effect:E | until:U、capture:play | BP12-109、CP04-060 |
| player_effect | who:P、effect:E | until:U | BP14-046 |
| win/skip_turn/extra_turn | who:P | 三個不同操作 | BP04-003、合成、EBD02-007；BP05-086 是費用，非 skip_turn 動作例 |
| make_type | on:O、type:CardKind | until:U | BP05-001 |
| evolve | ref:O | into:CardName／非空 CardName[]／ObjectSel；不收直接 param | A-060；BP09-004 的能力頭不算動作例 |
| transform | ref:O、into:CardName或{allocate:{among:CardName[],each_exactly_once:true}} | entry_effects:E[]；into 不借 evolve 列表 | A-016j |

TriggerKind 的封閉值為 fanfare、lastword、evolve_time、super_evolve_time、attack_time、race_time、drive_time。entry_effects 在成功產生的新世代上、入場誘發掃描前安裝；no_trigger 的 entry occurrence 責任仍沿效果矩陣。look 的多玩家 top 須使用 each_player，不能把 PlayerSet 當單一 viewer（BP06-113）。

create 必填 token 與 to；token 為 CardName／ParamRef／兩者的非空列表／`{allocate:{among:CardName[],total:Q,by?:PlayerRef}}`。可選 n:CountAmount／upto:Q／any:true 恰至多一，省略每個名稱 1；allocate 已有 total，不可再填外層數量。to 為 Z／ParamRef／`{one_of:Z[至少兩項],by:PlayerRef,scope:each|batch}`。可選 counters（非空 CounterName→Q map）、by、acted、entry_effects、bind、instead、M。acted 的已知區域限 field；選項與參數目的地由 C1 展開後驗。token 單值／列表見 GAP-A-002，allocate 見 A-016g，目的地選項 GAP-A-017，upto GAP-A-039，bind A-016f；any、counters 為合成格。

delayed 與 grant.abilities 沿同一遞迴圖，完整形狀見[能力矩陣](author-ability-shapes-1.0.md)。delayed 只帶 precondition 共通 metadata，不收 result。pilot、feed、drive_check、play_card.cost 沒有本矩陣的接受形狀。pilot 的舊 `{ref,on}` 無法承載印刷數值與期限，不能拿提醒文冒第二個物件。這個有限 Step union 不宣稱所有登錄構造均可承載。

## 自我取代

除 choice 外，entry 必填 if:C；同型差異或 replace_with:A.action 恰一種；可帶 after:Steps。差異至少一欄，不收 result/precondition；replace_with 可同種，但不接受控制流程。

| 動作 | 可取代欄位 | 證據 |
| --- | --- | --- |
| damage | to:R、n:DamageAmount | CSD01-010；to 合成 |
| move | ref:O、to:Z或ParamRef、position、order、acted | BP01-008 改 ref；其餘合成 |
| buff | power:N、hp:N | BP04-082、CP04-P60 |
| life | delta:RecoveryDelta | 合成 |
| grant | keywords:[Concept或ParamRef]、abilities:[GrantedAbilityHead] 可並用 | keywords BP15-029 sections/0；abilities 合成 |
| create | n:CountAmount或upto:Q、to:CreateDestination | ETD01-018、BP07-P20；to 合成 |
| draw | n:CountAmount | 同型合成；replace_with:search BP07-096 |
| search | sel:ObjectSel、n或upto:SelectionQuantity、to:SearchDestination或per_group | 同型合成；BP13-035 完整替換群組 |
| banish/destroy | ref:O | 合成 |

if 在原指示開始讀，search.instead 不能讀 found；找到之後的取代只放 search.to.instead。數量差異取代整個數量 variant（n→upto 時清掉 n）；合併後須驗完整合法 payload。ref/top、單組/群組切換請用完整 replace_with。after 只在取代成功適用後執行；禁止重入循環，C1 驗依賴及執行身分。

ChoiceInstead 獨立為 `{if:C,n:Q}` 或 `{if:C,upto:Q}`，不接受 after／replace_with；在打出時決定模式數，並非結算期動作取代（BP02-045、BP21-026）。

## 控制流程逐 variant

| 構造 | 必填 | 可選／限制 | 證據 |
| --- | --- | --- | --- |
| optional | do:非空Steps | who:PlayerRef、then:Steps、result:B | BP19-072 |
| pay | cost:Cost、do:Steps | who、then、result；then 仍受完整付款保護 | A-060、A-042a |
| choice.exact/up-to | n或upto:Q、options:至少一個Steps | who、exclude_chosen:this_turn、instead:ChoiceInstead[] | BP02-106、CP04-SL19；單選項合成 |
| either | options:至少兩個Steps | who、bind；bind 為選中選項的 1 起算序數，只由 bound 讀 | BP10-050；bind 合成 |
| if | cond:C、then:Steps | else:Steps | A-001 |
| repeat.count | n:Q、do:Steps | collect:{receipts:B}；不收 check | BP14-077、BP17-004 |
| repeat.until/while | until或while:C、do:Steps | check:before/after、collect | A-016c；while 合成 |
| repeat_ability | while:C | 不重新 play | BP05-054 |
| foreach | in:O、as:B、do:Steps | 不把群體指示偷換成逐一 | 合成；BP01-008 是 move 的位置選擇，不算 foreach 真例 |
| each_player | stages:至少一項Stage | who:each、as:B | BP13-043、BP06-113 |
| simultaneous | do:非空Steps | result:B | A-016i；單步合成 |
| by_kind | ref:R、leader:Steps、follower:Steps | as:B | BP21-P54 |

Stage=Steps 或 `{do:Steps,mode:choice|nonchoice}` 或 `{do:Steps,mode:explicit,evidence:非空字串}`；evidence 只配 explicit。each_player 省略 as 時 item 是該玩家，具名時只宣告該 PlayerRef，禁止隱式遮蔽。每 stage 跨玩家執行；draw.who:each 是單一群體指示，不能取代多階段程序。玩家作用域、終止性及時序由 C1/D0 驗證。

## 付款決定與准入責任

`{payment_decided:optional}` 是付款前的決定，定位到同一張卡、同一次 play 的唯一可選費用宣告；不是 paid，也不是 x_paid。C1 只准 choice.instead.if 及其布林組合，拒絕跨 play、多組歧義、其他本文／selector／history 位置和循環依賴。共用 Condition 的形狀驗證不代替位置准入。paid:optional 以同樣規則定位費用，但只讀已完成付款。

BP21-026 與 PR-585 是同卡異印，來源文分別含可選 +2 費用及一項→至多兩項模式。能力內容示意如下（完整作者容器仍須將 play、spell 分成各自的 A.ability，不允許單一能力兩個頭）：

```yaml
play:
  optional_cost: {pp: 2}
spell:
  do:
    - choice:
        n: 1
        instead: [{if: {payment_decided: optional}, upto: 2}]
        options:
          - [{search: {sel: {who: self, zone: deck, kind: follower, name_contains: エミリア}, n: 1, to: field}}]
          - [{search: {sel: {who: self, zone: deck, kind: follower, name_contains: リーシャ}, n: 1, to: field}}]
```

C1/D0 還須接上 per_group、compound 位置回答、付款決定、每玩家作用域、Order 成功 before 收據及展開後的合法 payload 檢查；本片不更改公開 IR 或 capability，不宣稱這些卡已可執行。多張 found 的條件量化、BP01-008 同位置多牌的相對順序及 pilot 數值／期限不得靠 C0 猜測。

驗證範圍含每個定義的 Python/schema 漂移、負向 schema 突變、獨立卡例與合成 variant、未知鍵／錯型別／互斥／錯上下文、遞迴與 canonical roundtrip。卡例核對遞迴涵蓋凍結 JP 302 檔、7369 documents、7383 faces，以及 EN 297 檔、7420 documents、7437 faces 的 text/sections；Q&A 不作卡例。未找到直接卡例的格明列合成，不將 shape 通過宣稱為全卡語義驗收。
