# 內部執行的分配、移動與排列契約

本文件規定 typed IR 交給引擎時的語義；它不表示 executor 已支援全部形狀。
[良構檢查](typed-ir-validation.md)只證明結構、型別及作用域；runtime 必須再驗整張卡與
對局情境。公開 program、capability 和最低版本不由本文件新增或放行。
下列輸入、輸出及拒絕條件都是實作義務，局面表是後續 executor 驗收規格。

規則依據為[日文 CR 1.28.0（2026-09-28）](https://shadowverse-evolve.com/wordpress/wp-content/uploads/2026/09/28121513/ShadowverseEVOLVE_cr_1.28.0_260928.pdf)，
核對日 2026-10-21。卡例採 `card-data/jp/` 已審來源，連結指向官方卡頁；
沒有直接卡例的合成邊界會明示。snapshot、producer、聚合量與授權 token 的表示法是
引擎契約，不宣稱官方規則規定了這些資料結構。

## 1. 分配最低量與打出 snapshot

### 1.1 最低量的來源

`ChoiceDomain::Distribution {recipients,total,minimum,binding}` 的 `minimum: Nat`
為每個成員的最低量，必填且不提供預設。lowering 從已審卡文／裁定產生它；來源 sidecar
須可定位卡號、句段、選擇時點和所用規則。合成內部程式可以明示 0、1 或其他非負量，
但沒有真實卡文證據時不能藉此准入整卡。

- 打出時先選為分配對象的目標：CR 10.6.2.4.2 要求每項至少 1。首批 runtime 只接受
  `PlayParameter Distribution` 指向同次 play 的已選目標 binding，且 `minimum=1`。
  其他 play 分配形狀保守拒絕；不能把固定集合與已選目標混為一談。
- `ResolutionChoice Distribution` 必須由該句卡文證明在結算時選擇及其最低量。
  CR 1.3.5 的選數非負不等於所有分配最低 1。良構可接受明示非負量，runtime 的
  合成入口也可驗它，但未核對時點與最低量的真實卡一律不准入。
- 首批只承載對所有成員相同的最低量；不同成員不同下限、動態最低量及未定義的
  分配取代整棵拒絕，不悄悄用 0 或 1 代替。

真實卡例：[BP08-028 簒奪の舞](https://shadowverse-evolve.com/cardlist/?cardno=BP08-028)
的 Q1070（2023-12-15）禁止把 4 分給 5 個已選目標；Q1069 允許選 0 體而繼續後句
對主戰者造成傷害。[BP16-077 エンドレスハンター・アラガヴィ](https://shadowverse-evolve.com/cardlist/?cardno=BP16-077)
依 CR 10.6.2.4.3，在分配時適用改變總量的條件，不在傷害執行時重新判斷總量。
本文件沒有採納任何真實卡的 ResolutionChoice 分配種子；此形狀仍需逐卡證據。

### 1.2 輸入、固定與回答

輸入為已准入能力、authority 的 play／結算 context、分配 DecisionId 與完整回答。
同次 play 先依 CR 10.6.2.1 公開並移入解決區，處理先決選項，再建立選擇 snapshot。
所有該次目標與其分配使用同一狀態版本：目標依序公開 binding 可以改變環境，但不能
推進遊戲狀態或穿插其他玩家行動。取代分配總量／去向在該 snapshot 上套用。
`AtSelection` 可在同一 snapshot 讀新回答；不能另拿付款後的 live 狀態。
付款與 play 事件依序在這之後（CR 10.6.2.5–.7）。

結算分配則在該 decision 開始固定自己的 snapshot，不能誤用 play snapshot。
求值後生成 `ResolvedRecipients`，以玩家或物件 ID＋世代去重，並凍結 total、minimum。
回答是每位成員恰一次的 `ResolvedDamageAllocation`；總和精確等於 total，求和須檢查
溢位。漏項、重項、外來物件、外來世代、低於 minimum、錯總量及錯 actor 均拒絕。
非空域若 total 小於 minimum×成員數，不發無解的詢問；打出程序回到目標選擇前，若
不存在任何合法目標方案則回復 play 前（CR 10.6.2.4.2–.4.2.1）。不能讓玩家額外分配。
結算時的固定非空域若無法滿足最低量，首批回傳 Unsupported／執行錯誤並完整回復
本次 execution，不發詢問、不發布部分 receipt、不繼續後句；它不是遊戲中的「略過
傷害」結果。沒有逐卡證據時不能套用 CR 1.3.2 自行縮小域或放寬下限。此拒絕是內部
支援範圍，真實卡仍須補其結算無解時的裁定。

空域不問、不分配、不執行該 Allocation 傷害（CR 10.6.2.4.1）；authority 保存空結果供
相應 binding 與 receipt 使用，總量即使大於零也不要求空答案的和等於 total。這只略過
該傷害指示，不略過整個能力、其他成本或後句。

輸出包含分配 binding 的 producer 身分、固定成員與其量。producer 鍵為
`(ExecutionId, ScopeId, DecisionId, binding slot)`，重複／迭代使用執行環境的 occurrence
區分；禁止只用靜態 slot 跨執行讀取。Damage 只能讀同 producer 的完整分配，不能重新
求 live selector 或把空掉的量重新分給其他成員。結算時仍要檢查原世代是否合法：
已換區或失效的目標不承受傷害，其他成員仍按原量處理。輸入與合法性重驗不同。

錯誤回答不更動狀態、RNG、事件、待機或額度，仍停在同一詢問。取消／不合法 play
回復整個 play 前狀態；已失效的舊 token 不得因 rollback 而重新有效。這不允許玩家在
看見未提交的隨機成本結果後任意取消（[付款交易](engine-boundaries.md#付款交易)）。

### 1.3 Damage requested／actual 投影

`DamageQuantityOutcome` 的兩欄都表示整個該指示的 Damage 單位，以下是專案採用的
聚合契約；CR 1.3.2.2、5.14、10.10 決定每項是否及實際造成多少傷害。

| 形狀 | requested | actual |
| --- | --- | --- |
| Uniform | 指示開始固定成員去重後，對每項加總 `max(0, amount)` | 每項通過世代／合法性檢查、禁止及取代後，實際造成的正量合計 |
| Allocation | 凍結分配答案各項量的和；空域為 0 | 各原成員實際造成的正量合計；失效成員為 0，不重新分配 |

已選目標在傷害前失效仍保留原 requested 項；指示開始求得的動態集合本來沒有的物件
則不計入。Uniform 原始有號單項 amount 是指示輸入，不冒充 receipt.requested；
負量不變成回復。actual 不以剩餘體力封頂：CR 5.14.1.1 允許體力為負；加倍等取代
也可能使 actual 大於 requested。0 傷害不發 Damage fact，不因 0 使 `did` 成立。

程式可讀同型 Damage receipt 的 Requested／Actual，不能當成 Cards 或對象數。
每目標事實保持每項正量；receipt 合計不另生一個總量傷害事件。只在整個指示完成後
發布，simultaneous sibling 不可提前讀。求和、Int／Nat 轉型與乘法皆 checked；超出
內部可表示範圍回傳 Unsupported／執行錯誤並回復提交，不飽和也不當作遊戲規則。
目前內部 executor 仍拒絕 Damage 投影；實作以上契約與測試後才可移除此限制。

### 1.4 可重現局面

| ID | 輸入 | 正例／應有結果 | 反例／必須拒絕或不發生 |
| --- | --- | --- | --- |
| D-A | BP08-028，墓場 9，已選 A、B | 4 分為 3＋1；requested=4 | 4＋0；或選 5 體分 4；沒有付款／事件殘留 |
| D-B | Q1069，墓場 20、無從者 | 分配空值、requested=actual=0，後句主戰者傷害仍執行 | 不問分配；不能略過後句 |
| D-C | BP16-077，分配時體力 10 | 固定 total=10；後來變 11 不重分 | 不得在 Damage 重算成 5 |
| D-D | 合成結算分配 A、B，total=4 | minimum=0 接受 4＋0；minimum=2 接受 2＋2 | minimum=2 拒絕 3＋1；total=3 則無解，不發詢問且完整回復 execution；沒有來源證據不能准入真實卡 |
| D-E | 分配 3＋1，A 在效果成本中換區／同實體重新入場 | requested=4、actual=1，B 只受 1 | 不可改選新 A、補分 3 給 B，或跨 execution 讀舊 binding |
| D-F | BP02-052 インペリアルドラグーン的 Uniform 片段，單項 3，三個相異成員 | requested=9；合成取代使其中一項 0、一項 6、一項 3，actual=9 | 重複 union 不多算；生命剩 1 也不把 3 截為 1 |
| D-G | 合成 Uniform -2；或空集合量 3 | requested=actual=0，無傷害事件 | 不回復；溢位不飽和；receipt 型別不能混用 |

D-E 的局面用同一規則程序在分配後、傷害前移動目標，不引入不存在的回應窗口。
所有含回答的局面須交換兩席再跑，並比較拒絕前後狀態與觀察。

## 2. CardProgram、能力實例與打出准入

### 2.1 三層准入

輸入是 `WellFormedCardProgram`、相符的卡表／來源對映與 authority context，不能讓
呼叫者直接注入 `RuntimeAbilitySource`、ExecutionId 或其他卡的 AbilityId。

1. 載入整張卡：所有能力、play captures／parameters／costs、所有成本替代、未選模式、
   條件分支、宣告與引用 closure 都須在 executor 支援集合。這是「是否實作」檢查；
   不要求未選模式在當前局面也有合法目標（CR 5.18、10.6.2.8.2.2）。
2. 對局預檢：所有可能介入的禁止、取代、事件、continuous／pending 能力也須有受驗
   管線。遇到原型能力混用或未支援依賴拒絕，不靜默忽略。封閉 Program 合成入口不
   等於整卡入口，不可用只抽取一個片段繞過第 1 層。
3. 實際 play：驗證 current holder／有效區、controller、正確時間／quick window、
   pending occurrence（自動能力）、模式／目標、分配與完整成本。只對所選分支做動態
   合法性檢查。Static 不經 play；Triggered 只能由 authority 的待機 producer 啟動。

輸出是 authority 封閉的執行環境或明確拒絕；禁止外部補一個「trusted=true」繞過檢查。
首批 play 可採 Printed Spell／Activated／已支援的 PerFact Triggered 子集；尚未實作
任何一層、卡片類型的場容量或移入／離開解決區程序，便維持相應整卡拒絕。異玩家
Handoff、Grant／Delay、重複觸發、RuleCarry 等須在各自依賴驗收後另行准入。

### 2.2 producer 與來源

以下為 runtime producer 規格；資料結構的儲存方式可由實作者選擇，身分關係不可省略。

| 資料 | 唯一 producer／存續 | 拒絕條件 |
| --- | --- | --- |
| AbilityId | 已載入 CardProgram 的定義；與卡表 face／line 對映 | 外來卡的定義、假造 face／line、未載入來源 |
| AbilityInstanceId | authority 於該 holder 世代實體化該定義時產生；同世代重複起動沿用實例 | 以 AbilityId 代替 instance；新世代重用舊實例 |
| ExecutionId | authority 在一次合法 play 嘗試／封閉入口建立；同一實例多次執行各不相同 | 客戶端指定、重用另一執行的 binding／receipt |
| RuntimeAbilitySource | 從實例取得 holder、origin 與能力控制者；結算環境保存其來源視圖 | 把 target、文字提供者或目的區 owner 當成來源／控制者 |
| EffectId | authority 在成功安裝指示建立修正實例，綁該 execution 與被安裝對象 | 把未執行的指示、失敗安裝或其他執行的 id 當成 producer |

卡片 play 從手／EX 移到解決區後，從該新世代建立本次 Printed Spell 的來源實例；
對舊區域的 captures 只能用明示時點。起動能力的來源留在原世代，即使成本令它離場，
不把新世代重綁成 holder。誘發能力沿用觸發當時的能力實例及其控制者；來源離場
不取消已待機／已打出的能力（CR 3.1.2、10.6.2.8.2.1、10.7.7）。來源所需的舊資訊
以事件／LKI 視圖保存；Current 讀取仍須驗世代，不能偷偷改成 LKI。

CR 10.12.1.1／10.12.2.2 決定能力效果及傷害的來源通常為持有能力的卡。由規則造成
的處理用 `Cause::Rule`，不捏造能力來源。來源視圖與 controller 在本次 execution
固定；目標的 owner／controller 在各自規則要求的時點讀取。未支援 LKI 傷害時，會
需要該路徑的完整能力先拒絕；不可把「來源不在場則取消整個能力」當遊戲規則。

成本成功後才發 play 事實；本文完整解算並處理解決區收尾後，才交回 CR 10.5 的
check timing／待機管線。不得每條 Instruction 任意呼叫原型回合推進。發生事件可先
形成待機，但不在本文中途插入另一個 pending 的結算。正式入口必須有明示的 resume
stage，不能憑「目前沒有問題」自動推進到 end／discard。

### 2.3 卡例與局面

[BP01-006 ロビンフッド](https://shadowverse-evolve.com/cardlist/?cardno=BP01-006) 的
起動成本與目標／傷害可驗同一 instance 兩次起動、不同 ExecutionId；只完成傷害片段
不等於其 Fanfare／Act 成本都已支援。
[CP02-007 ブランニュー・ビート](https://shadowverse-evolve.com/cardlist/?cardno=CP02-007)
成本使來源離場，本文仍加攻體；Q917 禁止沒有目標時先付離場成本。

| ID | 正例 | 反例 |
| --- | --- | --- |
| P-A | 同一來源兩次起動 instance 相同、execution 不同，事件歸到各自 execution | 第二次讀第一次的分配、receipt 或 token 拒絕 |
| P-B | CP02-007 有目標且成本完整，來源離場後本文仍結算 | 無目標不能先把來源移入墓地；缺少離場後來源支援則預檢整棵拒絕 |
| P-C | 已選模式 A 有合法目標；模式 B 已實作但當前無目標，A 仍可打出 | B 含未實作 Action，即使此次不選 B，載入整卡拒絕 |
| P-D | 成本與能力完全受驗後，在規定時點產生一次 play 與 resolve 事實 | 僅有 WellFormedProgram 不可宣稱 CardProgram 可執行；偽造 pending／origin 拒絕 |

## 3. 破壞、移動與成功成員

### 3.1 場外 Destroy 與禁止

CR 5.6.1 的 Destroy 是把**場上的卡**移到該卡 owner 墓地；沒有場外破壞的通用
定義。首批將指定給 Destroy 的場外／失效成員視為不適用，不搬卡、不發 Destroy
事實（CR 1.3.2）。卡文明示場外破壞或特殊含義的整卡須另有證據，不能由這個 no-op
規格外推。[BP02-116 氷獄の呼び声](https://shadowverse-evolve.com/cardlist/?cardno=BP02-116)
是場上破壞種子；Q323 無目標時禁止 play，與結算時目標已失效不同。

[BP02-089 ヘヴンリーイージス](https://shadowverse-evolve.com/cardlist/?cardno=BP02-089)
的禁止只阻擋能力破壞，不能阻擋一般 Put(Cemetery) 或 Banish。CR 1.3.3 及
[BP18-SL16 邪智の竜・ファフニール](https://shadowverse-evolve.com/cardlist/?cardno=BP18-SL16)
Q2345 要求先尊重禁止，不能把已被禁止的原事件送進取代管線以繞過禁止。

### 3.2 精確成功 before

輸入為指示開始固定的 before 世代及 proposed destination／position、MoveMethod、
cause；輸出逐成員保存 `MoveResult`。requested 是規則解析後、取代前的提案；actual
是取代鏈完成後確實提交的結果。每個取代對同一事件至多一次（CR 10.10.2.3）。

對 `ObjectReceipt::Movement Before` 而言，**成功 before** 精確定義為本次該成員的
移動管線確實提交了一次離開原區域的世代轉換，且 MoveResult 保存對應 after／actual_to。
它是「確實移動」的成功集合，不表示原要求的 Destroy／Put／Discard 事件發生。

| 結果 | 成功 before | after／事件 |
| --- | --- | --- |
| 正常跨區 | 包含舊成員 | 保存新世代與實際事件 |
| 取代改往其他區，仍實際跨區 | 包含舊成員 | 保存改道 after；不偽造被取代的原事件 |
| 禁止、容量未選中、失效、留原區的取代 | 不包含 | 無 after，不得取得 entry_effect／減費 |
| 原移動被改為非移動處理 | 不包含 | 只有該替代處理的真實事件；本移動 receipt 不假造 after |
| 同區 Order | 不屬於跨區成功集合 | OrderOutcome 記重排成功，moves 空，不換世代 |

CR 10.10.1.1 決定被取代事件不存在；Q2346 證明墓地改消滅不能產生 LastWords。
這個物理移動成功集合可供 remainder 扣除、move-after 或容量計數；**不能拿它當
「已破壞」事件或完整付款**。卡文若讀「被破壞的數量」須依對應實際事件語義，
不明時另立 predicate，不直接 count 成功 before。付款依 CR 10.6.2.5.2–.3 另判
PaymentComplete，即使物理事件改道或費用為零也可能完整。

[BP18-059 壊竜の六銘伝・エル](https://shadowverse-evolve.com/cardlist/?cardno=BP18-059)
的 remainder 是 looked 減成功 before：EX 滿而沒移動的牌仍留在 remainder；已被
改道搬走的牌不能再次拿去排牌。後句減費只作用於有規則指涉且適合該效果的成功
新世代，不因 before 成功就自動取得其可見性或跨區追蹤權限。

### 3.3 混 owner 的目的區

`Shared(ZoneRef)` 是 IR 中「一個有 player 的目的區」的名稱，**不是兩位玩家共享的
場／EX**（CR 4.1.1）。一般輸入的混 owner 集合逐卡解析，不能把來源控制者當 owner。

- `Owners(Hand/Deck/Cemetery/Banish)` 逐卡去其 owner 的對應區。
- 明示 Field 目的玩家決定該場；但兩個玩家場之間的移動屬 Transfer 路徑，依
  CR 5.22.1–.4 不換世代／不發 EnterField，不能用一般 Move 偷做。
- 場上曾被奪／渡的卡離開場去其他區，依 CR 5.22.5 回到 owner 的該區。
  例如混 owner 的場上卡移到 EX，不能全部丟進 `Shared(Self,Ex)`；requested_to
  須先逐卡依此規則解析。取代後再次驗證合法去向及該目的區的容量。
- 未明示玩家的區依 CR 4.1.6 去 owner；lowering 不能因 MoveDestination 尚缺
  `Owners(Ex)` 就把混 owner 卡寫成單一 Self。首批若未能表達來源規則，拒絕該
  lowering，或在可信指示 context 明示 5.22.5 的離場規則；不可推測卡文。
- 無場上控制轉移背景、卻要求把外來 owner 的卡從其他區放進某人的 EX，尚無本片
  真實卡例；保守拒絕該情境。普通同 owner 的 Hand／Deck→Self EX 不受此限制。
  場上外來 owner 卡若不是經 Transfer 取得，且指示明示另一玩家的目的區，亦須有
  該特殊來源的規則證據才能准入，不能泛用 5.22.5 類推。

真實背景：[BP13-043 混沌の流儀](https://shadowverse-evolve.com/cardlist/?cardno=BP13-043)
的渡す，以及 [BP03-109 カオティックエンジェル](https://shadowverse-evolve.com/cardlist/?cardno=BP03-109)
的奪う。它們證明 owner 與 controller 分離；不是宣稱它們具有「混 owner 一起放 EX」
的卡文。該混合集合是以上規則的合成局面。未指定 Deck position 的一般 Move 首批
仍拒絕；不能選用容器預設位置當卡文。
BP03-109 Q529（2022-12-09）直接確認奪來的從者放 EX 時回 owner EX；
BP13-043 Q1747（2024-12-13）確認渡出的從者離場時回 owner 的區域。

### 3.4 event move follow 與觀察

`FollowEventMove` 只能沿當前 trigger context 所綁定的**一個** Move／LeaveField
事實取得該 member 的 after；來源 holder 不改綁。沒有 after、已再換世代、錯 event
或只有整批資訊而不能定位單一 member，拒絕該依賴；不得以相同卡號搜尋替代。
CR 4.1.4.1 與 10.7.4.1 的指涉／資訊時點不等於一般永久追蹤權限。

authority 可以保存因果連結。給玩家的 view／log／決定 domain 則按 audience／epoch
投影：對手不可取得未公開手牌的固定 ObjectId、generation、receipt member→after
映射、可跨詢問比對的 opaque handle 或洗牌後的位置。顯示曾公開的卡名／移動事實
也不授權持續追蹤那張手牌。非公開區移位、洗牌、FlipDown 與公開效力結束依
CR 4.1.5.1、5.21.1.1 撤銷證據，舊 epoch 不復活。

私有 Order／Search 問答僅給 chooser 本次已授權的 handle；authority 轉為 ObjectRef
後驗證。其他玩家只得到允許的公開事實，不得到候選順序。純 authority 的 receipt
不得直接序列化為 player payload；精確單 member 與投影做不到時整能力拒絕。
完整 `MovedRef` 權限、RuleCarry、SelfOnly／BP02-041 白霜の風不因本契約自動准入。

### 3.5 可重現局面

| ID | 正例 | 反例 |
| --- | --- | --- |
| M-A | BP02-116 的場上破壞去各 owner 墓地，有新世代 | 場外 Destroy 不移動；失效目標不跟新世代；不能把 Q323 的無目標 play 當可打出 |
| M-B | BP02-089 不能被能力破壞；一般 Put 仍按自己的規則執行 | 禁止破壞時不可先改為 Banish；不發 Destroy |
| M-C | BP18-SL16 改墓地為消滅，before 成功、actual_to=Banish | 不發原墓地／Destroy／LastWords；不可因未進 requested_to 而把它留在 remainder |
| M-D | BP18-059 選一張、EX 滿、未移動：全部 looked 排回底 | 無 after 不安裝減費；同名另一張不能代替 |
| M-E | 渡す後混 owner 場上兩卡去 EX：各回 owner EX，分別判容量 | 不能全入 actor EX；場→對方場不得產生新世代／EnterField |
| M-F | 公開離場 trigger 有確切單 member，authority 能指涉 after | 對手不同手牌排列的 payload 不可區分；再洗牌／換區後舊 handle 不再授權 |

## 4. Order 的輸入點與 Search 收尾

### 4.1 Order 協定

`Action::Order` 有獨立 `decision: DecisionId`，與一般 Decision、MoveSpec.order、
Stepwise 等共用良構唯一性檢查，不能借 InstructionId；即使本次域空也保留靜態身分。
`MoveSpec.order` 則沿用它自己的 Decision.id。這是內部協定，不新增公開 wire 版本。

輸入為固定 cards 域、唯一 Placement、chooser 與 visibility。Action::Order 僅對已在
目的牌庫的牌同區重排；跨區使用 Move＋order。先求值、去重、驗每個世代與選擇證據，
再固定本次排列域。輸出 `OrderAnswer {cards: Vec<ObjectRef>}` 必須是該域完整排列；
local `matches_domain` 僅驗集合／重複／世代，不提供任何 authority 權限。
原始候選域須唯一，不能在驗答案時悄悄刪掉答案中的重項。

沿用 `game::internal::DecisionToken {execution,decision,epoch}` 與 server/session 的
玩家身分驗證：token 本身不是知道三個整數就取得權限。待答 context 同時保存 actor、
排列域、visibility 與 snapshot；actor 必須是 chooser。一次有效提交消耗 token、推進
epoch；同 DecisionId 在下一次迭代有新 epoch。其他 execution／舊 epoch／錯型別／
錯玩家拒絕，Game 保持待答。重播鍵為對局內的 `(execution,decision,epoch)`；同鍵
再次送達不得再次重排／耗 RNG，交還已記錄結果或明確 AlreadyConsumed，不能重新求域。
若待答可存檔，必須完整恢復 pending 與 token 序列；做不到便保留禁止待答存檔。

空域自動記錄空排列而不問玩家；非空域即使只有一張仍依一般必問規則，除非另有受驗
的等價省略程序。完整回答以牌庫頂至底方向解讀：Top 的第一張是牌庫最頂；Bottom 的
第一張是底部區塊最上方，不反轉。先從原牌庫移除這個域，再一次插入區塊；域外卡的
相對順序不變。只發受允許的 Order 結果，moves 空、世代不變，無離場／入場事件。

CR 4.1.3、4.1.5 及 [BP07-SL09 アイアロンの支配者・ベルフォメット](https://shadowverse-evolve.com/cardlist/?cardno=BP07-SL09)
的 Look 餘牌排序提供實例。混 owner 分牌庫後保留相對順序只是資料處理方式，
不授權單 chooser 操控他人順序：預設依 CR 4.1.5 由目的區所屬玩家定序；首批單一
chooser 若跨 owner，除非卡文明示該 chooser 可排全部，runtime 拒絕。尚無本片實卡
證明這項明示例外；checker 接受它不代表 runtime 准入。

### 4.2 Search destination 的域與時序

Search 的 found 是搜尋者找到的完整舊世代集合，revealed 只是向對手證明的集合。
若將來准入 destination.order，其域必須是該移動實際輸入的 found（Partition 則為
該段從 found 分出的成員），不能換成 revealed、全牌庫或重新搜尋。完成容量／取代
後只搬實際成功成員；同一完整排列投影至成功成員保留相對順序，不再問第二次排序。

標準 Search 嚴格採 CR 5.8.1–.2：搜尋回答 → 按規則出示條件證據 → 決定可選目的區／
移動與容量 → 完成所有 destination → **只洗來源牌庫一次**。沒找到或選擇不移動
也洗；0／1 張仍算 shuffle（CR 5.9.1.1）。
[BP03-002 コスモスファング](https://shadowverse-evolve.com/cardlist/?cardno=BP03-002)
Q445 的先找到再選場／手與 [CP04-060 カヤ](https://shadowverse-evolve.com/cardlist/?cardno=CP04-060)
的搜尋進 EX 都走此程序。

本片維持 Search destination.order／position 的良構拒絕：

- 放回來源牌庫後再洗會消除指定順序；不能暗改成「先洗剩餘牌庫、再放 found」。
- 放入另一牌庫的時序可按上列程序描述，但缺乏本片採納的真實卡例與 found 排列
  binding 良構擴充，仍不放行。
- 明文「洗牌後放置」需要另一個獲證據支持的流程；不能在 SearchDestination 偷加
 任意 Program callback。Look 餘牌本來仍在牌庫，依原卡使用獨立 Order，不是 Search。

這個邊界使實作者可先完成無 order／position 的 Search 與獨立 Order；不允許僅為
通過測試解除所有 Search destination 限制。多 Placement、獨立／compound Order domain、
entry_effects 與 Move＋Token 共享容量仍依各自契約驗收。

### 4.3 可重現局面

| ID | 正例 | 反例 |
| --- | --- | --- |
| O-A | BP07-SL09 剩餘 A、B、C 回答 C、A、B 放底，底部自上而下 C、A、B | 漏 B、重 A、新世代 A、外來 D 拒絕；原牌庫不變 |
| O-B | 同 execution 重複 Order 靜態點，以不同 epoch 接受本輪答案 | 其他玩家、上輪 token、相同鍵第二次提交不再執行 |
| O-C | 空域自動空結果；同區排列後域外順序、世代不變 | 非空域不能回空；無 Move／Enter／Leave 事件 |
| O-D | CP04-060 搜尋成功去 EX，再洗來源一次 | EX 滿、沒找到也依規則收尾；不透露未找到是「沒有候選」 |
| O-E | BP03-002 出示低費獣後選手牌，最後洗牌 | 不得在搜尋前據實際隱藏候選預先詢問／隱藏改道選項 |
| O-F | 合成帶 Search destination.position／order 的整棵程式拒絕 | 不能先洗再放頂假裝符合 Search；不得以 revealed 縮小 found 域 |
| O-G | 私有 Look 排列僅 chooser 可見；對手不同私有排序有相同輸出 | 對手不得收到固定隱藏 ID、完整排列或可重用 handle |

## 5. Numeric、usage 與 timestamp 的首批範圍

CR 1.3.2.4 允許無特定上下限的數值資訊為負；這不表示所有動作都可要求負數。
首批 Numeric Add 保留有號差值；Attack／Life 的 Set 先只准入可證明非負的結果。
[BP16-049 スノーアウェイク](https://shadowverse-evolve.com/cardlist/?cardno=BP16-049)
提供 Set 1／1 的真實種子，沒有本片真實卡證明直接 Set 負常值的語法意義；負 Set
仍列為未准入，不在 IR 有號欄位先截零。CostModifier Set 同樣先限非負，動態結果
不能證明則拒絕，不把負費用資訊可存在（CR 1.3.2.2.1）當作任意 Set 的許可。

數值修正 usage 維持拒絕；只有已受驗的 CostModifier 可使用共享 UsageId，完整成功
提交才消耗，預演／失敗／rollback 不消耗。多張候選共享一次額度不能每張各建一份。
期限使用實際 occurrence；NoTemporalExpiry 仍受世代與作用域約束，不等於永久跨區。

CR 10.9.1.6.1 的 static timestamp 是目前區域的入區時點；.6.2 的其他能力則是
ProperPlay 時點，不是每次讀取、安裝或重算時重新計時。效果安裝的 EffectId producer
仍在成功安裝時產生；id 產生時點與排序 timestamp 不混用。合成 closed Program 沒有
play 來源時，需由可信入口明示合法 timestamp，否則拒絕 timestamp 相依效果。
同 timestamp 的不可交換排序沿用既有 inferred 的 ContinuousTieOrder 契約；不宣稱
CR 已直接裁定。非數值／Grant 完整 timestamp、RuleCarry 仍未准入。

局面：CP02-007 的來源離場後加攻體仍在目標舊世代存續；目標換區後不跟隨。
BP16-049 把已受傷目標 Set 1／1 後讀有效值，重算不另發 stat_changed；同一次明示
Attack／Life 修改只形成一個多 facet 事實。反例須含負 Set 拒絕、usage Modify 拒絕、
成本失敗額度不減、兩個不同 play 反序安裝仍依 ProperPlay 排序，以及 snapshot 重算
不推進 timestamp。這些是 executor 待實現的測試，非本次 Rust 良構測試的保證。
