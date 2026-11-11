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
卡片清單使用沒有實體 ID 欄位的 `CardOption`，只帶本題局部代號、已授權印刷與名稱。
同印刷的不同候選仍有不同本題代號，不能把印刷號當作物件身分。

本次語義題型實作暫沿用原生的安全值載荷：AbilityOption、QuickOption、PendingOption、
QuotaDomain、ReplacementOption、TargetDomain、DistributionDomain 與 SearchGroup。
這些載荷只含本題座標與已授權資訊，不含權威 continuation 或實體物件參照；
TargetDomain 原有 roles 與 original_costs 保留。候選與 Observation 的連結，以及
選取所需的完整凍結名稱、群組與總費限制，列於 [#1977](https://github.com/gbaian10/sve-kit/issues/1977)
的剩餘驗收，尚未由本片補齊。宿主目前仍須接受原生驗答拒絕，不能從本片題目推定完整合法集合。

權威在提交後交出下一題前驗證投影，包含 typed Wait、pending／quota 與 Quick 快取。
投影不一致沿原回滾路徑成為持續 EngineFault，雙方一致；投降仍可執行。
