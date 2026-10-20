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
  BP13-043 與 BP06-110 的合成種子只驗證這項交接，不宣稱完整卡片可以執行。
- simultaneous 的 choice stages 先於成員提交；同 stage 的 decision 讀共同入口，
  後一 stage 可讀前一 stage 的公開不可變 Nat 回答。成員共用完成 choices 後的環境，不能讀 sibling
  的結果。choice bindings 只在批次內可見，成員輸出與 receipt 完成整批檢查後公開。
  空批次拒絕；批次 choice stage 的帶前提 decision 尚未接受。
  批次成員與 NonChoiceSimultaneous player stage 不能藏入數值 declare，
  也不能藏入 draw／look 的 UpToStepwise（逐張決定）；
  each-object 的 Chosen 順序也尚未接受。
- repeat 的 export 必須指向自己 body 結束時可見的 local receipt，並符合 producer
  outcome 型別；不能重新匯出外層 receipt 或本輪 binding。iteration 只在
  AfterIteration 的終止條件可見；Count 與 BeforeIteration 不能使用它。
  collect 只在迴圈後可見，並保留 `ReceiptList<T>` 的元素型別。
- instruction precondition 的 `protects` 必須是該 instruction，述詞在其輸出與
  receipt 公開前檢查。一般 body 不接受 Cost instruction。

## 接受的形狀

接受 sequence、if、simultaneous、直接 body 的 repeat、each-object、each-player
與下述結算付款、May 和模式；instruction 接受 draw、look、counter、reveal、flip-down、posture、
初次擲骰、數值 declare 與下述 discard。UpToStepwise 的決定描述及逐張協定見下節。
capture 只接受 Int、Nat、Bool、
Selector 的 `AtResolution`，且必須位於 root body 開頭連續的 capture 前綴：
AtResolution 在進入執行時一次凍結，之後的節點會看到本體自己造成的變更，
巢狀 sequence／if 也不是新的解算。前綴各 capture 同時凍結，輸入只能用入口
可見的值（能力入口包含已選參數），不能引用同前綴的其他 capture。本文其他位置回報 `UnsupportedShape`，
scope 不是 root 回報 `WrongScope`。

值檢查涵蓋這些形狀使用的 signed arithmetic、明示 Nat 轉換、條件值、部分 Live
讀值、數量 receipt／collect 投影、集合及局部 candidate。支援的式子依各運算元
遞迴檢查，不以整個型別家族推定所有變體都通過。
有號數不會先截成零；Add、Subtract、Multiply、FloorDivide 要求兩個運算元，
HalfUp 要求一個，Minimum／Maximum 與作者 `min`／`max` 相同，至少一個。
FloorDivide 只接受可靜態確認的正整數常值除數，其他動態除數明確拒絕。
InformationOrZero 接受 Cost／Stats 的 Original／Effective 特徵讀值；LastKnown 與
ProspectiveEntry 尚未接受，不能用一般讀值替代 LKI 或入場取代情境。

其餘形狀一律拒絕，包含分配傷害、移動／search／replacement、
未驗的事件投影、effect 安裝及下述範圍以外的能力宣告。未承載的 decision／付款時點回報 `DecisionTiming`，
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
receipt；不能越過其他節點、跨 stage 或外洩回答。BP13-043 獨立指定的選擇不應掛上
後續交付的前提。

discard 接受 Chosen、Random、All、DownTo。Chosen 群組必須 disjoint；群組、population、
eligible 透過目前手牌的 zone、filter、difference、union 或有手牌限制的 intersection
證明子集。無法靜態證明的 selector 拒絕；不靠虛構物件列舉驗證動態集合。
目前只接受 actor 就是手牌持有者。選擇域的公開 zone 白名單為 Field、Ex、Cemetery、
Resolution、Race、Drive、Trigger、Equipment；Hand 只容許持有者私有可見性，包含
述詞內巢狀集合讀取。Deck、Evolve、Banish 一律拒絕；Evolve 的私有牌組與 Banish
的裏向物件需另有可見性證據，不能把整區當公開。

選擇域拒絕未驗可見性的 bound／capture／receipt 集合來源；bound 物件也只接受本次
選擇域述詞內建立的 candidate，不能從外層 each-object 帶入未知可見性的物件。
Int／Nat／Bool 的 capture 與未知來源 bound 亦拒絕；唯有先前 Public Integer decision
產生的不可變 Nat 回答可供後續選擇使用。此限制包含 aggregate bounds、群組述詞和
Random discard 的 count；例如私有或 declare 來源的 X 張隨機捨棄目前會被拒。
這些限制不取代 executor 的 Observation 投影與動態合法性驗證。

`K.pay` 目前只接受 ResolutionChoice 的非空 Ordered costs。每個 step 必須是 Cost，
已驗成本形狀為 discard、移除 counter 與 act；其他成本及 alternatives 尚未接受。
Optional 的「付不付」與成本內 discard 的材料選取都歸這個 K.pay 程序所有，
materials 只建立同一次選取的結果 binding；這不是 K.may，也不經獨立 DecisionId
或額外的 `Node::Decision`。獨立 MayDecline 仍未接受。materials 與成本 receipt 僅對後續成本及 on_paid 可見。
PaymentResult 在成本檢查後才對 on_paid 與付款節點之後公開，不能從成本讀尚未完成的
付款結果。付款完成、物理實際量與選取結果仍是不同型別。

repeat 可以匯出 PaymentResult，終止式可讀本輪 complete，迴圈後可以讀 collected
payments 的 All／Any complete。相同本輪 receipt 可匯出到不同 iteration bindings；
每個 binding 仍獨立檢查 scope 與 producer 型別。這些都是結構檢查，完整付款、原子
消耗與重播中回答一次等執行語意仍須 executor 驗證。

## 診斷

失敗回傳第一個 `WellFormedError`：`stage = well-formed`、具型別的 reason、
最近 `Located` 節點的來源座標與內部 IR path。來源座標由呼叫者提供，沿用
`SourceSpan` 的公開座標契約；`Display` 只輸出 stage、reason 與程式內路徑，
不輸出卡文、表達式值或來源欄位內容。

## 能力與程序入口

`WellFormedCardProgram::check` 持有整棵 CardProgram，依序檢查所有能力、未選模式、
trigger、declarations 以及已接受的授予能力。只提供不可變 `card()`，不把依賴入口 binding
的子 body 包裝成封閉 `WellFormedProgram`。能力、effect 定義 ID 與 scope 全樹唯一；
各能力的環境互相隔離，不能從其他能力讀 binding／receipt。

Ability 明示 scope。Spell／Activated 需要 play 與 body，Triggered 需要 trigger 與 body，
Static 不得有 play 或 body；非 Static 不接受 declarations。UnionBurst 尚未驗證。
本文 scope 必須等於能力 scope；指示與 declaration 的 AbilitySource 須與所屬能力相同。
入口的可見性順序為：

| 入口 | 可讀內容與限制 |
| --- | --- |
| trigger pattern | 定義在能力 scope 的事件 binding；不能讀之後的 capture、打出回答或本文結果 |
| event_captures | AtEvent，event=Trigger 且 policy 與 trigger.view 相同；同批 capture 互不可讀 |
| play.captures | AtPlay，execution=Current；尚無 targets／X 回答，同批 capture 互不可讀 |
| ChoiceKeyword 的模式數與選擇 | 讀 play.captures 後、PlaySpec.parameters 前的環境；不得讀稍後的目標或本文結果 |
| PlaySpec.parameters | 按序公開回答；每次 Decision.captures 只接受指向該 decision 的 AtSelection，可讀該次回答 |
| 模式 parameters | 只對自己被選模式的 body 可見，不能互讀 sibling 模式 |
| body | 讀已完成的入口與參數；AtResolution 前綴以共同入口凍結，不讀本文造成的結果 |

AtPlay、AtSelection、AtEvent 只能出現在上述明確欄位，不能以普通 `Node::Capture` 偽裝
時點。capture 目前仍限 Int／Nat／Bool／Selector；其他型別明確拒絕。
AtEvent 採較窄範圍：拒絕 Live、Current 物件讀值及全部集合讀值，只接受常值、
已可見值與明確匹配 event／policy 的 ObjectView::EventSnapshot（及其純運算）。
不把一般 State 讀值隱含改解釋為事件快照。Holder 尚無 snapshot counter 讀法，
因此 BP07-088 的離場前祈禱 counter 仍拒絕，等待 LKI／事件欄位讀值。

PlayParameter 目前接受 Public 的 Targets→Objects、X→Integer。Targets 排除非公開領域
（CR10.6.2.3）；Public 是本片的保守承載限制。ExecutionRef 僅 Current，
SelectionSnapshot 為 AtDecision；Inline 的 actor 必須等於 play controller，Handoff 只
接受相對該 controller、基數恰好 1..1 的 RequireOne(Opponents)，不以不相等的符號猜測異玩家。
X 影響模式數或任意追加費用的形狀尚未接受（CR10.6.2.2.1），不能外推模式一律先於 X。
Modes 由 ModeDecision 承載；CostMaterials、Distribution、OptionalCost、Face 與非空
PlaySpec.costs 尚未接受。本文 K.pay PlayParameter 也仍是未實作，不能據此宣稱規格禁止。

Trigger 暫接受 PerFact、無 limit，並限定 EnterField／EnterEx 的 AfterEntry，及
LeaveField／Discard／Destroy 的 BeforeExit；各 pattern 述詞遞迴檢查。
State、AnyOf、Banish（可能裏向）、其他事件形狀、不同快照 policy 尚未接受。Continuous declaration 接受
Numeric 與 Combined patch，以及遞迴檢查的 GrantAbility Definition；Keyword grant、
其他 effect、layer 不一致、未驗 duration／usage／timestamp 一律拒絕。
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

## May 與目標角色

`K.may.choice` 必須為無 precondition 的 MayDecline；Execute 的完整 domain 與 captures
都只供其 body 使用，離開後僅留下不透明的 May receipt。Decline 不會建立本文 receipt。
Execute body 不得再出現選擇，包括獨立 decision、Chosen／DownTo discard、declare、
UpToStepwise、optional pay、mode 與巢狀 May；有選取的 payload 必須併入同一次回答。
沒有選取的 may 使用空 compound，不能拿 Integer 0..1 代替 Execute／Decline。
目前 Bound 手牌材料的可見性尚未驗證，因此 BP19-072 並非完整卡片執行驗收。

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
