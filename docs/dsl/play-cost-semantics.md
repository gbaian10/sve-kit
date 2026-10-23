# 打出費用修正

本文件補足 [play 程序](play-semantics.md)步驟 5，定義內部 typed IR 的實作義務，
不宣稱 executor 或正式來源准入已完成。作者語法仍以 `dsl/` 為權威；不變更公開 program、
capability、最低版本或 body hash recipe。

本文件的無付款第一版界線不變；後續單一材料換常值修正依
[付款交換契約](play-payment-exchange.md)，其首批範圍取代下列對該子集的概括拒絕。
executor 是否支援仍須另驗，不因規格已補足就跳過准入。

依據為[日文 CR 1.28.0（2026-09-28）](https://shadowverse-evolve.com/wordpress/wp-content/uploads/2026/09/28121513/ShadowverseEVOLVE_cr_1.28.0_260928.pdf)，
核對日 2026-10-23。以下條號均指該版。第一版的限制是支援界線，不是遊戲規則禁止。

## 1. 第一版範圍

| 形狀 | 第一版規格與界線 |
| --- | --- |
| 自身無付款的常值減費 | 接受無條件或公開條件的非正整數 delta；多條獨立條件各自適用，如 BP01-U05 メドゥーサ的兩段減費。關鍵字條件仍須有受驗的規則展開，不能只憑名稱准入 |
| 自身分段減費 | 接受同一公開計數的門檻分段；BP01-061 フレイムデストロイヤー以 15／10／5 的互斥區間分別取 −9／−6／−3，不能把「代わりに」加總成 −18 |
| 自身單一設值 | 接受至多一條非負常值 set，可有公開條件；這是合成形狀，沒有把付款才設值的 BP01-P11 次元の超越當成無條件真例 |
| 已安裝到特定卡的減費 | 沿用已受驗的成功 MoveAfter→EX、非正常值 delta、CurrentTurnEnd 子集；按 §2 修正資訊讀值與付款的界線。BP01-084 騎竜兵的無明示期限不能硬改為 CurrentTurnEnd，整能力仍拒絕 |
| 付款換 delta／set | BP01-SL15 骸の王、BP01-P11、BP07-024 王の一閃：本版整卡拒絕；付款義務見 §3，不把材料成本藏成一般效果 |
| 必要／可選追加、替代費用 | 包含單一 PP 的「+N してよい」、多組付款、模式內付款：本版整卡拒絕，不以已能扣 PP 推定可接受 |
| 別張卡的持續減費 | BP01-055 エンシェントアルケミスト等場上 aura、下一次 play、共享額度、不同期限維持拒絕；不能預先把場上來源改為每張手牌的固定安裝 |
| 動態數量與其他付款 | 自由 X、from_payment、卡文以 X 定義的計數減費、每 N 張的減費算式、正 delta、費用下限覆寫、複數 set、付款取代均拒絕 |

公開條件僅接受已受驗的純布林、布林組合、公開場／墓地查詢的存在性或常值門檻比較，
以及已有完整規則定義與 runtime 讀值的關鍵字條件。命名、種類、種族依同一卡表／規則名稱
身分解析；不以字串卡號替代。未知 predicate、非公開內容、歷史事件讀值未接、未支援
關鍵字或動態操作數，連同未選分支一起整卡拒絕。印刷卡上的其他能力仍須完整准入；
表中的真卡只證明片段，不是已可使用的完整真卡。

## 2. 決定實付 PP

### 2.1 時點與資料

CR 10.2.1.3.3、10.10.1：打出時修正屬對打出程序的置換；在被修改的成本決定步驟適用。
條件與數量在步驟 5 開始、任何付款之前，讀同一份 Resolution 狀態一次。
不是步驟 0 的 AtPlay，也不是本文的 AtResolution；不用新增 capture 種類。
步驟 0 只保存原打出區域等 authority 事實，供明示「從 EX 打出」的條件使用；
不得到步驟 5 用 `current zone == EX` 猜測。EX 承接、EffectId 與期限完全沿用
[play §2.1](play-semantics.md#21-ex-承接與費用只付一次)。

SC 用於本段費用置換時讀此時墓地，不把 Choice 模式數的 AtPlay 特例或本文解決的
SC 捕捉時間套到費用；第一版不包含會改變墓地的追加付款。打出的牌此時在 Resolution，
不是墓地中的一張 Spell；各條獨立條件與分段區間在同一狀態求值。

CR 10.4.4.1 將卡片 Cost 資訊與打出時扣除的 PP 分開：自身 play 修正和
`when_playing` 的 CostModifier 只改實付量。印刷 Cost、有效 Cost 資訊、搜尋的
cost 上限／比較、材料 orig_cost 及按 Cost 造成傷害，都不得讀到這些付款修正。
如果另有真正更改 Cost 資訊的效果，須有其獨立語義與受驗讀值；本版不增加這種效果。
既有將 CostModifier 同時接到 `internal_effective_cost`、Search 及付款的實作須拆開，
不能把原型行為當作 CR 例外，也不能用原型 cost map 混入 typed 付款。

### 2.2 合成順序與下限

依 CR 10.6.2.5.1.1–.4、10.10.2.4，先取基準、處理費用替代，再套指定值，
之後增減並納入追加行動。第一版沒有替代或追加，實付量為：

```text
base = 同一卡表的數字印刷成本
set = 本次唯一成立的自身 set（若無則用 base）
deltas = 本次成立的自身 delta ＋ 承接後仍有效的已安裝 delta
raw = checked(set + sum(deltas))
pay_pp = max(0, raw)
```

set 不是歸零後補差，也不是覆蓋整份成本中的非 PP 行動。delta 不能逐條截零；
CR 1.3.2.2.1 的下限作用在實際打出動作，不是改寫每個中間值。零成本依
10.6.2.5.3 視為已支付，不增加 PP。安全整數／checked 算術溢位是拒絕，不能飽和成零。

不同來源的獨立 delta 各適用一次，不按值去重。所有條件／操作數不依賴修正後的費用，
第一版純非正 delta 可交換，無須詢問順序。多個 set、依賴成本的條件、會改變其他置換
適用性的形狀整卡拒絕；CR 10.10.2–10.10.2.3 的玩家排序與一次適用不能由 mapping 鍵序、
卡文陣列序或 Numeric 的 timestamp 排序代替。分段「代わりに」須先以互斥條件表達（§4），
不交一般多來源加總去猜最大減費。

普通打出只接受有非負數字印刷費用的可打出 face。缺欄、null、`-`、非數字與負值在
整卡來源／卡表驗證拒絕；不是免費卡，也不能靠 set 讓它通過。CR 2.5.1 與 1.3.6
不足以單獨證明 `-` 一定代表數字 0；這是保守准入，並非裁定這類卡永遠不能打出。
非打出用的進化 face 是否缺 Cost 資訊，不由這項 base-face 檢查推定。

### 2.3 只付一次與失敗

Spell 仍由 lowering 明示唯一固有 PP Pay，literal 必須等於同一 face 印刷費用；
這個經 authority 驗證的位置在步驟 5 改用 `pay_pp` 一次。不能改 literal、加第二個 PP Pay
或把普通 activated PP 當作卡片費用。Follower 的固有費用由 card play 程序消費同一計算；
不造空 Spell／Fanfare 來承載，Fanfare 之後另付自己的成本。

不足 PP 或付款後場位不合法時，依 play 契約回整次嘗試之前，包括公開移區、修正承接、
事件、RNG 與 pending；沒有 played 事實，不耗減費額度。錯 token 的拒絕維持原題。
計算的中間值不是事件，也不是對所有玩家公開尚在手牌的候選清單。

## 3. 追加付款的已知義務與拒絕界線

以下固定已知語義，**不開放第一版的付款型態**；尚未提供最小完整承載的形狀整卡拒絕。
不因本節描述程序而允許 executor 只做一半。

CR 10.4.7–.7.2、10.6.2.2.2：打出時的「材料：減費／設值」是同一次 play 的可選
追加付款。在步驟 2 決定付不付，早於模式、目標與步驟 5；此時只保存決定，不移材料，
不產生 PaymentComplete。步驟 5 將被選的付款行動與對應修正加入完整付款計畫，
先算成本再支付，避免先付材料來改變條件或用尚不存在的 paid 值決定減費。
拒付不套該修正；選擇支付而付不起，依 10.6.2.5.4 回整次 play 前，不能自動改為
「不付」後沿用已看過的目標，也不能保留減費而跳過材料。

必要追加沒有付不付問題；可選「+N してよい」也在步驟 2 決定。其本文只能在完整
付款後讀具名 PaymentComplete，不能讀決定布林冒充付款成功；declined 是未付款，
合法零成本／合法取代仍可完成（10.6.2.5.2–.3）。本文的 `K.pay PlayParameter`
只消費此次付款結果，不再次扣費。receipt 不在目標或成本條件的作用域內。
跨 Fanfare 等另一 execution 的讀值不能直接拿上一個 execution 的 binding；未有正式
事件承載的卡維持拒絕。

跨固有 PP、additional、optional 的實際行動次序須有 CR 10.4.2.1 及來源段落的證明。
`Cost.steps` 只保留已證明的行動序；mapping 鍵序不提供證明。替代費用選用依
10.6.2.5.1.2，不能混同 OptionalCost 的步驟 2 決定。+N 與 set／減費下限如何合成、
多組可選成本與材料選擇、付款結果決定模式數尚未准入，不能先給通用公式或借
`one_of`／`steps` 改掉各組獨立決定。付款交換的第一片必須一併解開這些實際用到的接點。

## 4. 最小內部承載與 lowering

現有 `PlaySpec {captures, parameters, costs}` 屬 ability play；`CardProgram` 目前只有
abilities，無法讓 Follower 自身的卡片打出修正與 Fanfare 分離。以下是**待實作的內部
型別增補**，不是新作者欄位或序列化格式：

```text
CardProgram.card_play_costs: List<Located<FacePlayCost>>   // absent input lowers to empty
FacePlayCost { face: FaceId, modifiers: List<Located<PlayCostModifier>> }
PlayCostModifier { condition: Option<Expr<Bool>>, change: CostChange }
```

`CostChange` 沿用 Delta／Set；第一版 amount 恰為 literal，Delta≤0、Set≥0。
同一 face 最多一筆非空 FacePlayCost；至多一個 set 宣告（即使條件互斥也先不接複數 set）。
只為 card play 求值，不為 Activated、Fanfare、進化或出走求值。Spell 同樣使用這個位置，
其 PlaySpec 只保留固有付款，不重複放自身修正。原打出區域是 authority context 的事實，
EX 限定由 lowering 明確映射，不能讓一般 Current ObjectInfo 讀到不存在的舊世代。
若現有 predicate 沒有可表示該起點的封閉讀值，EX 限定自身修正先拒絕，不另塞自由字串。

良構檢查須走新增樹、來源座標、face 唯一性、值域及條件型別。條件在新入口僅能讀
本次 controller／source 與受驗公開狀態，不繼承其他能力的 bindings；不可讀 targets、
付款 receipt、AtSelection、本文 capture 或宣告在稍後的值。資料層驗 face／印刷成本／
完整來源與 qualification，executor 再驗本版白名單；通用 WF 不冒稱已驗卡表。
capability 推導也須走到新入口（含條件不成立的分支），並以有別於安裝 CostModifier 的
內部需求碼標示；不能因內部准入已列 `effect.cost-delta` 就讓卡片自身修正通過。

lowering 將自身 cost_delta／cost_set 的條件列表保留來源位置。分段由作者資料直接寫成
互斥條件（如 SC≥15、10≤SC<15、5≤SC<10），lowering 原樣保留，不從列表推斷或改寫互斥；
獨立兩行不互斥。這些不是 InstallEffect，不配 EffectId，不污染一般 Cost 讀值。
由別張卡已執行效果給特定卡的修正才用 `InstallEffect CostModifier`，保留成功 After、
安裝來源、EffectId、期限與世代；aura 應走 Static declaration 的生命週期，本版不造它。
所有未准入作者欄位、無法完整展開的關鍵字、未知條件與不完整整卡來源在降低／准入
時拒絕，包含死分支，不靜默丟欄。沿用作者短 Cost 單操作／多操作 steps，
不變更 canonical；`cost_delta` 直接 param 白名單亦不擴張。

代表良構正反例：合成 base face，單一 set 7 加兩條常值 delta −2、−1，空條件表示 true，
合法而實付 4；改成條件讀本文 receipt 或第二個 set 宣告即拒絕，不把條件 false 當逃生門。

## 5. 代表驗收

這些是後續實作義務，非本文件已跑過的案例；只加到既有公開介面的相應測試，不建矩陣。

1. 自身條件減費：合成印刷 5、公開條件成立取 −2，另承接 EX 當回合 −1，實付 2；
   同局面搜尋／一般 Cost 讀值仍為 5。已付款後的條件變動不得重算；不重複扣固有 PP。
2. 分段：借 BP01-061 的片段，墓地 10 張 Spell 只減 6；兩個獨立成立的 −1 則都算。
   不以此宣稱整卡的【突進】已准入。
3. 合成 set／delta 用 §4 正例；單一 delta 令 raw<0 時付 0，不回復 PP。
   PP 不足或付款後場滿沿既有 play rollback 案例驗一次，不再複製所有付款型態。
4. 整卡含任一未准入付款（以 BP07-024 的片段為種子），在問目標、移區或扣 PP 前拒絕；
   非數字 base Cost 同樣拒絕。來源與 qualification 不完整仍按既有閉包拒絕，
   不因合成機制測試成功宣稱真卡已採納。
