# 數值修正、排序與期限

本文件補充 [執行語義 §5](runtime-semantics.md#5-numericusage-與-timestamp-的首批範圍)，
定義 executor 必須滿足的契約；不是現有執行器已完成的功能表。
公開 program、capability、最低版本及作者語法均不在此變更。
IR 型別／良構的接受不代表 runtime 准入，現行限制仍見 [內部執行](internal-execution.md)。

規則依據為日文 [CR 1.28.0（2026-09-28）](https://shadowverse-evolve.com/wordpress/wp-content/uploads/2026/09/28121513/ShadowverseEVOLVE_cr_1.28.0_260928.pdf)，
PDF SHA-256 為 `c44b8f2ff7e1019bc2b4ec59df894d6a067179ff05eac2542b7723648a22cd47`。
卡文與 Q&A 可由已審 `card-data/jp/` 的來源觀測核對。下文區分官方規則、由規則推得
的 `generalized` 與既採用的專案解讀 `inferred`；內部欄位不是官方遊戲術語。

## 1. SetLife 與受傷量

[BP16-049 スノーアウェイク](https://shadowverse-evolve.com/cardlist/?cardno=BP16-049)
的 Set 1／1 指定的是目前攻擊力及體力。CR 2.8.2、5.14.1 的傷害減少體力，
5.27.2 的指定值變更比較前後數值，沒有「先把指定值扣一次舊傷害」的指示。
因此基礎攻體 2／3、已受傷 1、目前 2／2 的目標，Set 後是 **1／1**，不是 1／0。
「已受傷後 Set 的目前值等於指定值」有官方 Q&A：
[BP20-058 侮蔑の継承者・アジュラフリート](https://shadowverse-evolve.com/cardlist/?cardno=BP20-058)
Q2659 在同一能力內先受傷至體力 0，再「これの体力を7にする」，之後的 check timing
體力為 7、不破壞。下述 hp／max_hp／D 的拆分仍是本專案表示，屬 `generalized`。
Q2104 只證明原已為 1／1 仍能繼續後文。SpellChain capture 與 RemoveAbility 未接好時
BP16-049 整能力仍拒絕。

引擎使用 `hp`、`max_hp` 與受傷量的表示時，須維持以下內部不變量：

- `D = max_hp - hp` 是受傷量；Add／Set 不冒充 Damage 或 Recovery，不清掉 D。
- 對該主體安裝 SetLife(S) 時，固定當時 D 為 `D_set`；持續層的無傷基準設為
  `S + D_set`。後續有效體力是按規則排序得到的無傷基準減去**目前** D。
- 例中 Set 後 `hp=1, max_hp=2, D=1`。這裡的 max_hp 是計算基準，
  不是 CR 定義的體力上限，也不是卡片印刷體力。不能把 hp、max_hp 都寫為 1。
- 之後再受傷 1，`D=2, hp=0, max_hp=2`；重讀不能重新捕捉 D_set 把 hp 恢復為 1。
  Set 到期而沒有其他修正時，回到 `hp=3-2=1, max_hp=3, D=2`。
  是否已在到期前被破壞，另外依 §3 的檢查時點決定。
- AddLife(N) 對無傷基準加 N，D 不變；SetAttack 不涉及 D。所有算術 checked，
  溢位完整 rollback，不能飽和、截零或回印刷值。

這是本專案採用的內部表示契約，**不是官方另有 max_hp 或傷害標記規則**。
也可採等價的直接體力表示，但必須得到相同的當前值、後續傷害、期限還原與事件。
安裝時不得把 Set 改成永久 Add(S−before)：較早的修正到期時，Set 仍須在自己的
排序位置指定值（10.9.1.4–.6）。進化是例外，依下節。

### 1.1 進化時的差額

CR 5.16.2.1：進化前的攻擊力或體力與進化前的元の値不同時，把差額加減到進化後
卡片的表記值上。官方 Q&A 直接用於 Set：
[BP14-076 寒冷の死霊術師](https://shadowverse-evolve.com/cardlist/?cardno=BP14-076)
Q1923 把 0／1 的
[BP12-072 モーターグレイブディガー](https://shadowverse-evolve.com/cardlist/?cardno=BP12-072)
Set 為 1／1，之後進化（表記 1／2）為 **2／2**，不是 1／1；
[BP15-014 不殺の隠者](https://shadowverse-evolve.com/cardlist/?cardno=BP15-014)
Q1983 也以同一條回答 Set 體力 1 後的進化。因此 Set 有效的主體進化後，
當前值為「進化後表記值＋（進化前當前值－進化前元の値）」，Set 不再把值指定回 S。

只有上述結果有官方依據。持續中的 Set／Add 在進化後是否改為差額、之後到期或
條件翻轉時如何還原、與受傷量 D 的拆分如何換算，CR 5.16.2「效果持續適用」與
5.16.2.1 的關係沒有足夠卡例，列為規則疑義。executor 在 Set 層有效時，對該主體的
進化與其他基礎替換整棵拒絕；不能只刪原型 decide guard，也不能挑一種表示放行。

`I.modify` 在第一個主體被修改前固定全部 subjects／數值運算元／每主體 D_set，
依規則排序安裝，再以指示前後的完整有效值計算事件。S 是捕捉的非負操作數，
**不保證**最終有效值為 S：較晚 timestamp 的 Add／Set 仍可覆蓋它。
同指示同主體的非零 Attack／Life 合為一事實，零差不生 facet；兩指示仍是兩事實。
重算、到期、條件翻轉不另發 stat_changed，沿用
[GAP-S-008](ir-vocabulary-1.0.md#gap-s-008) 的 `inferred` 事件政策。

## 2. ProperPlay、層次與依存

CR 10.6.2.7 是適正 play 事實的提交點；選目標、付款途中或失敗的 play 都不能產生
可用的 ProperPlay。authority 為此事實保存單調的規則時間及 ExecutionId 關聯。
CR 10.9.1.6.2 的非 static 排序使用該時間；EffectId 則在成功安裝時分配。
兩者不得相互推導。來源先付款離場仍可解算（10.6.2.8.2.1），例如
[CP02-007 ブランニュー・ビート](https://shadowverse-evolve.com/cardlist/?cardno=CP02-007)。

合成 closed Program 的可信入口必須提供 authority 所持的 execution／適正 play
關聯及時間；不能由玩家回答、EffectId、陣列索引或本次讀取時間造 timestamp。
同一次 execution 的多個修正共用 ProperPlay 時間；重算、save／restore 不前進它。
缺關聯、未完成 play 或跨 execution 冒用應在整棵准入時拒絕，包含死分支。
失敗安裝不得留下 EffectId、修正、事實或 tie 回答；預演不消耗任何身分。
static 使用目前區域入區時點（10.9.1.6.1），不能借用此本文入口。

求值先基礎資訊（含進化、cost 例外），再能力、非數值、數值（10.9.1.1–.4）。
Attack／Life 的 Add 與 Set 同層，不能固定「先 Set 後 Add」或以安裝順序排序。
同層先比較 A 的套用是否改變 B 的適用對象或操作（10.9.1.5）；
單純都寫 Life 或 B 寫在 A 後面不構成依存。無依存才按 timestamp。
相依循環沿用 [IR §14](ir-vocabulary-1.0.md#14-持續效果與使用額度補充)：
按時間各套一次、重算剩餘依存（`generalized`），不求數值固定點。

遞迴 Combined 是同一指示／效果的 patch 集合，不自行產生新 timestamp 或事件：
不同 stat 的 patch 可合併，同 stat 純 Add 可 checked 加總。首批 runtime 對同一
Combined 中同 stat 的 Set 與其他寫入，只有可證明可交換時才准入（例如相同 Set）；
不可交換時整棵拒絕，不把 Vec 順序冒充卡文先後。卡文的先後兩次指示須保留兩個 Modify。
這是 runtime 限制，既有良構允許非空純 Numeric Combined 的結構不變。

不同效果同 timestamp、無依存且不可交換時，沿用 `ContinuousTieOrder`：回合玩家
的效果先，同玩家選序且保存，重算不重問，新增部分保留已定相對序。
這仍是基於 CR 1.3.4.1 類推的既有 `inferred`，沒有新增官方裁定。
尚未接好規則輸入程序、固定候選／actor／epoch、保存及 rollback 的 runtime，
整棵拒絕可能到達的非交換 tie；不能以 EffectId、source ID 或字典序補序。
不同 stat 或已證明可交換的純 Add 不需產生無意義的詢問。

## 3. 解算與回合期限

期限錨與排序時間獨立：排序取 ProperPlay，Installation 期限取成功安裝時的
authority 時間、當前 TurnId 與解算 execution。PlayerRef 在安裝時解析成玩家並保存；
`controller_of` 不是 owner，也不隨之後控制權轉移重新解析。
所有時點由同一回合／phase 排程供應，不建另一套「剩餘回合數」。

### 3.1 CurrentTurnEnd

綁定安裝所在的實際 TurnId，在該回合 **CR 7.4.8** 移除同批到期效果。
不是 7.4.1 的 end phase 誘發、7.4.5 的 Quick 窗口或開始下一回合時才清。
7.4.7 因棄牌再進 check timing 時仍有效；其後才清到期效果並重算一次完整有效狀態。
同批清理不能逐項造成虛構 stat_changed 或中途致死。

CR 11.1.2 只在 check timing 處理致死；7.4.8 沒有額外 check timing。
因此清理令 hp≤0 時，不得在此插入破壞；照 7.4.9 進入實際下一回合，
完成 7.2.1–.4 後，在 7.2.5 檢查並先做規則處理（10.5.2.1），才處理待機能力。
此順序為上述 CR 的 `generalized`，不把「到期」做成額外玩家行動。
期限過後安裝不能追認到已過去的 7.4.8；若內部合成入口允許在此空隙介入，該入口拒絕。

真實期限種子：[CP04-060 カヤ](https://shadowverse-evolve.com/cardlist/?cardno=CP04-060)
的本回合減費；它不證明同卡有加攻體，也不把減費視為 Numeric。
首批 Numeric 的 CurrentTurnEnd 局面是共用期限的合成驗收。

### 3.2 NextTurnEnd 與 edge 的差別

`NextTurnEnd(P)` 表達「下一個 P 的回合結束前」：先找**開始時點嚴格晚於安裝錨點**
的第一個實際 P 回合，再綁其 7.4.8。P1 自己的回合 T 安裝時不選 T 的結束，
選下一個實際 P1 回合 U；P2 回合安裝時則選接下來第一個實際 P1 回合。
期間從安裝起連續有效，包含中間其他玩家的回合。

依據是「次の…ターン」卡文、CR 5.26.2 的下次開始回合與 7.4.8 的結束清理，
屬 `generalized`；[BP11-018 カースドクイーン・ナハト・ナハト](https://shadowverse-evolve.com/cardlist/?cardno=BP11-018)
及 Q1501、Q1717–1718 提供 next player turn／到期還原的種子，沒有直接裁定在該玩家
當前回合安裝的 Q&A。它的 BOX／能力層未由本數值片段准入。
已審 card-data 中「次の…ターン終了時まで」只有此卡（含再錄），且它在自己的主要階段選對手的
從者，P 必為非回合玩家；目前沒有真實卡會在 P 自己的回合安裝 NextTurnEnd(P)，
T-B 只作合成驗收。

`NextTurnEdge(P, End)` 則是嚴格晚於錨點的第一個 P 結束 edge，依
[GAP-S-006](ir-vocabulary-1.0.md#gap-s-006) 已有 edge 契約；P 目前正在進行的回合
尚未到結束 edge 時可以命中。因此兩者不能當 alias。未能從卡文確定是「下一回合」
還是「下一個結束時點」的 lowering 必須拒絕，不能挑較好實作的一個。

### 3.3 EndResolution

此期限綁安裝所在的那次解算，不是每個 instruction、Combined leaf、Repeat iteration
或待答之前。內部完成點置於本文全部指示與 10.6.2.8.3 的解決區清理之後，
返回呼叫端的 check timing **之前**：到期清理 → 有效值重算 → 呼叫端 check timing。
不能先用尚未到期的數值判死，再移除 EndResolution。

CR 10.6.2.8、7.3.4／7.4.6、10.5.2.1、11.1.2–11.3 提供這個先後關係；
EndResolution 是本專案內部期限，該映射為 `generalized`，不是聲稱 CR 有同名步驟。
來源／目標換區失效仍可更早發生。待答、resume 與 save 不代表結束；rollback 撤回整次
安裝及其期限，不能留下已耗用的 occurrence。nested play 未有完整 execution 邊界時拒絕。

真實檢查時點種子是 [BP10-076 《節制》・ルーゼン](https://shadowverse-evolve.com/cardlist/?cardno=BP10-076)
Q1327：同一能力前段致死不讓後段提早失去其保護；BP20-058 Q2659 則是同一能力中
體力曾為 0、後段 Set 為 7 而不破壞。兩者都 **不含 Numeric EndResolution**。
目前沒有已核對的真實卡文要求 Numeric EndResolution；允許用合成 closed Program
驗證以上內部生命週期，但 lowering 不得替未寫期限的 buff 自動填入 EndResolution。

### 3.4 區間、跳過與額外回合

CR 3.3.2.1–.2、5.26、5.28：跳過的回合沒有 start／end occurrence，額外回合有。
未開始的預定回合不是已發生的 TurnId；查找下一實際回合必須隨排程跳過與插入調整。
一旦開始便保存實際 occurrence，之後不重新挑選；同玩家連續兩個額外回合只命中第一個。
多個 skip／extra 的消耗次序照 5.26.2.1、5.28.1.1，不由修正層另行決定。

Duration 的主區間是安裝至 end；`intervals` 是**附加區間的聯集**。
`CurrentTurnEnd + [DuringNextTurn(P)]` 是當前回合餘段加下次 P 整回合，
中間其他回合無效；不是一路延長到 P 結束，也不是兩區間的交集。
下一區間仍有未來機會時，主區間結束僅停止適用，不能刪掉整個效果實例。
NoTemporalExpiry 已覆蓋未來，附加區間不會令它變成「只在下回合」。
若只需 DuringNextTurn／DuringNextPhase，必須先有已定義的空主區間承載；本片
不以 NoTemporalExpiry 猜代，該形狀維持 runtime 拒絕。

DuringNextTurn(P) 綁下一個開始晚於 anchor 的 P 回合，開始時有效、7.4.8 清除。
DuringNextPhase(P,F) 同理選下一個實際 phase 的 begin，整個 phase 有效；
不能在 begin 就移除。尚未有可驗證的 phase begin／end 排程時拒絕。
安裝後 target 世代失效不會在後一區間復活；來源離場是否影響另依 static／SourceZoneExit，
不能因暫時不在區間就換 source、EffectId、timestamp 或重新掃描目標。

真實種子：[BP05-061 カースドストーン](https://shadowverse-evolve.com/cardlist/?cardno=BP05-061)
Q718 的當回合加下個對手回合（非連續區間）；
[BP08-024 不滅の聖剣・デュランダル](https://shadowverse-evolve.com/cardlist/?cardno=BP08-024)
Q1066 是官方直接例：「このターンと次の相手のターン中」遇自己連續回合時，第二個自己回合
不適用，之後的對手回合適用（該卡為能力授予，不是 Numeric）；
[BP05-006 マインドルーラー・モートン](https://shadowverse-evolve.com/cardlist/?cardno=BP05-006)
Q672–674 的下個整階段；[BP05-086 安息の絶傑・マーウィン](https://shadowverse-evolve.com/cardlist/?cardno=BP05-086)
Q747 的跳回合；[BP01-057 次元の超越](https://shadowverse-evolve.com/cardlist/?cardno=BP01-057)
的額外回合。均只提供時間語義種子，不宣稱相關整能力已支援。

## 4. 來源讀值與其他修正

### 4.1 離場不是 AbsentByRules

CR 10.11.1 要求效果引用已離開原區域的卡時取最後資訊；10.6.2.8.2.1 保留能力解算。
因此 Current SourceHolder 世代已失效不是「該卡依法沒有 Attack／Life」。
`InformationOrZero` 只能把確定的 AbsentByRules 轉成 0（CR 1.3.6.3）；
未知 read、缺來源、錯世代或未支援 LKI 都必須拒絕，不回 0，也不追到墓場的新物件。
需要最後資訊的 lowering／executor 未接好時整棵拒絕該依賴，包含死分支。
已捕捉數值的 CP02-007 修正仍留在目標，與之後 live read 是否可行是不同問題。

真正 AbsentByRules 的對照是 [BP05-001 不殺の絶傑・エズディア](https://shadowverse-evolve.com/cardlist/?cardno=BP05-001)
Q660：同世代變為護符後保存原數值但依法不能參照 Attack／Life（CR 5.25.2.1）。
直接資訊取得按 1.3.6.3 轉 0；比較條件依 1.3.6.2 為 false，不先轉 0 再比較。
這不准入 SetKind executor，只確定與無法解析物件的差別。

### 4.2 Continuous 與 SetKind 的准入邊界

本文 InstallEffect Continuous 在安裝時固定 affected；條件及 live 操作數每次純求值
按同一有效特徵管線求值，read/write 只用來縮小依存檢查，不能直接當成依存關係。
條件為 false 仍保存實例，條件恢復可再適用；對象新入場不補入固定集合。
安裝、翻轉及重算都不生 stat_changed；Modify 則固定操作數且明示產生實際非零 delta。
來源有效區、條件與目標世代是三個獨立限制（CR 10.3.2–.5、10.9.2；GAP-S-008）。
沒有證據的自我依存 live SetLife／受傷補償、多次重算捕捉、未實作非數值依存一律拒絕。
本片不擴大既有 Continuous 的 runtime 支援。

SetKind 必須先於 Numeric，保留其他資訊、能力與物件世代；不隱含 RemoveAbility 或
Grant。BP05-001 的 Q658–669 與 CR 5.25 支持此邊界。所有合法性、讀值、傷害接受者、
規則致死與 view 必須一起改用有效種類後才能准入；不能只讓畫面顯示護符。
Grant 依賴未實作前，BP05-001 整能力仍拒絕。

## 5. 可執行局面驗收清單

下列是給 executor 的驗收義務，**本文件沒有執行這些局面**。合成輸入使用可信
closed Program，真實卡只借指定片段；兩席互換再跑。共同斷言 effective read、合法性、
事件 snapshot、view 一致，以及拒絕後狀態／RNG／身分／pending／排程完整 rollback。

| ID | 輸入與期望輸出 | 反例／拒絕條件 |
| --- | --- | --- |
| N-A | 基礎 2／3、D=1，BP16-049 Set 1／1：hp=1、max_hp=2、D=1；Attack −1、Life −1 同一事實 | 不得 hp=0、清傷、兩個 fact；負 Set、不能證明非負的動態 Set 拒絕 |
| N-B | N-A 後再傷害 1：hp=0、max_hp=2、D=2；重讀不恢復；合成到期在 check 前則 hp=1、max_hp=3 | 到期不能再發 stat_changed；先經 check 已破壞者不能因到期復活 |
| N-C | 基礎 2／3、D=1，較早 play 的 Add 1／1、較晚 play 的 Set 1／1：1／1；兩者 play 先後對調：2／2，D 均為 1 | 不得固定 Set／Add 分層；對不同 D 的兩目標不能共用 D_set |
| N-D | 基礎 Attack=2，play A(Set 1)早於 B(Add 2)，即使 B 先安裝，最終 Attack=3 | 用安裝順序會得 1；來源無合法 ProperPlay、外來 execution 拒絕 |
| N-E | 同 timestamp Set 1 與 Add 2，回答 Set→Add 得 3、Add→Set 得 1，保存後讀取不重問 | 無 tie 輸入程序整棵拒絕；錯 actor／epoch、漏項、重項、重播不改狀態 |
| N-F | 遞迴 Combined 的 Attack Add 1、Life Add 2 同一 fact；純 Add 同 stat 可合併 | 空 Combined、混層由良構拒絕；同 stat 非交換 Set／Add 在 runtime 拒絕 |
| N-G | 基礎 Life=3，較早 Add 2 及較晚 Set 1；前者到期後仍為 1；Set 到期後回 3 | 安裝時不能永久儲存 Set 當時的差值代替指定值；兩個零差 Set 不造事件 |
| N-H | 規則 fixture：BP14-076 Q1923，0／1 Set 1／1 後進化（表記 1／2）為 2／2 | Set 層有效時 executor 對進化／基礎替換整棵拒絕並完整 rollback；不得得 1／1，也不得只刪原型 guard |
| T-A | CurrentTurnEnd 在 end phase 誘發、Quick、棄牌 check 仍有效，7.4.8 才消失 | 不得在 7.4.1、7.4.5 或下一回合才清；到期無 stat_changed |
| T-B | 合成：P1 回合安裝 NextTurnEnd(P1)，P1 本回合結束仍有效，下個實際 P1 的 7.4.8 到期 | 同錨點 NextTurnEdge(P1,End) 可命中本回合，兩者不可混同 |
| T-C | P1 安裝 NextTurnEnd(P2)，插入 P1 額外回合再跳過預定 P2 回合，等真正 P2 開始後的 7.4.8 | 不得全域 turn+1／+2，也不得 skipped 回合造 start／end |
| T-D | CurrentTurnEnd＋DuringNextTurn(P2)：本回合有效，中間 P1 額外回合無效，真正 P2 回合恢復（Q1066） | 主區間到期不能刪待來區間；target 換世代後不能恢復 |
| T-E | 基礎 Life=1，合成 EndResolution AddLife(−2)：本文可見 −1，解算清理後回 1，check 不破壞 | 不得每 instruction 檢查致死；無期限 −2 在後續 check 必須破壞 |
| T-F | 基礎 Life=1，EndResolution AddLife(2) 後傷害 2：本文 hp=1；到期 hp=−1，check 破壞 | 不得先檢查存活再忘記到期後重算；pending 問答期間不可提早到期 |
| T-G | CurrentTurnEnd 到期後 hp≤0：先下一實際回合的 start 程序，再 7.2.5 破壞 | 不得在 7.4.8 插 check；next start／turn trigger 的待機與解算分開驗 |
| T-H | 安裝時 P2 為 target controller，之後轉交 P1，仍等待所捕捉的 P2 occurrence | 不得用 owner 或每次 live controller；缺排程能力整棵拒絕 |
| R-A | 合法已捕捉值來源離場，修正仍在；Current SourceHolder live read 需 LKI | LKI 未接整棵拒絕，不能 InformationOrZero=0、追新世代或取印刷值 |
| R-B | 合法同世代護符的 Attack／Life 為 AbsentByRules，直接取值 0、比較 false | 不是缺來源回 0；SetKind／Grant 未准入時先拒絕整能力 |

負 Set、Modify usage、未定義的來源實例／EffectId producer、RuleCarry、Grant／RemoveAbility
與跨層 Continuous 仍依既有界線拒絕；不能以新增期限契約繞過整棵預檢。
