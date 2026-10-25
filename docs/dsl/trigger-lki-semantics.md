# 常見自動能力、待機與來源最終資訊

本文件定義內部 typed IR 的執行義務，補充[執行語義](runtime-semantics.md)、
[打出程序](play-semantics.md)與 [IR §7](ir-vocabulary-1.0.md#7-事件cause-與誘發)。
這是待實作契約；不代表[良構檢查](typed-ir-validation.md)或
[executor](internal-execution.md)已准入，也不變更DSL 語法、公開 program、capability 或最低版本。

規則依據為[日文 CR 1.28.0（2026-09-28）](https://shadowverse-evolve.com/wordpress/wp-content/uploads/2026/09/28121513/ShadowverseEVOLVE_cr_1.28.0_260928.pdf)，
PDF SHA-256 `c44b8f2ff7e1019bc2b4ec59df894d6a067179ff05eac2542b7723648a22cd47`。
條號均指該版；以保存的 PDF 核對。事件欄位、producer 與交易狀態是專案表示，
不是官方新增規則。本文不宣稱整卡已可執行。

## 1. 第一版範圍

依 JP 卡文中五族（抽牌、傷害、破壞、搜尋、加攻體）的詞彙盤點，Fanfare 之外
優先承載下列五類。僅接受 Printed 能力及 Printed 關鍵字的來源，
`AbilityKind::Triggered`、能力與 trigger 的 `active_zones=[Field]`。
本文、選項、成本及所有可能介入的效果仍須逐棵准入，不能以觸發種類放行整卡。

| 入口 | 首批事件與限制 | 依據 |
| --- | --- | --- |
| ラストワード | 自身實際 Field→Cemetery；不限定 MoveMethod 或 cause | 12.5、10.7.4.1.2 |
| 進化時 | 自身 Evolve；含合法超進化的 evolve facet；不含進化相當能力 | 12.6、12.2.4、5.16 |
| 攻撃時 | 自身 Attack；長寫攻擊、被攻擊及交戰訂閱先拒絕 | 12.7、8.4.5 |
| end phase 到來 | `Phase(End, Begin)`，只收自分／相手／每個 end phase 的 Printed 訂閱 | 7.4.1–.2 |
| 長寫場に出たとき | EnterField；收自身、自己／對手場上的從者、可排除自身及已受驗的類型／特徵事件條件；「自分のターン中」比對事件 TurnId 的回合玩家（同 §3），其他時期條件拒絕 | 5.5.3、10.7.4、10.9.3 |

既有 Fanfare 沿用自身 EnterField；新入場路徑仍須有受驗 Move／Creation producer。
一般「場に出た」不得額外要求是 play。來源區讀事件 `from`，不是目前區域；
經 Resolution 的來源依 5.5.3 保存，不能一律寫 Resolution。
一次同時入場時，新入場的監看者也按 10.7.4.2 觀察同批其他入場，
先原子安裝入場效果、算有效資訊，再匹配。場間控制權轉移不產生 EnterField。

main phase、傷害、自身離場與獨立超進化時的有限擴充另見
[第二批觸發契約](trigger-second-semantics.md)；未列入口仍拒絕。
本文件的首批範圍中，main phase、抽牌／傷害／生命變更等其他訂閱、一般離場、獨立超進化時、State、
Delay／Grant／非 Field 有效區、一般 AnyOf、PerBatch 皆先拒絕。
這是首批支援限制，不否定 IR 詞彙登錄的完整語義。一般回合開始／結束文字不自動
別名到 main／end phase；「次の…」產生的延遲能力也不冒充 Printed phase 訂閱。

## 2. 事件、待機與檢查時點

事件在規則動作實際完成時形成，非提案時、非檢查時點才形成。完全被取代的動作
不產生原事件。採用既有 EventHeader、OriginFactId 與 typed payload：一個事實的
Move／LeaveField／Destroy facets 不能讓同一能力同一 occurrence 重複待機。
不同事實即使同時發生也保留重數（10.7.2.1）。

authority 在動作提交的同一交易中依事件 snapshot 判定有效能力、匹配事件、處理
抑制及 §3 額度，建立 pending。本文中的「なら」仍是 K.if，依既有 R-0009、R-0002、R-0003
到結算該句才判定，不拿本文條件取消觸發；事件資訊則固定為 AtEvent。
執行中可新增 pending，不能插入另一個能力的結算。

pending 至少保留：唯一 occurrence、AbilityInstanceId、定義與 origin、原 holder
ObjectRef、`controller_at_trigger`、EventId 與不可變事件 context、captures、已占額度。
holder 與事件主體不一定相同；controller 不從離場後的新物件補讀。
來源移區或喪失能力不刪既有 pending（10.7.7）；多份同文能力的實例也不合併。

檢查時點嚴格依 10.5.2：

1. 同時執行當前全部規則處理；有新規則處理就再做，直到穩定。敗北依 §5 處理。
2. 回合玩家有 pending 時，由該玩家選**一個 occurrence**，打出並解決，再回第 1 步。
3. 回合玩家沒有 pending，才由非回合玩家選自己的一個，打出並解決，再回第 1 步。
4. 兩者皆無才返回原來的規則程序。

不是 FIFO、卡號序或一次預排雙方全部能力；非回合玩家解決後若回合玩家新增待機，
下一輪仍先回合玩家。詢問候選包含能區分事件的公開上下文，由 authority 驗 actor、
pending id、epoch 及仍可選資格。選序不等於可以不打出（10.7.3.1）。

選中後依 play-semantics 執行獨立能力 play，目標到此時才選。成功解決取消該一次待機；
依法不能打出時取消該一次待機（10.7.3.2），不撤回造成事件的前一次 play。
錯 actor／token／payload 留原題；Unsupported、缺 producer 或內部錯誤須回本次能力
嘗試前並保留原 pending，不能當成規則上不能打出。本文任意選項的取消權不因此增加。

## 3. 限次與同時批次

首批支援無限次，以及 `LimitSpec {maximum:1, window:Turn, scope:AbilityInstance,
consume_at:PendingAdmission}`。自分回合限定另外匹配事件的 TurnId／回合玩家；
不是把 Turn window 改成「下次自己回合」。其他數值、Game、ObjectGeneration、
RulePermission、PlayCommitted 與共享額度先拒絕。

依 10.7.2.2，以該 AbilityInstance 在當前 TurnId **進入待機的次數**扣除額度。
同時符合多個事實但只餘一格時，該能力 controller 選其中一個事件進待機，
不是執行器任取第一筆，也不是先全入列再丟棄。只有一個候選就直接建立；
沒有額度不詢問。多位玩家須同時選時依 1.3.4 回合玩家先選，另一方可知其選擇。
這是事件提交中的額度選擇，不開 Quick 窗口、不提前解算 pending。

額度與所選 pending 原子提交；之後本文條件不成立、無合法目標或依法未能打出，
都不退還額度。造成事件的原交易 rollback 才一併撤銷其 pending／額度。
新實際回合用新 TurnId；同一回合不因重算或再掃描重置。進化不換 holder 世代；
對仍屬同一來源定義的能力不得重建實例以重置額度。進化後的印刷能力依 5.16.2.2.1
是新取得的定義，即使文字相同也是新實例，不承接進化前實例的次數。移區後的新世代有新實例。

本片所有訂閱為 PerFact；同時 N 隻入場，監看者可有 N 個 occurrence，
再受上述限次約束。需要「1 体以上」聚合的 PerBatch 保持拒絕；
不能把 Damage 的同時 batch 當成已實作觸發聚合，也不新增通用 batch 分組機制。

## 4. 事件 payload、世代與可見性

共用 header 保留事件／批次／執行身分、直接 cause、actor、source、規則時間、
before／after snapshot 與 visibility。規則破壞的 source=None；不能把先前傷害來源
冒充規則破壞的直接 source（IR §7）。以下 policy 是內部既有 enum 的限定使用。

| 事件 | 必須可驗回的資訊 | 來源判定與視角 |
| --- | --- | --- |
| Last Words 的 LeaveField | `object_before`、`controller`、`lki`、實際 `to=Cemetery`；同一成功 Move 成員的 from=Field | `BeforeExit`；holder 等於 object_before；用移動前有效能力。成本送墓亦符合，回手／消滅不符合；不看 Destroy facet 是否存在 |
| Evolve | `subject`、`with`、Normal／Super、before／after characteristics | `AfterEntry` 在此表示事件完成後，不另造入場；場上 subject 保持世代，進化卡及關聯各有自己的身分。以進化後有效能力判斷進化時；事件在合法進化程序全部完成後產生，然後到其規則檢查時點 |
| Attack | `attacker`、`target`（Player 或精確 ObjectRef）、controller、回合內 ordinal | `AfterEntry` 表示 8.4.5 宣告完成後；先驗目標、act，再發事件，8.4.6 檢查，之後才到 Quick／傷害。不是命中才觸發；攻擊目標與能力 play 的 targets 分開 |
| Phase | TurnId、回合玩家、phase=End、edge=Begin | `AfterEntry` 表示 7.4.1 到來時；7.4.2 處理待機，早於守護 act、Quick、棄牌及 7.4.8 到期。重進檢查時點不再發同一次 phase 事件 |
| EnterField | 入場新世代 object、from、controller、entry characteristics、entry method、可選 play receipt | `AfterEntry`；入場效果與持續效果已適用。觀察者的 holder 和新入場 object 分別保存；同批來源也入場時依 10.7.4.2 納入 |

Normal／Super 必須來自規則程序的真實結果，不能由呼叫者填字串。Super 的 evolve
與 super-evolve facet 共用原事實；本片只訂閱 evolve。數值進化仍受
[numeric-timing §1.1](numeric-timing.md#11-進化時的差額)限制；本契約不解鎖 Set／帶期限
Add 跨進化的還原，也不把食事／憑依當作 Evolve。

事件資訊凍結不代表可以永久操作該物件。以 EnterField.object 指向「それ」時，
之後只操作原入場世代；其已離場則依該指示的合法性處理，不找同 physical ID 新世代。
這不需要泛用 TriggerEvent receipt 或 FollowEventMove。本片兩者仍拒絕，
一般 MoveAfter／RuleCarry 權限也不擴張。

以上場上資訊及送公開墓地的事件可公開；source／target 的 server ObjectRef、
SnapshotId、內部 receipt 不因事件名稱而成為 wire 欄位。給玩家的待機選項只投影已知
卡名／公開事件與當次選項識別。不得沿來源以後進入非公開區而提供固定可追蹤 ID；
對手隱藏牌、裏向消滅、隱藏區 event snapshot 及無法驗證的可見性均保持拒絕。

## 5. LKI 與已準備的 play

CR 10.11.1：效果要讀特定卡資訊或配置，而卡已離開所參照區域時，讀它最後仍在該
區域的資訊；場間控制轉移例外。10.6.2.8.2.1 與 10.7.7 分別保證已打出與已待機
能力不因來源離區而消失。這與來源現在是否仍能**新起動**是不同問題。

### 5.1 保存與讀取

authority 對每個受驗來源保留最小的不可變資訊視圖：原 ObjectRef、原區域、
owner／controller、卡表版本與 face、進化／配置狀態、依 10.9 算出的有效特徵及其
Original 視圖。印刷資料仍引用卡表快照，不複製另一套卡表；只保存本片讀值所需資訊。
第一版資訊使用端限 source 的 Cost／Attack／Life（Original／Effective）、
既有來源歸屬與能力傷害 source；其他 LKI 讀值、任意對象、CostMaterial 先拒絕。

保存點是**成功移區前最後一刻**，早於移區清除 continuous／counters／進化關聯與換
世代；包括成本、本文、規則處理及 Resolution 收尾造成的移區。同時移區的全部成員
使用同一 before 狀態，不依逐張清理順序讀資訊。先暫存，移動真正成功才封存；
取代／禁止沒有造成離區時不製造該區的 LKI。正常同區重排及場間控制轉移不製造新世代。
視圖至少保留到引用它的 pending、execution 及其可回復 origin 全部釋放；
不要求永久歷史倉庫。尚未能完整保存這些關聯時，待答存檔仍拒絕。

Last Words 的來源 LKI 指向該 LeaveField.before；其他 pending 的事件 snapshot 不等於
後來的離區 LKI。例如攻擊時先入待機，其他能力把來源加攻後再使它離場，
「これの攻撃力」在結算讀的是最後離場值，不是攻擊宣告值。來源仍在原區時則讀當前值。
事件條件／AtEvent captures 始終讀事件視圖；已捕捉的數值不重新計算。

為避免把 Current 偷改成自動 fallback，規定一個**待新增的內部變體**
`ObjectView::SourceInformation {}`：只指目前 authority execution／pending 的來源。
它驗該 holder 的精確世代仍在參照區域，成立則讀 Current；已合法離區則讀該來源的
authority LKI。沒有任意 object 參數，也不接受 caller 交來的 snapshot。
lowering 對「結算時讀這張來源的資訊」及 Damage.source 使用此變體；
它不是任意物件的自動追蹤器。Field→Field 仍是同區域物件，依現行值讀，
能力的 controller 仍取打出／待機時已固定的值。

來源資訊的唯一正規形是 SourceInformation；Last Words 也用它，由 LeaveField.before
提供離區證明。既有 `ObjectView::LastKnown {object:SourceHolder, reason:SourceDeparted}`
與它語義重疊，lowering 不產生、checker 維持拒絕，不形成第二種寫法。
reason enum 本身不是 producer。`RuntimeObjectView {object,snapshot}` 也只是引用，
必須驗它來自本次來源的封存；跨 execution／來源／世代或缺資料都是內部錯誤。
日後任何 LastKnown 在物件尚在原區時也不可悄悄讀 Current；一般數值特徵仍用 Original／Effective，
不因此開放另一個 `CharacteristicView::LastKnown` 歧義入口。

stale Current 仍是錯誤，絕不能轉成 `AbsentByRules` 或 0。只有合法資訊視圖中
規則上不存在該欄位才是 AbsentByRules，之後才能按 1.3.6 求值。
LKI 只供資訊與效果來源，不能讓已失效 targets 再變合法、當 selector 的 live 候選，
或把來源新世代拿來 Damage／Modify。來源上的傷害禁止等仍須依既有規則驗證；
無法承載 Q1709 類限制時拒絕整棵，不因 source 已離場把禁止清空。

### 5.2 prepared play、回復與終局

沿 play-semantics 的 origin／ExecutionId／AtPlay／共同選擇 snapshot 繼續，
來源於成本或本文離場時更新 authority 的來源資訊 handle，保留已選目標、付款結果、
待答 continuation 與適正 play 事實，不重新 prepare、不付第二次成本。
Spell 的來源是進 Resolution 後的新世代；不能把 Hand 的 AtPlay 視圖當 Resolution LKI。
收尾只處理仍屬本次 Resolution 的物件，不能追到同張實體牌後來的新世代。

內部錯誤回本次能力／card play 的外層 origin，含 LKI 保存、RNG、pending、額度與
暫存 facts；epoch 高水位不退。已成功的前置 card play（例如觸發 Fanfare 的那次打出）
不隨後續能力失敗回復。
若來源仍存在但缺讀取權、離區卻沒有 LKI，都是錯誤，不是取消 play 或規則 no-op。

來源消失不等於遊戲終局。正常生命歸零／抽空依 11.1–.2 在檢查時點處理，不能在
本文中途憑條件提前放棄已準備的能力。實際敗北／同時敗北依 1.2.1–.2 結束對局；
投了依 1.2.3 不等待檢查時點。終局是成功的規則結果，保留已執行前綴的狀態與事實，
使所有舊 prompts 失效，停止 pending 與本文後續，不退費、不 rollback、不偽造尚未
完成的 resolved。若終局在本文已完成後的 CheckTiming，既有 resolved 仍成立。
首批不承載卡文直接勝敗指示；遇到該形狀預檢拒絕，不混為來源生命週期問題。

## 6. 良構接點與代表驗收

本片不修改 Rust 型別或 checker。D1 接線時須同時補正式 lowering／良構與整卡准入：
新 SourceInformation 僅在能力入口提供來源 context 時可用，closed synthetic Program
不得憑字面 source 取得權限；Last Words 精確使用 LeaveField 而非 Destroy。
Evolve／Attack／Phase 的 `AfterEntry`、limit 的精確四欄及事件 binding 的支配關係
須遞迴檢查。新增內部型別時補既有 walker／診斷，但不宣稱任何公開 qualification。
TriggerEvent receipt、MovedRef 泛用跟隨及任意 LKI producer 均維持拒絕。

代表驗收只用下列兩個情境加小型變體，不建交叉矩陣：

1. 合成來源有 Last Words，攻擊力先從 2 加到 4，再因**非 Destroy** 的合法成本
   Field→Cemetery；待機後相同實體牌回場為新世代、攻擊力 7。
   舊能力仍須打出，SourceInformation 讀 4，傷害 source 是舊 holder；
   改成回手則不觸發 Last Words。把讀值換成 stale Current 或偽造 LKI 必須拒絕，
   不得得到 0 或 7。這是合成契約例，不宣稱有這張完整真卡。
2. 合成 Printed 入場監看能力每回合一次，同時兩個符合從者進場，controller 選其中
   一個事件占額度。雙方都有 pending 時先回合玩家，且每解決一個都重跑規則；
   該能力本文條件 false 仍不退額度。改成 PerBatch 或用 PlayCommitted 消耗必須
   靜態拒絕。Attack／Evolve／Phase producer 各自接線時只加一個時點代表例，
   不把這個 pending 情境重建三遍。
