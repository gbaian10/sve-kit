# 玩家決定題

`QuestionShape::Decision(DecisionShape)` 只給本題應答席操作所需的語義與候選。
用途仍由 `QuestionPurpose` 表示；同一種答案可能服務不同用途。宿主把答案與
`QuestionToken` 一起交給 `Command::Answer`。權威仍驗證應答席、完整 token、候選、
數量、角色、付款、世代與揭露權限，題目本身不是付款或執行許可。

## 題型與答案

| DecisionShape | Answer | 回答方式 |
| --- | --- | --- |
| DiceReroll | Integer | 0 保留、1 重擲；值與剩餘次數依原規則公開 |
| Abilities | Ability | 選一個有效能力代號 |
| ResourceEntryPayment | ResourceEntryPayment | 本題材料代號與是否使用 EP |
| WardEntry | WardEntry | 接受或拒絕入場姿態 |
| Quick | Quick | 一個合法打出代號，或 None 明確 Pass |
| OptionalCost | OptionalCost | 宣告是否接受可選費用 |
| ResolutionPayment | OptionalCost | 結算中是否付款執行子效果 |
| CostMaterials | CostMaterials | 完整付款材料集合 |
| Discard（Effect） | Targets | 效果棄牌的完整手牌集合 |
| Discard（Cost） | CostMaterials | 付款棄牌的完整手牌集合 |
| ExchangeRoutes | Integer | 列出的原生路線索引之一；不重新編號或補齊缺口 |
| May | Integer | 0 拒絕、1 接受可選結算本體 |
| BeforeDraw | Stepwise | Continue 繼續、Stop 停止逐張抽牌 |
| Pending | Pending | 選一個本題待處理 occurrence |
| TriggerQuota | TriggerQuota | 選一個本題候選 occurrence |
| Integer | Integer | 在提供的界限內填值 |
| Order | Order | 每個候選恰好一次的完整排列 |
| DamageReplacements | Order | 全部取代效果代號的排列 |
| DriveTrigger | DriveTrigger | 是否執行及適用時的目標代號 |
| Search | Search | 完整選取；有群組時同時回答全部群組，包含空群組 |
| SearchPartition | SearchPartition | 本次目的地的完整分配 |
| MovementCapacity | SearchPartition | 從到達候選選出 available 張 |
| Distribution | Distribution | 每個候選恰好一份，滿足逐份下限與總和 |
| SearchReplacement | SearchReplacement | 已找到集合的原目的地或取代目的地 |
| Targets | Targets／TargetsWithRoles | 完整目標集合；有角色時同時回答完整不重疊分割 |
| TokenCapacity | TokenCapacity | choices 中原索引對應的合法名稱組合 |
| Modes | Modes | 宣告模式；依原生模式識別值與數量限制 |
| ResolutionAlternative | Modes | 選一個結算選項；原生模式識別值不重新編號 |

`MovementCapacity` 與 `TokenCapacity` 都明列接收席、Field／EX 目的地與可用空位。
前者選尚未移區的卡片或尚未建立的 token 候選，後者選名稱組合；都不事先建立 token。
`May` 不等於宣告的 `OptionalCost`；`ResolutionAlternative` 也不是連續整數題。

## 可見性與查詢

非應答席先取得 Waiting，不建構私有 shape；骰值依既有規則附在等待摘要中。
`input` 不推進程序、不付款、不改 RNG，也不配置觀看者或權威代號。
卡片清單使用 `CardOption`；目標、材料、棄牌使用 `SelectionQuestion`。
這些封閉 DTO 不含實體 ID、權威參照或 continuation。同印刷的不同候選仍有不同
本題代號，不能把印刷號當作物件身分。

候選的 `object`（能力選擇的 `holder`）可連到本席 `Observation` 中既有的
`ObjectHandle`。連結保存來源凍結時的世代與辨識期；只有目前仍是同一辨識期、
同一世代且本席仍能定位時才提供。私有 Look 在取得知識後保存來源連結，後續
排序、分配沿用該來源；搜尋 continuation 也沿用找到集合的來源。洗牌或退休後
重新定位不會把舊候選連到新代號。未知牌庫候選、尚未建立的 token 與主戰者沒有
物件連結，候選本題代號仍可正常回答；不提供隱藏牌庫索引。

`AbilityChoice`、`QuickChoice`、`PendingChoice`、`QuotaQuestion` 與
`ReplacementChoice` 保留原題座標、已授權印刷及文字來源。Pending 使用原 occurrence
捕捉的來源；quota 歷史選擇只有已公開上下文，不以印刷或行號推測物件關聯。

## 完整選取限制

`SelectionQuestion.cardinality` 是本次整體有效上下限。`groups` 一律至少一組；
一般選取的單組界限等於整體界限，多組則要求存在不重疊分派，使每組達到自己的
有效上下限。同一候選可同時符合多組資格，但只能分派一次。結算時單組的有效
下限可能依剩餘候選縮減，不能用宣告時的印刷下限取代。

`distinct_names` 使用候選捕捉的完整 `rules_names`，包含追加名稱；`name` 只是顯示
名稱。`original_costs` 與成員的 `original_cost` 使用原生驗證的原費來源，與目前費用
不同。`distinct_original_costs` 才表示原費必須互異；有原費 map 本身不表示互異。
`minimum_original_cost`／`maximum_original_cost` 分別限制選取總原費的下限／上限。
未參與限制的名稱或原費可省略。`roles` 保留原角色座標、牌庫端點及 Fixed、
IfNonempty、Remainder 大小規則；有角色時整個選取與完整不重疊分割一起回答。

`SearchQuestion` 同時提供整體界限、完整名稱與總原費上限；原費讀取與原生驗證
共用同一函式。`groups: Some` 表示必須使用 Grouped 答案，所有群組包含空群組都要
同時提供；`None` 使用 Ungrouped。答案使用原候選座標，不重新編號。分配與排序
各有自己的完整集合或排列語義，不能把逐個選擇當成已提交的部分答案。

`EvolutionQuestion::Materials`／`Discard` 共用相同 `SelectionQuestion` 投影，仍經過
原演化宣告的材料、棄牌與付款重驗。一般演化、超進化及效果演化的原有差異不變。
宿主可從題目建立完整答案，但提交仍由權威重驗；不一致、錯席與過期 token 仍會拒絕。

權威在提交後交出下一題前驗證投影，包含 typed Wait、pending／quota 與 Quick 快取。
投影不一致沿原回滾路徑成為持續 EngineFault，雙方一致；投降仍可執行。

## 基礎 policy 的選取答案

`policy::select`、`search`、`distribute` 只接收玩家題目的凍結資料，不讀取對局、
物件實體身分或題目 token。選取會同時滿足整體上下限、完整規則名稱互異、
原費互異與總費限制、群組分割及角色分割；搜尋保留所有群組（含空列）。
分配對每個候選恰給一份，空候選依原生規則略過總量限制。

策略依候選順序選擇最少的非空合法集合，沒有非空解且允許時才選空集合。
`SelectionAnswer.roles` 在題目有角色時列出每個角色，否則為空；
呼叫端仍須依題目用途組裝原生答案，再由玩家提交介面驗證。
`InvalidQuestion` 表示缺少必要資料或題目結構損壞，`NoSolution` 表示已窮盡所有可能，
`SearchExhausted` 表示共用的 100,000 次選取／群組回溯步數已耗盡、可行性仍未知。
上限不截斷候選，也不能將策略失敗當作正常終局或引擎錯誤。

## 基礎 policy 的程序與主要行動

每席各持有一個 `BasicPolicy::new(viewer)`，呼叫
`choose(&Observation, &QuestionShape)` 取得 `Choice`。
策略拒收其他觀看者的觀察，僅讀玩家資料；`Choice` 保留候選的不透明能力供回送，
題目 token 由呼叫端綁定為 `Command`，不參與策略偏好或回合識別。

目前各題型皆有確定性答案：整數取有效下限，路線取第一個實際座標，排序回完整原順序，
選取／搜尋／分配沿用前述求解器。Modes 取原生上限內最少非空選項，保留可用量為零時
回空的原生差異；有角色的選取使用完整角色答案，Discard 依付款或效果使用原答案通道。
開局選第一個起始護符、自己先攻並保留手牌；收尾選取全部 Guard 與精確的超額棄牌。
進化選第一個形式，PP 足夠時先用 PP，不足時才用允許的 EP，依題目許可使用超進化。
Quick 選 Pass、BeforeDraw 選 Stop，其餘可選付款與效果接受；Drive 只在 Critical／Stand
帶上第一個目標，空集合依原生規則略過指定；Front 不接受。

主要行動依序偏好 Play、Evolve、Activate、Attack、EndPhase，同類保留原候選順序。
每個公開回合，Play、Evolve、Activate **每類各一次**；Activate 的限制涵蓋該席所有來源，
不是每張卡各一次。Attack 持續選原生列出的合法攻擊，不因策略預算跳過強制攻擊。
這是保守策略，沒有比較未來局面的搜尋。

每次提交後，立即以剛送出的 `Choice` 呼叫 `record_submission(&choice, kind)`。
Applied、Reverted、Cancelled 才使用該主要行動類別的預算；Rejected、EngineFault 與
非主要行動的答案不使用預算。查詢、未提交的選擇及其他題目的回報都不暗中使用預算。
宣告被接受後才回退時，該類預算仍已使用。公開的 active 或 elapsed_turns 改變時清除預算，
同回合內的 phase 改變不會清除。

完整對局宿主仍須區分策略失敗、有限次的 Rejected 重試、回退／取消、EngineFault、
正常 Finished 與步數耗盡，並以步數上限處理可能的持續攻擊循環；步數耗盡不能當作終局。
