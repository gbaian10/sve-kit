# 已選目標的執行檢查與進化數值承接

本文件補充 [play](play-semantics.md)、[執行語義](runtime-semantics.md)與
[數值時序](numeric-timing.md)。這是 D1 的實作義務，不宣稱現行 executor 已完成；
[內部執行](internal-execution.md)仍記錄已驗子集。不變更作者語法、公開 program、
capability、最低版本或正式整卡來源資格。

依據為日文 [CR 1.28.0（2026-09-28）](https://shadowverse-evolve.com/wordpress/wp-content/uploads/2026/09/28121513/ShadowverseEVOLVE_cr_1.28.0_260928.pdf)，
SHA-256 `c44b8f2ff7e1019bc2b4ec59df894d6a067179ff05eac2542b7723648a22cd47`。
卡例與 Q&A 核對 `card-data/jp/` 的已審觀測；以下區分直接規則、`generalized` 推導、
`inferred` 專案解讀與支援限制。沒有直接依據的條件翻轉與 Set 還原不作假裁定。

## 1. 範圍與選擇條件

首批為公開場上的從者目標、同次 play 的單一 selector group、已受驗基數與分配，
新增可證明不變的種類／タイプ條件。種類與タイプ都讀有效特徵視圖中的值，不能直接讀
印刷卡表來迴避 continuous。數值、姿態、進化狀態、名稱、能力、歷史、聚合與跨目標
關係的選擇條件先不開放；少見的群組、roles、隱藏區選取另依各自契約。

| 條件位置 | 時點與意義 | 第一版處理 |
| --- | --- | --- |
| 選擇句的區域、玩家、kind／trait | CR 10.6.2.3：付款前共同選擇 snapshot | 驗完整候選與答案；後續依 §2 檢查 |
| 本文「それが…なら」 | CR 10.6.2.8.2：到該句才判斷 | 保留本文 If，不上提成選擇 filter |
| 「選んでいたなら」及明示捕捉 | 讀原 AtSelection 值 | 不重寫捕捉值，不重新選牌 |
| Damage／Destroy 等動作自身條件 | 執行該指示的當前狀態 | 每次重驗世代、可作用區域與該動作條件 |

[BP11-P31 従順な駿馬](https://shadowverse-evolve.com/cardlist/?cardno=BP11-P31)
Q1585 允許先選非荒野從者，付送墓成本後才因本文條件不符而不加攻體。
它證明選擇 filter 與本文條件不能混用，**沒有**裁定選擇後失去 trait 的情況。
[CP03-098 アビス・フリーザー](https://shadowverse-evolve.com/cardlist/?cardno=CP03-098)
Q1433 另確認不能選付款才捨到墓地的卡；它只證明選擇早於付款，不授權墓地目標或重選。

CR 10.6.2.3 與 .8 沒有明文要求在結算重新執行整個選擇述詞；不能把
「現在無法執行某動作」（1.3.2）擴寫成「所有原選擇條件不成立都失去目標資格」。
第一版只在兩種解讀結果一致的子集准入：仍在場的原世代，從共同選擇 snapshot 到
每個使用點，區域玩家限制及 kind／trait 的讀值不變。這是**支援限制**，不是官方
認定種類／タイプ永遠不重驗。會改變這些資訊的能力、付款、本文、取代或規則程序，
在預檢整次拒絕；不能先付款，直到 predicate=false 才當作正常略過。

穩定性可由已准入閉包證明：只讀固定玩家的 Field，沒有控制轉移、種類／特徵修正、
改變相關印刷資訊的進化、static 或未受驗 continuous；Draw、數值 Add、Damage
不改 kind／trait。一般移區可以存在，由世代檢查處理；後續重入的新物件不能替代。
不要求建立通用依存分析器；無法用現有支援白名單證明時拒絕。只讀候選當下為 true
不能代替這項證明；預檢不因這次沒選到某物件而放行未受驗的效果管線。

## 2. 每個指示的檢查與結果

輸入是同一 execution／scope／decision 的原始已選 ObjectRef 集合、選擇 snapshot、
已驗 selector 定義及該指示的當前 authority 狀態。原 binding 與分配答案保持不變。
每個使用該目標的指示開始，先固定該指示的完整輸入，再逐項判斷：

1. 原世代已離開場或失效：不能對新區域／新世代操作（CR 4.1.4、4.4.3）。
   離場再返回也不重綁；LKI 只供資訊，不能恢復可作用目標。
2. 原世代仍在場：核對 §1 的穩定性前提，讀當前有效資訊並驗動作自己的合法性。
   同一進化不換世代（5.16.2）；但若它可能改變選擇條件，仍受 §1 的准入限制。
   場到另一玩家場的既有例外不能誤當普通新世代；控制轉移情境首批拒絕。
3. Damage 只能作用於從者／主戰者（5.14.1–.1.2）；Destroy 只作用於場上卡（5.6.1）。
   禁止先於取代（1.3.3），其他動作照各自已受驗契約。不是重新跑玩家選擇程序。

輸出區分原選擇、該指示可作用成員與實際結果。某些成員不能執行時只處理其餘成員，
全部不能執行時該指示無實際作用，仍執行獨立後句（1.3.2、10.6.2.8.2，`generalized`）。
不退款、不取消已成立的 play、不補選、不補分；後句若以 receipt 成功為前提則依法判 false。
依 [runtime §1.3](runtime-semantics.md#13-damage-requestedactual-投影)，原已選目標失效
仍計入 Damage requested，actual 只計實際傷害；Movement 成功 before／after 亦不含失效者。

不能全域刪掉 binding 中的失效成員，否則會改變選擇計數、AtSelection 與 requested。
也不能在解算入口只檢查一次：前一指示的移動會影響後一指示。一次同時指示的全部
成員在同一開始狀態求值，不能逐項重選；本片不改既有 simultaneous、receipt 與 check timing。
來源離場不使本文取消（10.6.2.8.2.1）；不插入 Quick 或其他待機能力回應窗口。

若遇到應已由預檢排除的條件翻轉、缺 producer、未知資訊或不支援的禁止／取代，
回傳 Unsupported／內部錯誤並回復整次 origin，不能當作遊戲中的不適用結果。
舊 token 不復活；錯 actor／token 的答案仍留原題，沿用 play-semantics 的交易界線。

## 3. 純 Add 跨進化與到期

### 3.1 可開放的子集

首批只開放固定對象、安裝時已捕捉操作數的 Numeric Add（Attack／Life），
其已受驗期限限 NoTemporalExpiry、CurrentTurnEnd、NextTurnEnd；無附加 intervals、
條件翻轉、live 操作數、static、Set、來源離區期限或其他基礎替換。既有合法的普通／
超進化程序可使用本節；不擴大效果本文 Evolve、退化、換面、操縦或種類轉換的准入。

直接規則：5.16.2 保留同一張卡及既有效果；5.16.2.1 承接與進化前元值的差額；
10.9.1.1.1 改用進化面基礎資訊、費用除外；7.4.8 移除到期效果。
[BP01-004 リノセウス](https://shadowverse-evolve.com/cardlist/?cardno=BP01-004)
Q152 直接確認無期限加攻承接；進化當下的數值由 5.16.2.1 唯一決定。

限期 Add 跨進化後的到期還原沒有直接 Q&A，屬本專案解讀，標為 `inferred`；
不用 Q152 冒充完整的限期卡例。5.16.2 的效果持續與 5.16.2.1 的差額若照字面各加一次，
Add 會被重複計入；Q1923／Q1983 只回答 Set 進化當下的值。現有條文與裁定不足以排除
其他承接／還原解讀，例如差額在進化時固定、原 Add 到期不再改變數值。本片採「原 Add
繼續作為效果存在、7.4.8 到期後移除其貢獻」，與期限規則一致且可回頭。日後取得不同
裁定時，須同步修訂本契約 §3、§5 驗收期望、numeric-timing §1.1 與 D1 實作及測試。
沒有明示期限的卡文仍降為 NoTemporalExpiry。

### 3.2 計算與保留的身分

沿 numeric-timing 的表示，B 是進化前印刷基礎，E 是進化面基礎，M 是尚有效的 Add
之和，D 是目前受傷量。純 Add 子集的等價式為：

```text
進化前 attack = B.attack + M.attack
進化前 hp     = B.life + M.life - D
進化後 attack = E.attack + M.attack
進化後 hp     = E.life + M.life - D
```

進化只替換基礎；不把有效值或含 M 的總差額再次寫回基礎。不複製修正、不把限期
Add 烘焙成永久差值、不清除傷害。每個 EffectId、目標世代、來源、ProperPlay timestamp、
已固定的玩家／TurnId／期限錨均原樣保留；進化不重新安裝，也不延長或提早到期。
期限到來移除該效果後，從**進化後**基礎與仍有效的其他 Add 重算，保留當時 D；
不是恢復進化前數值，也不是把當前 hp 與 max_hp 一起重設。

§3.1 的純固定 Add 可交換，不引入新 tie 選序；timestamp 仍按 numeric-timing §2
保存，未來安裝的 Set 不得因前面曾進化就失去原排序。進化完成後才新安裝 Set 的
非進化行為仍依原規格；只要 Set 仍可能適用，就不能再通過本節的進化入口。
超進化的明示 +1/+1 與原期限 Add 分開保存，不能在原期限結束一起扣掉。
基礎進化、到期與重算不另發 stat_changed；超進化明示加值沿 GAP-S-008 發事實。
checked 算術及到期後致死的 check timing 完全沿用 numeric-timing §1、§3。

### 3.3 Set 的保守界線

[numeric-timing §1.1](numeric-timing.md#11-進化時的差額)的 Q1923／Q1983 只確定
進化當下的差額結果。它們沒有回答多層 Set／Add 之後個別到期、傷害改變、條件暫停
再恢復的還原。CR 5.16.2 的效果持續與 10.9 的重新求值還不足以唯一決定實作表示。

因此含 Set 的數值實例，只要尚未永久終止、未來仍可能適用，就對該主體的進化拒絕，
即使現在被較晚修正遮住或條件為 false。預檢必須早於支付進化成本／消耗 EP、SEP／
移動進化資源；若後續發現未受驗交互，完整 rollback。不得把 Set 永久改成安裝時
Add，也不得進化時刪 Set 或重抓 D_set。Set 已確實到期、無未來區間時才可按純 Add
子集判定。此拒絕是 executor 支援界線，不是說遊戲規則禁止該從者進化。

## 4. D1 接點與 lowering

| 接點 | 要接的輸入／輸出 | 拒絕與解鎖範圍 |
| --- | --- | --- |
| `game/internal/card/selection.rs` 的 selector gate、`TargetWait`／binding | 保存既有選擇 producer 與 selector；固定 kind／trait 的有效視圖 | 解除恆真 predicate 限制只到 §1 穩定子集；保留 group／roles 等原限制 |
| 目標的 Damage／Modify／Move 消費端 | 原已選集合 → 當次可作用集合 → 原有 receipt／facts | 依 §2 逐指示檢查；不能過濾原 binding 或 requested，也不能把未知當 false |
| `game/internal/numeric.rs` 的進化 guard、base 更新與期限清理 | 原基礎／D／effects＋進化面 → 新基礎、原 effects／期限 | 只解除 §3.1 純 Add 的限期 guard；Set 與其他替換繼續拒絕 |
| 正式 card／ability play 與規則進化入口 | 全能力閉包、來源證據、成本與外層 origin | 上述子集成功不等於 C1 qualified；預檢與 rollback 不能只包內層本文 |

不需要新增 IR 型別或公開欄位。`Decision`、`SelectionConstraint`、`Selector`、
`CharacteristicView`、`Binding` 與 `Duration` 足以承載；runtime 的穩定性證明不由
caller 自填布林或 capability 提供。通用良構不冒稱已驗「結算時條件不變」。

lowering 接續 `ir/lowering/play.rs` 的 target 產生點，將作者已存在的種類／タイプ
條件保留為 typed predicate 與來源座標，不再用恆真域或列舉當下卡號替代。連同
`ir/lowering/ast.rs` 的受限輸入檢查一起更新；既有未支援欄位不可靜默忽略。
本文 If、AtSelection、原值／有效值及原期限維持原位置；不把全部 selector 都當作
要在結算重跑的 query。lowering 能表達與 runtime 可准入是兩個判斷。

代表正例為場上從者＋固定 trait 的 target，成本只動來源、本文不改 kind／trait，
輸出保留原 binding 的 typed predicate；反例為同一選擇後可能改 trait 再用目標，
良構可以接受但 runtime 必須在付款前拒絕。不得為了放行反例丟掉 predicate。

## 5. 最小驗收

以下是未來 D1 的行為驗收，不是本文件已執行的測試。沿用已有場景，避免矩陣：

1. 一個合成場上 trait 目標：非匹配者不能答、匹配者能答；付款使已選來源離場後，
   該成員無實際作用，獨立抽牌後句仍執行。以既有分配 D-E 驗部分失效者保留原量，
   不另建另一套分配測試。條件可翻轉的同型能力在預檢拒絕，不扣成本。
2. 合成基礎 2／3、D=1、CurrentTurnEnd Add +2/+2，進化面 3／4：進化前 4／4，
   後 5／5（max_hp=6、D=1）；再受傷 1 後 hp=4；原回合 7.4.8 到期後 3／2
   （max_hp=4、D=2）。EffectId／timestamp／原期限不變，不因進化或到期另發數值事件。
   同一局面把 Add 換成 Set 則在進化付款前拒絕，原局面與資源保持不變。

Q1923 的 0／1 Set 1／1 → 進化表記 1／2 得 2／2 仍只作規則參照，不冒充已准入
的成功測試。NextTurnEnd 沿既有期限驗收，毋須重建跳過／額外回合交叉格。
