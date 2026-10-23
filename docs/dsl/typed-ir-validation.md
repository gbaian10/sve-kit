# 內部 typed IR 良構邊界

`sim/engine/src/ir/wellformed.rs` 的 `WellFormedProgram::check` 接收並持有一個
`Program`，成功後只能取得不可變參照。呼叫者無法直接構造已驗型別，也不能修改
其中的程式體。它驗證的是封閉程式體，並把輸入的 root 視為一次執行的完整本體；
沒有接受外部 binding／receipt 的入口。
`SourceHolder`、控制者與持有者仍是執行環境提供的符號。

此程式體邊界不代表整個 `CardProgram`、能力宣告、capability 或可執行性通過。
`Registry::check_capabilities` 仍只檢查需求，不能產生此型別；已驗程式體也不能
跳過 capability 與執行環境的准入。這些 Rust 型別沒有公開 wire 格式。

## 作用域與結果

- 每個程式體的 `ScopeId`、整棵樹的 `InstructionId`、`DecisionId` 與 `ReceiptId` 不可重複。
  binding 以 `(ScopeId, id)` 識別，同一 slot 不可改用另一個型別重新宣告。
- 宣告先檢查輸入，再公開輸出。使用處須由宣告支配，並吻合型別及一般值、capture、
  本輪 receipt 三種用途。未執行分支也檢查，不依常數條件略過。
- sequence、if 的子程式不隱含匯出 binding 或 receipt。分支間不能互讀；沒有隱含
  branch join。selector／predicate candidate 只在自己的 predicate 內可見。
- each-object 的元素只在其 body 內可見。each-player 的玩家在各 stage 可見；
  同一玩家的 stage 輸出會交給其後續 stage，包含選取 binding、骰值及 receipt，
  但不會流到迴圈外。每個 stage 保留自己的 scope 與 choice 限制；後續
  ChoiceProcedure 不會繼承前一 stage 的 NonChoiceSimultaneous 限制。
  BP13-043 種子涵蓋先指定、後依明示順序交付及交付各自的場容量前提；
  BP06-110 種子涵蓋 fanfare 本文：各玩家擲骰，1–3 抽二、4–6 隨機捨二，
  擲骰、抽牌與捨牌的 actor 都是該次玩家。BP06-110 另有下述整卡結構種子，
  包含自身入場與印刷進化能力；兩卡的實際結算仍未驗收。
- simultaneous 的 choice stages 先於成員提交；同 stage 的 decision 讀共同入口，
  後一 stage 可讀前一 stage 的公開不可變 Nat 回答。成員共用完成 choices 後的環境，不能讀 sibling
  的結果。choice bindings 只在批次內可見，成員輸出與 receipt 完成整批檢查後公開。
  空批次拒絕。帶前提 decision 必須與唯一 member 的完整 guard 相等，
  包含 kind、reason、predicate 與 protects；其回答只交給該 member，不能流至其他 stage
  或 sibling。所有 member guard 在共同批次開始環境檢查，不能讀本批 choices 或產出。
  批次成員與 NonChoiceSimultaneous player stage 不能藏入數值 declare，
  也不能藏入 draw／look 的 UpToStepwise（逐張決定）；
  each-object 的 Chosen 順序也尚未接受。
- repeat 的 export 必須指向自己 body 結束時可見的 local receipt，並符合 producer
  outcome 型別；不能重新匯出外層 receipt 或本輪 binding。iteration 只在
  AfterIteration 的終止條件可見；Count 與 BeforeIteration 不能使用它。
  collect 只在迴圈後可見，並保留 `ReceiptList<T>` 的元素型別。
- instruction precondition 的 `protects` 必須是該 instruction，述詞在其輸出與
  receipt 公開前檢查。帶前提的指示不匯出新 binding，因為可能沒有開始；
  包含單獨指示、guarded pair 與 batch member。receipt 保留，由執行器表示未開始的結果。
  一般 body 不接受 Cost instruction。

## 接受的形狀

接受 sequence、if、simultaneous、直接 body 的 repeat、each-object、each-player
與下述結算付款、May 和模式；instruction 接受 draw、look、counter、reveal、flip-down、posture、
初次擲骰、數值 declare、life、resource、transfer，以及下述 discard、damage、token、move、search、destroy、banish、install-effect、modify、order。UpToStepwise 的決定描述及逐張協定見下節。
本文的 resource 不接受 Pay；Pay 只屬於 K.pay 的成本步驟。draw、discard 與 search 目前只接受
actor 就是抽牌者／捨牌者／搜尋者。擲骰者只由 instruction actor 表示，checker 無法對照卡文，
lowering 必須寫入實際擲骰的玩家。
capture 只接受 Int、Nat、Bool、
Selector 的 `AtResolution`，且必須位於 root body 開頭連續的 capture 前綴；
有打出付款時，前綴位於 `K.pay PlayParameter` 之後：
AtResolution 在進入執行時一次凍結，之後的節點會看到本體自己造成的變更，
巢狀 sequence／if 也不是新的解算。前綴各 capture 同時凍結，輸入只能用入口
可見的值（能力入口包含已選參數與已完成的打出付款結果），不能引用同前綴的其他 capture。本文其他位置回報 `UnsupportedShape`，
scope 不是 root 回報 `WrongScope`。

值檢查涵蓋這些形狀使用的 signed arithmetic、明示 Nat 轉換、條件值、部分 Live
讀值、數量 receipt／collect 投影、集合及局部 candidate。支援的式子依各運算元
遞迴檢查，不以整個型別家族推定所有變體都通過。
有號數不會先截成零；Add、Subtract、Multiply、FloorDivide 要求兩個運算元，
HalfUp 要求一個，Minimum／Maximum 與作者 `min`／`max` 相同，至少一個。
FloorDivide 只接受可靜態確認的正整數常值除數，其他動態除數明確拒絕。
InformationOrZero 接受 Cost／Stats 的 Original／Effective 特徵讀值；LastKnown 與
ProspectiveEntry 尚未接受，不能用一般讀值替代 LKI 或入場取代情境。
後續來源資訊 producer 與常見觸發的限定擴充見[觸發與 LKI 契約](trigger-lki-semantics.md)；
其中 SourceInformation 是待新增的內部變體，本 checker 尚未實作。

其餘形狀一律拒絕，包含下述來源持有者靜態離區改道以外的 replacement、
未驗的事件投影、下述範圍以外的 effect 安裝及能力宣告。未承載的 decision／付款時點回報 `DecisionTiming`，
其餘未驗形狀回報 `UnsupportedShape`。
拒絕形狀的程式不會得到已驗包裝。這個拒絕不是「語法無效」或「引擎已實作」的宣稱。
通用 `A.comb.instead` 沒有可構造的 typed IR 節點；作者輸入的拒絕診斷仍屬 lowering
邊界，不能靠塞入佔位節點繞過。

## 結算選擇與付款

獨立 decision 只接受 `ResolutionChoice` 的 Required 物件／Nat／compound 域。物件／Nat
的 decision 與 domain cardinality 必須一致；compound 的外層固定 exact 1，表示一次回答，
各子域各自保留基數並讀共同入口，不能讀 sibling 回答。空 compound 表示 Execute 無 payload。
Order 等未驗域在 compound 子域內也拒絕。
min=0 仍是選擇，不能省掉。MayDecline 必須由 `K.may` 提供 Execute 區段；普通本文不能
假冒打出／規則程序入口。

帶 precondition 的 decision 必須緊接其受保護的 instruction，兩者保存相同前提，
`protects` 指向該 instruction；前提不能讀其尚未產生的回答。這是一個指示開始點，
不是提前詢問再於 instruction 重讀前提。回答只在該 instruction 可用，之後保留
receipt；不能越過其他節點、跨 stage 或外洩回答。受保護的 instruction 可能未開始，
因此它的 Reveal 不公開證據，它的 FlipDown 仍視為可能已執行。BP13-043 獨立指定的選擇不應掛上
後續交付的前提。

discard 接受 Chosen、Random、All、DownTo。Chosen 群組必須 disjoint；群組、population、
eligible 透過目前手牌的 zone、filter、difference、union 或有手牌限制的 intersection
證明子集。無法靜態證明的 selector 拒絕；不靠虛構物件列舉驗證動態集合。
目前只接受 actor 就是手牌持有者。選擇域的公開 zone 白名單為 Field、Ex、Cemetery、
Resolution、Race、Drive、Trigger、Equipment；Hand 只容許持有者私有可見性，包含
述詞內巢狀集合讀取。Deck、Evolve、Banish 一律拒絕；Evolve 的私有牌組與 Banish
的裏向物件需另有可見性證據，不能把整區當公開。限定進化入口的私有 Face 選擇
是下節明列的例外，不放寬一般選擇域。

選擇域要求來源證據：物件／整數回答保留該 decision 的 visibility，Look receipt 只給
viewer；沒有 start guard 的 Reveal 直接公開一個 bound／captured 集合或 Look receipt 後，
該集合可作 Public 選擇域。帶 guard 的 Reveal，包括 guarded decision 之後的受保護指示與
batch 成員，可能沒有執行，不授權。動態 zone 的 Reveal 不自動授權後來重新求值的整區。
Token 的 created 集合是公開的。
AtResolution／AtSelection／AtPlay capture 依輸入傳遞 Public 或特定玩家的證據；
私有集合計數不能透過 scalar capture 公開。EachObject 的元素繼承集合證據；
repeat 的 iteration／collect 繼承 local receipt 的證據。

FlipDown 採 fail-closed 子集：同一能力依檢查順序一旦可能執行 FlipDown，
之後一律不接受物件證據，包括 bound、capture、receipt、collect 與迴圈元素。
repeat、each-object 與 each-player stage 的本文含 FlipDown 時，從該迴圈開始就不接受，
因為下一輪在本輪翻面之後。之後新的 Look、Reveal、Token 或選取也不能恢復物件證據；
這不是只撤銷被翻面的成員。不可變的 scalar 回答與 scalar capture 保留原本證據。
翻面後重新觀察的授權（觀察 epoch）尚未承載。

這是保守的靜態來源證明，不取代 executor 的 Observation、
世代、當時區域與快照檢查。手牌 Bound／Objects 即使已知，仍要與當時手牌相交才能當 discard 材料。

`K.pay` 接受 Ordered（包含空串列）與非空 Alternative；所有分支都驗證，替代分支的
materials／receipt 不隱含 join。Optional 的「付不付」、alternative 與成本材料是同一
付款程序，不能拆成第二次任意回答。材料 binding 只引用該成本指示原本的選取。
已驗成本為 discard、移除 counter、act、常值非負 Resource Pay（PP、PP 最大值、EP、SEP
同一形狀）、常值非正 Life Add；
其他成本保守拒絕。Cost 不得帶可略過指示的 precondition。Ordered 的材料／receipt
只對後續成本及 on_paid 可見，付款 receipt 在所有成本檢查完才公開。

PlaySpec.costs 接受同一 CostSpec 檢查。`K.pay PlayParameter` 僅可放在有 play 入口的
root body 前綴，on_paid 必須為空；打出付款在結算開始前完成，付款結果可供其後的
AtResolution capture 與結算本文判斷，capture 之後不能再出現打出付款。這個位置是
打出付款的內部承載，不是到了結算時重新付款。一般 body、巢狀區段、本文動作之後
皆拒絕。獨立 CostMaterials／OptionalCost decision 仍未接受，避免與 CostStep 內的材料
或 optional 回答重複詢問；卡片固有 PP 費用仍須 lowering 明示為 PlaySpec.costs。

repeat 可匯出 PaymentResult；本輪 complete 與 collected payments 的 All／Any complete
保留型別。**完整付款不是從 actual>0 推論**：零費用依法完成；合法取代後原費用視為
執行，但不能偽造原動作的物理事件（CR10.6.2.5.2–.3）。本 checker 不執行付款，
executor 必須先證明整個被選 CostSpec 可付，按規則處理取代，材料只選一次，任何一步
失敗不得留下部分消耗；非法 play 的 rollback 也包含事件、待機及使用額度。
PaymentResult 的建構、zero／replaced 投影、動態費用與完整 rollback 尚未接入。

## 分配與 producer

RecipientSet 接受 Objects、Leaders、Union、Explicit 與已定義的 Bound；空集合與空
Union 都合法。每個成員遞迴驗證，沒有從靜態語法捏造 runtime membership。
Distribution 是一次完整回答，ResolutionChoice 的外層 cardinality 固定 1；
PlayParameter Distribution 也可承載。Damage Allocation 只能讀支配它的分配 binding，
且 recipients 必須與該分配域精確相等，不能改成其他集合或同 slot 不同型別。
目前不驗分配 binding 的消耗者數量；沒有消耗者或多次引用也不在此層拒絕。
lowering 必須依卡文產生正確數量的傷害指示，不能將本檢查當成一次消耗的證明。
Distribution 明示 `minimum: Nat`，沒有預設；結算時的實際卡例與最低量須在 lowering
驗收，不外推打出目標的最低量規則。checker 只驗型別，不從數字推論卡文或最低量來源；
打出 snapshot、trusted producer、runtime 准入及正反例依[執行語義 §1](runtime-semantics.md#1-分配最低量與打出-snapshot)。

`ResolvedRecipients` 只正規化已求值的成員，依物件 ID＋世代／玩家去重，保留空集合。
`accepts_answer(total, minimum, answer)` 檢查每個成員恰出現一次、沒有外來世代、每項
最低量與總和（含溢位）；selected play targets 的 minimum 必須為 1（CR10.6.2.4.2、Q1070）。
空集合接受空結果，不問分配、不執行該分配傷害，無論 total 為何（CR10.6.2.4.1、Q1069）。
這不是動態合法性證明；executor 必須用正確 snapshot 求域與 total，先適用分配取代，
把同一 membership 交給傷害 producer，不能重新求值成另一批 recipients。

Damage 目前接受 Ability flags、Instruction batch、Current source，uniform signed
amount 或 Allocation；不接受攻擊／交戰來源或 LKI。結果為 DamageQuantityOutcome，
可讀 quantity、repeat export／collect，不能冒充 Cards receipt。
Damage requested／actual 的整指示合計依執行語義 §1.3；目前 executor 尚須完成該契約
才能解除投影拒絕，不能把舊 authority 保存的 Uniform 單項 amount 當成聚合 requested。
Token 檢查所有模板的常值名稱、數量、Field／Ex 單一目的區及可選 StackCounters；
空模板串列、未驗 entry_effects、allocated destination 與動態名稱拒絕。output 在輸入
檢查後才定義，receipt 為 TokenOutcome；同 batch 的 sibling 看不到它，批次結束才公開。
模板身分／名稱相符、實際建立世代、共享容量裁切與入場事件由 executor 驗證。
一般目的區滿不自動成為 P.start；BP13-043 的 transfer 前提是個別卡文，不能泛用到 token。

## 搜尋、移動與減費安裝

`Action::Modify` 獨立接受 Numeric Add／Set、非空且全為 Numeric 的遞迴 Combined，
或單獨 SetKind。GrantAbility、SuppressAbility、名稱／類型增減及混合層 Combined
整棵拒絕。subjects、每個數值、source 與 expires 的引用都檢查；數值可以有號，
不能在 IR 邊界先截為零。CurrentInstallation 表示本指示建立的修正實例，
Bound source 仍須有支配且型別相符的 EffectId producer（目前沒有此 producer）。
期限沿用安裝期限檢查，但 usage 一律拒絕，因為尚無數值修正消耗規則。
Numeric 的 stat_changed 與 InstallEffect Continuous 的差異依 IR GAP-S-008；
checker 不執行重算或事件。CP02-007、BP16-049、BP05-001 僅為動作片段種子，
不代表完整成本、能力、lowering 或 executor 已驗收。

MoveSpec 與 search Partition 使用內部 `MoveDestination`：`Owners(Hand / Deck /
Cemetery / Banish)` 逐物件解析該物件持有者；`Shared(ZoneRef)` 僅接受 Field／Ex。
混合持有者的集合不能用來源控制者或來源持有者的一個 ZoneRef 替代。Destroy／Banish
保留專用動作，目的區由各自規則逐物件決定。一般 Move 的 position 只接受 Owners(Deck)，
initial_posture 只接受 Shared(Field)；Mill 目的地必須為 Owners(Cemetery)。
Placement.zone 同樣使用 MoveDestination。Order 先接受一個 Placement：cards 與
Placement.cards 精確相等、Owners(Deck) 及明示 position；在指示開始固定集合，回答必須
是其完整排列，不重新選子集。MoveSpec.order 只在 Owners(Deck) 且有 position 時接受：
Required ResolutionChoice、外層 exact 1、無 guard／capture，Order domain 精確等於
Move.objects，actor 等於該指示 actor；回答僅在該動作內可見。
Order action 的 chooser 也須等於指示 actor；兩者都檢查決定可見性與物件證據，
禁止選擇的 batch／stage 仍拒絕。空集合可以承載，實際空排列的省略依規則程序，不能
用空集合推論牌庫沒有搜尋候選。
Order 明示 `decision: DecisionId` 並共用唯一性檢查；`OrderAnswer::matches_domain`
只驗完整排列與世代，不授權 actor／epoch。協定與 runtime 准入依執行語義 §4。
單一排列跨多個持有者時，先保存一次完整回答，再依各持有者保留相對順序放入各自牌庫；
回答順序以牌庫頂至底方向解讀，Bottom 的第一項在放入區塊最上方。
同區重排不變世代、不造移區事件；跨區才記 MoveResult 與 after。
lowering 的正規形以 Action::Order 表示已在目的牌庫內的重排（例如 Look 餘牌），
跨區則使用 Move＋order；良構檢查不推導物件的執行期所在區域，executor 仍須驗證。
跨 owner 的單 chooser 排列是內部契約約定，沒有以本片卡例證明其官方適用範圍。
runtime 依 CR 4.1.5 預設由目的區所屬玩家定序；缺卡文明示單 chooser 權限時拒絕
跨 owner 排列。Shared(Field／Ex) 的逐物件規則解析與成功 Before 定義依執行語義 §3。
多 Placement、獨立／compound 的 Order decision 及 Move／Token entry_effects 尚未接受。

Search 的選牌點明示 `decision: DecisionId`；每個 SearchPartition 及
OptionalSelfReplacement 也各有必填 decision，與一般 Decision、Order、Stepwise 共用
全樹唯一性檢查，包含未執行分支。Single 不新增第二次選牌，groups 仍是同一次回答；
候選／continuation 的型別與 scope 隔離不因新增 ID 改變。
`SearchSelectionAnswer` 以 Ungrouped／Grouped 區分回答，後者的 Vec 按 groups 印刷順序
完整保留空組；`matches_domain` 只檢查形狀、成員世代與互斥，不驗基數或 authority。
`SearchPartitionAnswer::matches_remainder` 檢查剩餘 found 子集與截限後的當段基數；
`SearchReplacementAnswer` 僅 Original／Replacement，不帶第二次 cards。
基數的搜尋特例、各題時點、token／epoch／snapshot、公開證據與拒絕條件依
[執行語義 §6](runtime-semantics.md#6-search-的輸入點回答與觀察)。
Public Order 及 Repeat 的 executor 准入仍分別依該文件 §7、§8，不能由良構推定支援。

Search 的 filter 與替代目的地 predicate 依搜尋者的私有選擇域檢查可見性；
不能藉可選牌集合或替代選項讀取對手手牌、原始牌庫或他人的私有 Look 結果。
candidate 只在 filter 可見，continuation 的 found／revealed 在另一個獨立 scope
定義，只供 destination 使用；candidate、兩個 continuation binding 均不流到後續本文。
內部 SearchFoundCount／SearchRevealedCount 提供這段的計數。SearchFoundAny 在
found 成員上逐一綁定局部 object，只供自己的 predicate；空 found 為 false。
BP03-002 的替代目的地條件可讀這個物件的 cost，不能以 found 計數替代。
object、found 與 revealed 均不得流出 continuation；found 不能冒充 revealed 或
successful moved after，也不新增第二次選牌。SearchFound／SearchRevealed selector 僅在
continuation 內投影移動前物件；前者為搜尋者私有，後者為公開證明，兩種 binding
不能互換。計數與 SearchFoundAny 同樣遵守 audience。Search destination 中的
MoveSpec.order 與 position 尚未接受：末尾洗牌會抵銷同牌庫的放置順序；若要承載
洗牌後放置，須另定時序與 found 排列域，不能以 revealed 代替實際要搬的 found。
continuation 仍是 destination 的封閉區段，沒有任意 Program／shuffle callback。
Single、非空 Partition 與 OptionalSelfReplacement 的每個欄位／分支都檢查；Partition
保留順序與各段基數，不要求各段 minimum 的總和不超過 found，因為後段可能無牌可做。
沒有 destination 的形狀尚未定義，回報 UnsupportedShape；全部 partition 有限時，
其 maximum 總和須覆蓋搜尋 maximum，無上限搜尋也不能配全有限 partition。
groups 接受一個 SelectionConstraint，總基數須等於 Search.cardinality、群組非空且
Disjoint；各群組是指定 deck 的 Current Zone selector，predicate、基數與 aggregate bounds
逐欄驗證，保留 distinct_by。群組域與全域 filter 取交集，所有群組共用同一次搜尋回答，
不讓每組重新搜尋；重疊候選由回答分組且同物件最多一次。群組根節點的 Deck 只授權
該組候選，巢狀 predicate 或 aggregate 不得讀整個牌庫／對手手牌。其他群組 selector
暫不接受。禁止決定的區段內 search 仍拒絕。HasTrait 與 NamesIntersect 的 Current 物件
讀值支援 CP04-060 的 filter；未驗快照形狀仍拒絕。

Move／Search／Destroy／Banish 各自產生對應 outcome，不能互換 receipt 型別。
MoveAfter／MoveAfterAll 需要支配它的同型 producer／repeat collect，並明示
SameEffectExplicitReference；TriggerExplicitReference 與 RuleCarry 尚未接受。
ObjectReceipt::Movement 僅接受 Before 成功成員投影，供原集合扣除實際移動成員；
After 必須用 MoveAfter，不能省略跨世代引用權限。BP18-059 的 remainder 是觀察集合
減去成功移動的 before 成員；不能減去所有 chosen，否則 EX 滿而未移動的牌會遺失。
Move 與 Token 可共用 SharedCapacityAtomic 批次，成員間仍不能讀 sibling 結果。
Move／Destroy／Banish 的成功 Before 成員可保留原輸入已知的 audience，供 remainder
選擇使用；audience 必須在首次檢查 selector、宣告 candidate 之前推導，
成功完成指示後才發布，batch 在所有成員完成後合併，sibling 仍不可讀。
目前僅本地 receipt，不擴及 repeat iteration。這只證明哪些已知舊世代成員
成功移動，不授權其目的地、新世代或隱藏身分。MoveAfter／MoveAfterAll 不自動成為
Public 選擇證據：取代可能把牌移到非公開區域。FlipDown 仍單調撤銷物件證據。

InstallEffect 接受 InstructionStart、Objects 的獨立 affected binding，以及：

- CostModifier 對同一 binding 的 Delta／Set；負常值 Set 尚未接受，動態數值的合法範圍
  仍由 executor 檢查。獨立的 CostModifier.usage 仍拒絕，使用額度放在 Duration.usage。
- Continuous 的 Numeric 層與 Numeric／非空 Combined patch。target 必須是精確引用
  affected 的 Dynamic selector；executor 必須在安裝時固定對象，不得重新掃描區域。
  每個 patch、condition 都檢查，即使 condition 為 false。EffectDefinitionId 全樹唯一。
  source 必須與安裝指示相同，封閉 Program 也檢查這個等同性。
- Permission 的 Objects subject 與 Destroy／Banish action 都精確引用 affected，
  polarity 必須是 Forbid；cause 與 condition 遞迴檢查。這是既有保護述詞的安裝形狀，
  不將其他 ActionPattern 或 Allow／Require 一併放行；InstallEffect 不發布可見性證明，
  executor 也不得從 Banish 推定物件公開。

內外 Duration 必須一致，affected 只在此安裝內可見，不能在另一個指示沿用。
Live 數值在求值時讀取，卡文要求鎖定時 lowering 必須使用 capture。
本文 Continuous 的 active_zones 必須為空，表達不隨來源能力失效而移除的設計要求；
靜態 Continuous 仍要求 active_zones 等於能力有效區，兩者不可互換。
本文的 timestamp 必須為 ProperPlay，不能使用 ZoneEntry；靜態宣告仍使用 ZoneEntry。
ProperPlay(Current) 指向本次解算的適正打出事實；Bound execution 只做下文的引用檢查。
期限明寫 SourceZoneExit 時，才額外錨定來源物件與區域；NoTemporalExpiry 不會取消
目標世代的失效規則。

安裝期限檢查所有 end 的 player／predicate／source，以及每個
DuringNextPhase／DuringNextTurn interval。anchor 為 Installation 或下文的 Bound。
Duration.usage 只在 CostModifier 安裝接受 PerObject 或
SharedPlayer(player, CurrentInstallation)：IR §14 只定義 cost_mod.next_only 的消耗，
Continuous／Permission 帶 usage 一律拒絕，不替數值修正或保護效果訂消耗時點。
checker 原樣保留 UseLimit 的 Nat(0)；executor 必須將它視為已耗盡的額度，
不能視為無限制；無限制須使用 None。
TimeAnchor、ProperPlay execution、SharedPlayer effect 的 Bound 形狀均檢查
binding 的支配、scope 與型別；目前沒有產生 AnchorId／ExecutionId／EffectId
binding 的已驗入口，不能憑空構造有效的外部實例引用。
期限在安裝輸入環境檢查，不能讀稍後建立的 affected binding；condition 則可以讀。
安裝時玩家捕捉、嚴格晚於錨點的 occurrence、複合區間求值、適正 play 消耗／rollback、
來源與目標的獨立生命週期仍待 executor 驗收，這裡只驗結構。
NextTurnEnd 以「下一回合的開始」找 occurrence，NextTurnEdge 以指定 edge 找 occurrence；
兩者在指定玩家的目前回合可能不同。數值 Set、Combined 的 runtime 限制及具體期限
依[數值修正、排序與期限](numeric-timing.md)，不由本 checker 推定。

CP02-007 的本文 buff 依撰寫語法降為 I.modify＋L.continuous；Action::Modify 的結構
已由本文件前節接受，runtime 的固定 Add 子集見[內部執行](internal-execution.md)。
本片只借它的 Numeric patch、NoTemporalExpiry 與單一目標形狀作合成安裝例，不是該卡的
lowering，也未包含完整成本、另一能力及實際執行。BP02-089 只提供 Destroy 保護述詞
（能力造成的破壞），該卡仍可被消滅；Banish 是同形的合成例。兩者都不宣稱該卡本身會
執行 InstallEffect。Grant／Delay、Granted 來源實例、Ability／NonNumeric 安裝、
Players 安裝、AtPlay 安裝、Replacement／RuleModifier／DeckRule 仍拒絕。BP02-041 的
指示自我取代仍未承載，不能用靜態改道、Put 或 If 替代。

CP04-060 的動作種子使用 search 的成功 moved after 安裝本回合 delta -2；found／revealed
只在 continuation 投影移動前物件，SearchFoundAny 也不授予移動後引用；不能假冒 moved。checker 不由卡號反推卡文：合法的 Set 或另一個已定義
選擇集合仍可能是別張卡需要的程式，lowering 必須獨立驗證與 CP04-060 原文相符。

本節的 CP04-060、BP18-059、BP13-SL16 是動作片段的結構種子，非完整卡片執行驗收。
Search 仍須由 executor 證明條件、先找到才問替代目的地、拒絕時回原手牌、末尾只洗一次；
不得依真實牌庫先告知無候選。EX 滿時保留未移動 remainder、跨 Move／Token 的容量分配
（Q1776）、after 的新世代與入場事件也須實際局面測試。BP18 種子已含私有的至多一張
武闘竜人選擇與 −3 減費；完整 trigger／UB、新世代 entry_effects 尚未由上述片段涵蓋。
任意順序另以 Look→選取→Move→成功 Before remainder→Order／MoveSpec.order 的合成片段
驗證良構，不把此片段當作整卡或實際排列驗收。

MoveResult 保存 requested_position／actual_position，與 requested_to／actual_to／
replacements 一起保留提案與結果；checker 沒有建構或驗證執行結果，不把這些欄位當成
LeavesZone／RedirectMove 或每事件一次取代已實作的證據。

## 事件投影、歷史與取代

typed payload binding 表示原事件的 payload 與 envelope，header projection 必須明示
EventContext。Trigger 僅在真正的 trigger 入口可用；封閉 Program 不得憑空使用。
typed EventContext 依原 binding 的 scope 與型別檢查，不能把 LeaveField slot 轉成
Damage，或讀其他能力、未來節點的事件。header／payload 的相等比較保留原輸入，
checker 不重建或猜測「最近一個事件」。

AnyOf 每個分支只在自己的 predicate 內窄化，所有分支皆驗證，不因常值條件略過。
分支 payload 不隱含 join 到其他分支或本文；本文仍可讀 Trigger envelope。
AnyMove、AnySnapshot、AnyStatFacet 的元素與 EarthRitePayment component 只供各自
predicate，不能在其後讀取；binding slot 仍全樹唯一。事件欄位、快照及歷史讀值
目前不授予選擇域可見性，須待 runtime observation 證據，不以公開事件名稱推定全部
payload 公開。

PlayerRef::FromExpression 只接受常值、既有 binding 與凍結事件欄位；Live 角色讀值
（例如來源目前的控制者）會在同一次結算中隨控制權移轉改變，不能當 actor、viewer
或可見性對象，否則 Look 的私有結果可經同一運算式轉給新控制者。PlayerEquals／
PlayerIn 的比較運算元仍可讀 Live。ObjectEquals 的 Bound 物件與 ObjectOperand::Bound
同規則：選擇域內只有本域候選豁免，其餘須有物件可見性，翻面撤銷後不再可比較。

FollowEventMove 僅接受當前 trigger 明示綁定的 Move／LeaveField，型別表達的是沿
header receipt 查找 triggering subject 的 before 成員，取得該單一成員成功移動後
的引用。其他 binding、歷史事件與 union 外的分支 payload 均不授權。整個
TriggerEvent receipt、MoveAfter 的 TriggerExplicitReference 與 RuleCarry 仍拒絕；
不能用整份 receipt 的所有 moved 成員冒充觸發主體。本文的 FromExpression 物件動作
仍拒絕，Frozen MoveAfter 欄位只可作事件資訊比較，不能繞過 follow 權限執行動作。
實際配對 before／after、失敗或無 member 的結果及卡文明示引用，仍由 executor／lowering 驗收。

SnapshotCounter 只讀 RuntimeObjectView 指定的快照與 counter kind，沒有「找不到就
改讀 Current」的退回。BP07-088 種子在 AtEvent 捕捉 LeaveFieldLki 的祈禱 counter；
一般 Live Counter 在該捕捉時點仍拒絕。凍結的 Names／Kind／Traits 可讀快照或事件
characteristics；不把離場後同 ID 的新世代當作材料。

HistoryExists、HistoryCount 與 HistorySum 檢查 window、cutoff、include_execution
及局部事件 predicate；查詢不把自己的事件 binding 匯出，也不授予 trigger follow。
Since 與 Capture cutoff 必須有正確型別及 capture 類別的既有 binding。
Facts／Subjects、數值加總與 OriginFactId 去重的實際結果仍由歷史執行器驗證。

CausePattern::Effect 與 AnyRule 是不同變體；EffectSource::Matching 的 binding 只在
該 Effect cause 的 source predicate 可見，其他 cause 沒有 source candidate。
Permission declaration 接受 Destroy／Banish 的對象、cause 與 condition 遞迴檢查，
保留 polarity、rule 與 duration。BP02-089 的禁止保留 Effect cause，不泛化成所有
規則破壞。checker 不依卡號檢查卡文等價性；AnyRule 是其他規則可能需要的合法輸入，
lowering 必須另外驗證 BP02-089 使用 Effect。

Replacement declaration 的子集是 General、LeavesZone 的 SourceHolder，離開區須
在來源能力有效區內，rewrite 為 RedirectMove。目的地共用 MoveDestination 檢查，
position 僅適用 Owners(Deck)；合成種子保留 Top。condition 與 affected_player
逐欄檢查；OncePerEvent 僅能構造 Enforced。SelfOnly 表示效果對自身指示的自我取代，
不等於「對象是 SourceHolder」，靜態宣告的 SelfOnly 一律拒絕。BP02-041 是スペル
自身返回手牌指示在條件成立時改置牌庫頂的自我取代，尚未承載；不得降為靜態離區
改道、一般 Put 或先執行再補的 If。usage、prospective_chars、其他 pattern 及
其他 rewrite 尚未接受。原提案不可覆寫，實際目的地／位置與 replacements 必須並存；
多個取代的順序、每事件每個取代至多一次，以及被取代原事件不得偽造，仍須 executor
局面測試，不能把本節的結構通過當作取代已執行。

## 診斷

失敗回傳第一個 `WellFormedError`：`stage = well-formed`、具型別的 reason、
最近 `Located` 節點的來源座標與內部 IR path。來源座標由呼叫者提供，沿用
`SourceSpan` 的公開座標契約；`Display` 只輸出 stage、reason 與程式內路徑，
不輸出卡文、表達式值或來源欄位內容。
選擇域目前先檢查可見性再檢查引用；缺少 binding／receipt 證據時，可能先回報
`UnsupportedShape`，不保證一定是 `UnavailableBinding`／`UnavailableReceipt`。

## 能力與程序入口

`WellFormedCardProgram::check` 持有整棵 CardProgram，依序檢查所有能力、未選模式、
trigger、declarations 以及已接受的授予能力。只提供不可變 `card()`，不把依賴入口 binding
的子 body 包裝成封閉 `WellFormedProgram`。能力、effect 定義 ID 與 scope 全樹唯一；
各能力的環境互相隔離，不能從其他能力讀 binding／receipt。

`card_play_costs` 也完整遍歷來源座標；每 face 至多一筆非空 `FacePlayCost`，
每筆至多一個 set 宣告，連恆假分支也計入。delta 必須是非正常值，set 必須是非負常值。
條件在獨立入口檢查，不繼承任何能力的 binding、target、capture 或 receipt；
卡表 face、來源閉包與純公開條件的執行白名單由整卡 loader 另驗。
承載及拒絕規則見[打出費用契約 §4](play-cost-semantics.md#4-最小內部承載與-lowering)。

Ability 明示 scope。Spell／Activated 需要 play 與 body，Triggered 需要 trigger 與 body，
Static 不得有 play 或 body；非 Static 不接受 declarations。UnionBurst 尚未驗證。
本文 scope 必須等於能力 scope；指示與 declaration 的 AbilitySource 須與所屬能力相同。
通用 checker 只驗有效區非空且不重複；正式 Printed Spell 的 `[Resolution]`、Hand／EX
起點與卡表類型對映須由[正式 play 准入](play-semantics.md#1-有效區打出起點與來源)另驗，
現有通用測試中的 Spell／Field 包裝不構成正式 header 範例。
入口的可見性順序為：

| 入口 | 可讀內容與限制 |
| --- | --- |
| trigger pattern | 定義在能力 scope 的事件 binding；不能讀之後的 capture、打出回答或本文結果 |
| event_captures | AtEvent，event=Trigger 且 policy 與 trigger.view 相同；同批 capture 互不可讀 |
| play.captures | AtPlay，execution=Current；自身移區／付款前，尚無 targets／X 回答，同批 capture 互不可讀 |
| ChoiceKeyword 的模式數與選擇 | 讀 play.captures 後、PlaySpec.parameters 前的環境；不得讀稍後的目標或本文結果 |
| PlaySpec.parameters | 按序公開回答；每次 Decision.captures 只接受指向該 decision 的 AtSelection，可讀該次回答 |
| 模式 parameters | 只對自己被選模式的 body 可見，不能互讀 sibling 模式 |
| body | 讀已完成的入口與參數；AtResolution 前綴以共同入口凍結，不讀本文造成的結果 |

AtPlay、AtSelection、AtEvent 只能出現在上述明確欄位，不能以普通 `Node::Capture` 偽裝
時點。capture 目前仍限 Int／Nat／Bool／Selector；其他型別明確拒絕。
AtEvent 採較窄範圍：拒絕 Live、Current 物件讀值及全部集合讀值，只接受常值、
已可見值與明確匹配 event／policy 的 ObjectView::EventSnapshot（及其純運算）。
不把一般 State 讀值隱含改解釋為事件快照；BP07-088 改用上節的 Frozen SnapshotCounter，
不透過普通 Holder Counter 讀目前狀態。

PlayParameter 目前接受 Public 的 Targets→Objects、X→Integer。Targets 排除非公開領域
（CR10.6.2.3）；Public 是本片的保守承載限制。ExecutionRef 僅 Current，
SelectionSnapshot 為 AtDecision；Inline 的 actor 必須等於 play controller，Handoff 只
接受相對該 controller、基數恰好 1..1 的 RequireOne(Opponents)，不以不相等的符號猜測異玩家。
同次 play 的 targets／Distribution 之 AtDecision 共用選擇狀態版本；AtSelection 只增加
已回答的不可變 binding，不改讀付款後 live 狀態，詳見[快照契約](play-semantics.md#3-同次-play-的快照與-binding)。
X 影響模式數或任意追加費用的形狀尚未接受（CR10.6.2.2.1），不能外推模式一律先於 X。
Modes 由 ModeDecision 承載；Distribution 與費用承載見上節。獨立 CostMaterials、
OptionalCost 仍未接受；Face 僅接受下述限定進化入口，不能據此宣稱作者語法禁止其他形狀。

### 單面進化與自身入場

BP06-110 的整卡種子包含兩個能力。Fanfare keyword 入口限自身 EnterField、
AfterEntry、active_zones=[Field]；述詞固定為
`ObjectEquals(Frozen EnterFieldObject(event), Frozen SourceHolder)`，event 必須是該 pattern
自己的事件 binding。非 keyword 的 EnterField trigger 也可使用；一般 ObjectEquals
遞迴檢查 typed Frozen／Bound 投影，事件 binding 支配的 AtEvent capture 與本文條件
也可比較，不以特例跳過欄位檢查。Fanfare 入口仍要求上述精確自身入場形狀。

進化入口只接受 Printed 的 Evolution keyword、Activated、SourceHolder、
Controller 與 active_zones=[Field]，採下列封閉形狀：

1. 唯一 Face PlayParameter：AtDecision、Current、Inline、Only(Controller)，沒有 capture。
   Objects 域恰選一張自己的 Evolve 卡，唯一群組也為 exact 1；沒有 roles、distinct_by 或 aggregate bounds。
   述詞為目前候選卡名稱與 SourceHolder 名稱的 NamesIntersect，不接受未過濾的整個進化牌組。
2. 唯一 Ordered 費用：先 Reveal 同一選取 binding，再 Pay 非負常值的 PpCurrent。
   兩步都是 Controller 的 Cost 指示，無 guard、無材料再選；不接受 EP Alternative。
3. 本文用 EachObject 對該 exact-one 集合取元素，唯一指示為 Evolve：
   subject=SourceHolder、form.object=該元素、固定 FaceId、mode=Normal，產生新的 link binding。
   不接受 Super／Effect 或 Face 回答的其他用途。

EachObject 是既有型別內的暫時單元素投影，會多出該 capability 需求；它不是卡文的逐物件效果。
未來 exact-one 型別投影應取代它。此處不驗資料庫中的面數或 FaceId：
lowering／准入必須證明選取域只含單面進化牌，固定 FaceId 是該面的識別；雙面卡不能套用此形狀。

Normal 在此入口表示印刷進化的基底程序，不是「永遠禁止超進化」：
未來規則執行器須在整個打出交易中，依序適用替代／固定值／增減，再提供 EP 代付
（CR12.2.3）與合法的 SEP 超進化選項（CR12.2.4）。選擇超進化時，執行計畫須升級為 Super，
支付 SEP、增加攻體並產生超進化事件，不能照 Normal 動作執行卻漏掉這些效果。
這些選項屬規則，不能抄成每張卡的印刷 Alternative。
尚未提供這個程序的 executor 不得因本結構通過就接受進化執行。
每回合額度（CR8.3.2.1）、進化卡上的進化能力不得打出（12.2.5）、形態合法性、
同世代關聯（5.16）及雙面選面（4.6.4）同樣須在規則／准入層驗證。

### 其他入口

Trigger 暫接受 PerFact、無 limit。EnterField／EnterEx、Damage／StatChange／LifeChange／
CounterChange 使用 AfterEntry；LeaveField／Discard／Destroy／Move／Payment／
EarthRitePayment／Fused 使用 BeforeExit。這是本 checker 的快照 policy 子集，
不是宣稱每種事件都由移動或入場產生。AnyOf 每支須與同一 trigger.view 相容，
各 pattern 述詞遞迴檢查。State、Banish（可能裏向）、其他事件形狀及其他 policy
尚未接受。Continuous declaration 接受
Numeric 與 Combined patch，以及遞迴檢查的 GrantAbility Definition；Keyword grant、
上節以外的 effect、layer 不一致、未驗 duration／usage／timestamp 一律拒絕。
Static declaration 的 active_zones 必須與能力完全相同，duration 僅 NoTemporalExpiry，
anchor 為 Installation、無 usage／intervals；條件走 condition，時間性期限留給 effect 安裝。
GrantAbility Definition 在這裡只驗來源仍為 Printed／Keyword 的定義模板；
授予實例的 Granted 來源、provider 與持有者重綁尚未驗證，不能直接拿模板作執行實例。
這是保守的承載範圍，不是事件或持續效果的完整執行語義。

`WellFormedProcedure` 只驗明確程序的輸入描述，不帶任意本文。Rule 的 kind／procedure／rule
須與入口一致；目前只接受 CR7.4.3 的結束階段 GuardAct，固定 domain 為 EndPhaseGuardAct：
回合玩家場上有守護的從者、任意張數、Public、actor=Active。不能自行提供 selector 放寬範圍。
Capacity 的候選是本次移動／建立集合，與入場時守護選擇及其他程序都尚未接受。Quick 的 window 須一致，
actor 固定為相對 Active 的 RequireOne(Opponents)，基數 1..1（CR7.4.5、8.4.7）；
domain 必須為該 actor 的 QuickActionOrPass，
可見性只給該玩家；不依手牌內容刪除窗口。這些入口不是規則執行器，也不能嵌入卡片本文。
Quick 的許可正規形、合法動作及續體依 [Quick 契約 §4–5](quick-semantics.md#4-合法動作可見性與答題)；
通用 Permission 的動作只收 Destroy／Banish；卡片 Quick 的 `PlayCard` 許可僅接受
該節完整 header／唯一 declaration 的精確例外，良構通過也不等於已驗執行。

## May 與目標角色

`K.may.choice` 必須為無 precondition 的 MayDecline；Execute 的完整 domain 與 captures
都只供其 body 使用，離開後僅留下不透明的 May receipt。Decline 不會建立本文 receipt。
Execute body 不得再出現選擇，包括獨立 decision、Chosen／DownTo discard、declare、
UpToStepwise、optional pay、mode 與巢狀 May；有選取的 payload 必須併入同一次回答。
沒有選取的 may 使用空 compound，不能拿 Integer 0..1 代替 Execute／Decline。
Bound 手牌材料可沿同次回答傳遞可見性，但仍須當時手牌相交證明；
BP19-072 的實際付款、捨棄取代及成功判斷並非完整卡片執行驗收。

roles 只接受 PlayParameter Targets。每個 role ID 與 binding 唯一，由同次回答產生；
目前接受一個 IfNonempty(1) 與至多一個 Remainder。若無 remainder，目標上限不得大於 1；
Fixed、多個 IfNonempty、空 roles、重複 remainder 明確拒絕。BP21-PR13 是合成種子。
`TargetRoleSpec::accepts_answer` 對已驗目標集合檢查角色基數、同世代 membership、互斥與
完整覆蓋；目標空集合時 first/rest 都是空集合。它不驗選取域或目標在結算時是否仍合法，
也不會新增另一個 DecisionId。

## 模式與逐張回答

ChoiceKeyword 只可放在有 play 入口的能力 body 頂層或 Sequence；If、Repeat、EachPlayer、
EachObject、May、pay on_paid 與其他模式分支內不接受。ResolutionAlternative 是結算時
exact 1。所有模式 ID 依印刷順序與 domain 完全一致，不能重複；exclude_used 尚未接受。

Mode.count 是上限的權威。常值 count 先依 CR5.18.2.3 clamp 到模式數，domain 基數須一致；
可為 exact、1..上限；只有上限涵蓋全部模式時才接受 0..上限，承載「好きな数」，
不能把一般「まで」降成可選零個（CR5.18.2.1）。條件 count 僅接受 Nat Conditional 的常值葉子，
條件以常值與 AtPlay／Current 的 SC、NC 比較（含 And／Or／Not）；domain 固定 min=1、
max=None，動態上限只由 count 產生。IR 目前無法表達動態 exact count，
lowering 必須拒絕此類原文，不能誤降成「まで」而期待 checker 辨認。SC／NC 的 AtPlay 讀值
只能用於模式數，不能出現在一般本文或 capture。BP02-045／BP02-088 是種子；通用 instead
仍無 typed 節點。`ModeDecision::accepts_answer` 驗已求值的 count、模式 ID 與基數，count=0
不執行，不能從此推定所選模式的付款或目標在 runtime 合法。

Draw／Look 的 UpToStepwise 明示 decision ID、actor、visibility 與 timing。ID 全樹唯一，
actor 為抽牌者／觀看者，visibility 為 Only 該玩家，choice-free context 拒絕。
`StepwiseSession` 是純逐張協定：Decide→Continue→Perform→Decide，Stop 或到上限才結束；
每次 performed 計一次嘗試，不能用成功數代替。依 CR5.10.3.1、5.11.2.2，兩者都可在
第一張前停止；look 後續回答可利用已授權觀察，不能提前一次選好張數。look 觀察的語義
由 Action::Look 與 AfterLook timing 保留，不代表第一張必看。

此協定不保存牌庫或卡片身分，不負責卡文降低、實際抽空敗北、觀察授權與 receipt；
draw 不因呼叫端知道牌庫空而跳過選擇（CR5.10.1.1、11.2.2）。look 缺牌仍按有限次
嘗試處理是專案的保守結構解讀（inferred），不宣稱 CR 禁止略過空位；禁止抽牌與缺牌的
executor performed 語義須另驗。完整 executor 必須在 Perform 處完成單張操作及資訊
投影，再開下一個回答。合成測試以 BP01-007／BP01-065、BP07-SL09 的動作為種子；up-to
本身是 CR5.10.3／5.11.2 的 rule-only 形狀，不冒稱這些卡印有 up-to。
