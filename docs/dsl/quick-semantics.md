# Quick 許可與規則窗口

本文件定義內部 typed IR 的執行義務；已驗範圍仍以
[內部執行入口](internal-execution.md)為準。不變更作者語法、公開 program、capability
或最低版本。卡片 Quick 與起動能力 Quick 是不同許可，不由作者容器的位置推斷。

依據為[日文 CR 1.28.0（2026-09-28）](https://shadowverse-evolve.com/wordpress/wp-content/uploads/2026/09/28121513/ShadowverseEVOLVE_cr_1.28.0_260928.pdf)，
核對日 2026-10-23，以下條號均指此版。IR 承載、token 與交易是專案表示約定。

## 1. 首批範圍與拒絕邊界

CR12.3.2 的獨立 Quick 允許打出該卡；12.3.3 的起動前 Quick 僅允許打出該能力。
有 Quick 起動的從者不因此能在對手回合入場，卡片 Quick 也不使所有起動能力取得 Quick。

| 形狀 | 首批執行義務 |
| --- | --- |
| 無條件、印刷的卡片 Quick，普通 Spell | 自己的 Hand／EX，走完整 Spell play；如 BP01-007、BP01-024。整棵本文與其他宣告仍須各自准入 |
| 無條件、印刷的 Quick 起動，普通場上從者 | 自己控制、當前 Field 世代的指定 Activated，沿既有成本與本文子集；代表為 BP02-027，不擴張起動有效區 |
| 條件取得、授予或失去 Quick | 拒絕；例如 BP12-058 的條件 Quick 不可降成無條件，也不可丟掉條件後允許 main |
| 其他卡種／區域／來源 | Quick 護符、token Spell、エボルヴ等特殊面、手牌／墓地起動、Granted origin、效果巢狀 play 保持拒絕 |
| 其他未受驗互動 | 特殊成本、替代／追加費用、限次或禁止打出等，若既有管線尚未准入便整卡拒絕；Quick 本身不放寬這些形狀 |

JP 非空卡文以「名稱＋完整 text」去重的 3,689 面中，112 面含 `{クイック}`：普通 Spell
110 面（109 面獨立無條件、1 面條件取得），token Spell 2 面。另有 18 面含 `{Q}{起動}`，
共 19 個起動能力：普通從者 11 面／12 能力、エボルヴ從者 1／1、護符 6／6。
普通從者其中 4 面／4 能力從手牌起動；其餘 7 面／8 能力在場上起動。
這是形狀盤點，不是可執行卡數；不同卡名、異文重印及 reskin 不合併，sections 中的 token
說明不當成持有卡的 Quick。本文、成本與其他能力會進一步縮小整卡准入交集。

## 2. 時點、行動者與窗口結束

| 規則位置 | 可以行動的玩家與下一步 |
| --- | --- |
| 自己 main 的普通行動點（7.3.3、12.3.5） | 回合玩家可普通打出 Quick 卡或起動 Quick 能力；這不是 Quick 窗口，仍受卡文禁止與一般合法性限制 |
| 對手攻擊宣告後（8.4.5–.8） | 先完成攻擊事件與 8.4.6 檢查；8.4.7 僅非回合玩家選一個 Quick 行動或 Pass。成功 play 後 8.4.8 檢查，再回 8.4.7 |
| 對手 end phase（7.4.1–.6） | End Begin 事件、7.4.2 檢查、回合玩家守護 act、7.4.4 檢查之後，7.4.5 僅非回合玩家選 Quick 或 Pass。成功 play 後 7.4.6 檢查，再回 7.4.5 |

兩個窗口的 actor 都固定為非回合玩家。沒有輪流優先權、沒有「雙方連續 pass」；該 actor
提交一次有效 Pass 即關閉此窗口。回合玩家不能回應 Quick，Quick 也不能插在成本、本文、
其他決策或 pending 能力的結算中。自己攻擊或自己 end phase 不另獲 Quick 行動權。
Start、任意 main 行動之間、一般卡片打出後與傷害之後，均不因 Quick 新增窗口。

每次成功 Quick 後，先完整處理 CheckTiming，再由同一非回合玩家在新狀態重新選擇。
這是同一攻擊／end 程序中的下一次詢問，不重新宣告攻擊、重發 End Begin 或重做守護 act。
即使攻擊者已被 Quick 移走，也依 8.4.8 回窗口，直到 Pass 或終局；不得用攻擊者不存在
提早跳過該次詢問。另一個合法攻擊則有自己的窗口。

Attack 的 Pass 續接 8.4.9：原攻擊者此時仍在場，才依當時狀態對原目標處理戰鬥傷害；
不在場便不造成戰鬥傷害，照樣 8.4.10 檢查、8.4.11 結束攻擊。移區再回場是新世代，
不改綁攻擊者／目標；既有攻擊程序負責戰鬥合法性，Quick 不重選攻擊目標。EndPhase 的 Pass 續接 7.4.7 棄牌及其檢查循環，
之後才到 7.4.8 到期與 7.4.9 換回合；棄牌觸發不重開 Quick。

## 3. 與 play、pending 及交易銜接

### 3.1 單次行動

Quick 只補時點許可，不免除 PP、目標、禁止或來源限制。Spell 完整沿
[play §1–3](play-semantics.md#1-有效區打出起點與來源)：預檢、公開並進 Resolution 新世代、
先決選項、目標／分配、決定並付費、played、本文、Resolution 收尾、resolved、CheckTiming。
固有 PP 只付一次，費用修正與 EX 承接依[打出費用修正](play-cost-semantics.md)，不可開第二條
Quick 專用付款或本文捷徑。移入 Resolution 後不因獨立 Quick 的有效區結束而取消已合法開始的 play。

Quick 起動沿同一 Activated 程序：精確的持有者、AbilityId／instance 與控制者由 authority
決定，持有者不因起動移入 Resolution。成本使來源離場時沿既有來源 LKI；未受驗來源／成本
組合先拒絕。普通起動仍不能在 Quick 窗口使用；限次按既有 PlayCommitted 語義消耗，
窗口重問不重設額度，也不把「每回合」改成「自己的回合」。

### 3.2 檢查、續體與終局

每次檢查沿[觸發與 LKI §2](trigger-lki-semantics.md#2-事件待機與檢查時點)的共同排程：
先規則處理，回合玩家 pending 優先；每解決一個再檢查，兩方皆無 pending 才續接窗口。
選 pending 的玩家與 Quick actor 是不同角色；不能因上一個行動由非回合玩家執行就讓其
pending 先解決。每次 Quick 的新待機必須處理完，才列下一題的合法 Quick。

authority 保存規則續體：Attack 或 EndPhase、該次程序身分、原攻擊 context（如適用）、
下一個規則步驟，以及有無未完成的 play／CheckTiming。typed 的 Awaiting／Resolved／
CheckTiming 不可漏接回原窗口；既有原型 `decide`／`submit` 不能繞過 typed pending。
整段只走一條 CheckTiming 排程，不重複發事件。沒有待答不表示 Pass。

依[觸發與 LKI 的終局規則](trigger-lki-semantics.md#52-prepared-play回復與終局)，
終局立刻使所有窗口與 play token 失效，停止待機及後續程序；不退費、不偽造 resolved、
不再詢問 Pass。
`CurrentTurnEnd` 的數值與費用修正到 7.4.8 才清除，Quick 期間仍有效，沿
[數值期限](numeric-timing.md#3-解算與回合期限)；不以非回合玩家的回合結束計算本回合期限。

### 3.3 拒絕與回復

錯 actor／token／型別／不屬題目 domain 的回答是協定 Invalid：保留原題與狀態，不扣 PP、
不耗 RNG、不算 Pass。有效選擇開始 play 後，規則非法 play 與內部 Unsupported 分別回報，
依 play 的整次 origin 回復，保留外發 epoch 高水位；配新 epoch 恢復同一窗口的選擇。
無法執行的卡不能悄悄從支援閉包刪掉、變成無效果或自動 Pass；對局預檢發現未受驗形狀
應在任何行動前拒絕。先前成功的 Quick 不因下一次失敗而撤銷。

## 4. 合法動作、可見性與答題

規則程序使用既有 `Procedure::Quick`、`QuickWindow::{Attack, EndPhase}` 與
`ChoiceDomain::QuickActionOrPass`。domain 是「本 actor 此時可合法開始的卡片 play／
能力 play，加上一個 Pass」，不是所有 Quick 定義；authority 驗證當前區域、控制權、
世代、Quick 許可、成本可付、目標可選、禁止與額度。首批沿既有可證明的 play 預檢，
需未知隨機結果或未受驗推理才能判合法的形狀拒絕。合法動作列舉不得試跑並提交遊戲效果。

選行動只決定打出哪張卡或哪個能力；其模式、目標與付款仍在原 play 各步詢問。不可先扣費
或提前固定 play 的選擇 snapshot；下一次 Quick 使用新狀態，不能沿用前次目標候選。

- `actor=RequireOne(Opponents(Active))`、恰選一個行動、visibility 僅該 actor，window
  必須與 authority 目前程序吻合；呼叫者不能自選窗口或把 `Procedure` 塞進卡片 body。
- 即使沒有合法 Quick，仍產生只含 Pass 的問題並等待有效回答。唯一合法動作也不自動執行。
  省略答案、斷線、空 payload、UI 關閉或錯答不算 Pass；逾時策略不由此契約新增。
- 回合玩家只知道正在等非回合玩家，不取得私有候選、張數或被拒原因中的手牌資訊。
  該 actor 可取得自己合法動作的必要資訊；對手手牌仍無可追蹤固定 ID。選定卡公開的時點沿
  play 步驟 1，不因列合法動作就公開所有手牌。
- 回答綁認證 actor 與既有 `(execution, decision, epoch)`；token 的 authority context
  綁本次規則程序、window 與固定 domain。下一次詢問配置新 epoch，舊答不得重放。
  回答不可提供自製 source、卡片 program、snapshot 或 window 以取得行動權。

這些是內部 prompt／answer 的義務，不宣告新的公開 wire schema。候選物件識別沿既有
view 的 audience 與 observation epoch；待答保存只有在完整保留續體／domain／token 序列
時才可用，做不到便沿既有禁止待答存檔，不新增另一套重播帳本。

## 5. 內部 IR 正規形與 lowering 邊界

以下是內部狹窄語義准入，不定義新的作者寫法。
沿既有 `Ability`、`AbilityOrigin::Keyword` 與 `Effect::Permission`，不新增 IR 型別：

| 類別 | 首批內部承載 |
| --- | --- |
| 卡片自身的獨立 Quick | 獨立 `Static` 能力，`Keyword(Quick, Printed(face,line))`，holder=SourceHolder、controller=Controller、active_zones 恰為 Hand／Ex，無 play／trigger／body／event captures；唯一 declaration 是下述自身打出許可 |
| 起動前的 Quick | 原 `Activated` 能力的 origin 為 `Keyword(Quick, Printed(face,line))`，active_zones=[Field]，其餘 PlaySpec／body 原樣；不另造 Static 許可、不改成 Spell、不標記其他起動能力 |

卡片許可恰為 `Permission`：subject 與 `PlayCard.objects` 都只選 SourceHolder 當前世代，
`polarity=Allow`、`condition=true`，`rule` 標示 CR12.3.2；duration 為 NoTemporalExpiry、
Installation anchor、無 usage／intervals。selector 恰為 `Objects([SourceHolder])`，
不接受他卡、額外 Filter 或跨世代跟隨。其效果只是 §2 的有限時點許可，不能以
泛用 Allow 跳過任何其他 play 檢查，也不安裝跨區永續實例。正式載入要求此 Static 的 face
對應同一普通 Spell 本文；Spell 本文仍是 Printed、[Resolution]，不把 Quick 改成本文的來源。

通用良構的 Permission 動作只收 Destroy／Banish；checker 與整卡准入僅為上述精確 header
及 declaration 接受例外，不能只放行 `KeywordId::Quick`、`PlayCard` 或整個 `effect.permission` requirement；
良構通過也不等於 Quick 執行准入。`RuleId` 目前是不透明編號、沒有固定的 CR 對照，准入不憑
它辨識 Quick，而以 origin 與精確形狀判定。origin 只能包一層 Quick＋Printed，不接受任意
巢狀 Keyword、Granted 或 Triggered Quick。authority 再比對整卡、face、來源座標閉包與實際時點。

代表正反例（示意，不是作者 YAML）：

1. 正例：BP01-024 的已核對獨立 Quick 位置 lower 成上述 Static 許可，另保留原 Spell 本文；
   可在 Hand 的 Attack 窗口選該卡，進 Resolution 後按原本文執行。反例：把同一許可的
   subject 改成全部己方手牌，或直接在 Spell 本文包 Quick origin，均拒絕。
2. 正例：BP02-027 的 Quick 僅包住那個 Field Activated 的 Printed origin，沿原 Act 成本。
   反例：持有者還在 Hand 時，不能因這個能力有 Quick 就把整張從者列為 Quick 的卡片 play。

lowering 必須保留 Quick 自身與被修飾起動的真實 face／印刷行，無法定位就拒絕，不猜 line；
來源閉包涵蓋許可與本文。未選模式或死分支中的不支援內容仍令整卡拒絕。候選診斷須分清
「Quick 許可已可 lower」與「整卡其他形狀未受驗」，不能只丟棄 Quick 而宣稱 main 可執行。
作者端的 card_field 或能力頭應由各自入口映到同一語義形；本文件不改 P1 的分類、hash
或來源格式，不把 card_field 假造為 ability_body。未具備所選作者入口的正式來源映射前，
可以用內部 typed 合成輸入驗 D1，不冒稱作者端已完成。

## 6. 代表驗收與實作接點

沿既有 play、成本、pending、期限與觀察測試，只加以下代表行為；不展開卡種、玩家、
窗口與付款的交叉矩陣。此處為實作驗收要求，不代表本文件提交已執行測試。

| 編號 | 代表局面與結果 |
| --- | --- |
| Q-A | 攻擊先產生 pending，排空後非回合玩家連用兩次受驗 Quick Spell，各次只付費一次、完成收尾及新 pending 後才重問；回合玩家不可插入行動。Pass 才回戰鬥；其中一次移走攻擊者仍重問，Pass 後不以新世代代打 |
| Q-E | End Begin → pending → 守護 act → 檢查 → Quick；只含 Pass 仍問。Pass 後棄牌檢查、7.4.8 到期，期間不重發 End Begin、不重開 Quick；到期前修正仍有效 |
| Q-I | Field 的 BP02-027 形狀沿原起動付款；同一持有者改在 Hand 不能 Quick 打出從者。普通 Quick Spell 在自己 main 仍走普通 play；自己的 end 沒有 Quick 權限 |
| Q-R | 一個錯 actor／舊 token 代表例保持原題；另一次開始後失敗回本次 play origin、以新 epoch 恢復窗口，保留先前成功行動。對手投影不暴露候選；沿用現有終局測試補驗不再重開窗口 |

D1 的接點為：整卡准入的 Quick 精確例外、play／activate 的時點 gate、規則 Procedure 的
prompt／answer、CheckTiming 後的 authority 續體，以及對局預檢與私有投影。現有原型
`game/rules.rs` 的 Quick 流程只供對照，不能據此跳過 typed 的 checkpoint、來源或拒絕邊界。
`engine-rules/1` 的作品設定、公開支援清單與最低版本均不因本規格變動。
