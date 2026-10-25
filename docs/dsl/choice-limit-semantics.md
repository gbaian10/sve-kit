# チョイス擴充與起動能力限次

本文件補充[正式 play](play-semantics.md)、[執行語義](runtime-semantics.md)與
[觸發額度](trigger-lki-semantics.md#3-限次與同時批次)，是內部契約。
現行支援以[內部執行入口](internal-execution.md)為準；不新增DSL 語法、公開 program、
capability、最低版本或整卡資格，也不擴張 Quick、費用及結算重驗的支援範圍。

規則依據為[日文 CR 1.28.0（2026-09-28）](https://shadowverse-evolve.com/wordpress/wp-content/uploads/2026/09/28121513/ShadowverseEVOLVE_cr_1.28.0_260928.pdf)，
PDF SHA-256 `c44b8f2ff7e1019bc2b4ec59df894d6a067179ff05eac2542b7723648a22cd47`。
條號指該版。能力實例、TurnId、交易與 IR 欄位是專案表示，不是官方新增規則。

## 1. 准入範圍

只承載單一 play 入口、唯一頂層 ChoiceKeyword、controller 選擇、公開模式。
模式參數仍只流入自己的 body；不互讀 sibling，不把結算時的 `choose` 當作チョイス。
本契約採以下三項擴充，每一項仍須通過完整來源、良構與執行支援檢查。

| 擴充 | 首批形狀 | 規則依據 |
| --- | --- | --- |
| 模式數 | 常值 exact／upto；既有 SC／NC 條件式模式數子集，限 §2.1 的表示 | 5.18.2–.3、10.6.2.2.3 |
| 本回合模式排除 | `exclude_chosen: this_turn`，固定 exact 1，唯一模式群組 | 卡文及 Q1567、Q1779、Q2563；IR 的 H.mode-use |
| 起動限次 | 普通 Printed Activated、Field、常值 `per_turn=1` 或 `2` | 卡文、10.6.2.5.4／.7、5.16.2.2.1 |

先拒絕：對手／其他 actor 選模式、巢狀或多個 ChoiceKeyword、If／Repeat 內的
ChoiceKeyword、模式內 Distribution、無 option 的根 targets 與模式 targets 混用、
任意私有選擇、可變 X、通用條件模式數、選擇數取代、共享／玩家／整局額度、授予能力限次。
觸發 `per_turn>1` 仍拒絕，`per_turn=1` 完全沿用觸發契約。

這些是第一版支援界線，不是否認合法卡文。模式內既有 Search 等獨立已驗操作不因本文件
擴成通用 ResolutionChoice；未選模式也須通過靜態支援閉包，不能靠選其他模式規避拒絕。
護符、Crest、UB、額外費用或未支援的 trigger 不因含上述形狀而取得整卡准入。

## 2. 模式決定、合法性與資訊

### 2.1 模式數與不足

沿 play 契約的先決選項階段，在任何 targets、分配、付款及本文之前決定模式。
count 的條件讀 `AtPlay`，選中集合固定於本次 execution；不是走到 Mode node 才問。
令 M 為**印刷模式總數**、N 為本次 count、cap=min(N,M)：

- exact：選 cap 個不同模式；upto：cap>0 時選 1..cap 個不同模式。
- cap=0：不執行該一連串チョイス指示；保存空模式結果，不發必須選 1 的無解題。
  不因此略過能力的其他內容或起動成本。
- cap 只對印刷模式數截斷。因目標不足或歷史排除而不可選的模式，不能把 exact 的
  必選數再縮小。例如 exact 2、印刷三個模式，但只剩一個合法模式，不能改成選一個。
  upto 2 則仍能選該一個；正數需求下沒有合法方案時不能打出。

模式數沿現有 `Control::Mode.count` 與 `ChoiceDomain::Modes.cardinality`：
常值 exact 為 cap..cap；常值 upto 為 cap>0 時 1..cap、cap=0 時 0..0。
條件式只承載既有 SC／NC AtPlay 比較與常值葉、以 1..求值後 cap 選擇的形狀；
IR 的靜態 cardinality 為 minimum=1、maximum=None。exact 1 可作該形狀的葉，
但條件分支若要求 exact 2 以上，不得偽裝成 upto，首批拒絕。SC／NC 的計數母體
取代仍拒絕。其他條件、由 X 決定個數及「好きな数」不在此表示範圍。

CR 5.18.3.1.2 要求選中的模式能適正打出；10.6.2.3.3 的必要目標選不足仍不能打出。
由 authority 在既有受限公開候選域證明至少一個合法模式集合及其目標／成本方案，
不把「先選之後會有的目標」當合法，也不因本文將無法完全執行就刪掉模式。
例如全體效果沒有成員，不等於指定目標選不足；結算動作的可行性沿各指示規則。
無法判定的互動回 Unsupported，不猜測合法、不問無解題。

[BP02-045 連続実験](https://shadowverse-evolve.com/cardlist/?cardno=BP02-045)
Q293／Q294（2022-06-24）確認 upto 至少選一個且不能重複選同一項；
[BP01-005 リノセウス](https://shadowverse-evolve.com/cardlist/?cardno=BP01-005)
Q1598（2024-08-20）確認缺必要目標的模式不能選。
卡例只證明規則，包含 token 等其他內容的完整卡仍須獨立准入。

### 2.2 回答、本文與可見性

輸入仍是既有 ModeDecision 的完整模式集合，綁本次 execution、decision 與 authority
token；集合沒有玩家指定的解算順序。正確 actor 是能力 controller，錯 actor、重複 ID、
域外 ID、已排除模式或錯數量均為協定 Invalid，保持原題、不消耗 RNG／額度／歷史。

合法回答後，僅對選中模式建立其原作用域的 targets；所有 targets 仍在付款前完成，
共用 play 契約的選擇 snapshot。之後按卡面印刷序解算（10.6.2.8.2.2），不按回答順序。
未選模式依 5.18.4 視為不存在，不選它的目標、不執行它的成本或本文。
在此之前的靜態全樹支援檢查仍不能略過。

模式選擇及本片公開目標可向雙方公開；未公開手牌、牌庫候選不能因模式公開而公開。
外部僅用當次題目的識別與已知來源資訊，不提供 AbilityInstanceId、server ObjectRef
或物件進入非公開區後可繼續追蹤的固定 ID。歷史留在 authority，玩家無法提交或覆寫它。
目前整卡待答保存仍未准入；不得新增忽略模式歷史或額度的存檔路徑。

### 2.3 本回合不得重選

沿用 [IR GAP-S-017 的模式使用事實](ir-vocabulary-1.0.md#gap-s-017)，
不新增卡片專用 counter。`ModeExclusion::CurrentTurnAbilityInstance` 的讀值鍵是
當前 `(TurnId, AbilityInstanceId, ModeId)`；本片一個能力只有一個模式群組。
先驗目前模式的打出合法性，再排除已用模式，保留原 ModeId 及印刷序。
沒有可選項即不能打出；這不等於允許選空集合。

選過但未完成打出程序不記使用。於 CR 10.6.2.7 的 AbilityPlayed，將已選模式以
H.mode-use 的 `(execution, ability_instance, mode_id, turn)` 事實原子記入；
同一 execution 的 continuation／重送不能再記一次。本文條件 false、目標之後失效
或實際造成零傷害，都不退還已用模式。付款失敗或整次 execution 內部錯誤 rollback
時一併撤銷這次新記錄，保留此前成功執行的歷史；沿既有 origin 與 epoch 規則。

實例與回合的生命週期依 §3.2，不能以卡名、印刷 AbilityId 或玩家共用一份集合。
[BP11-092 清浄の領域](https://shadowverse-evolve.com/cardlist/?cardno=BP11-092)
Q1567（2024-08-20）、[BP13-073 クリムゾンウォー・ラウラ](https://shadowverse-evolve.com/cardlist/?cardno=BP13-073)
Q1779（2024-12-13）及 [BP19-020 出航の咎人・バルバロス](https://shadowverse-evolve.com/cardlist/?cardno=BP19-020)
Q2563（2026-03-25）證明兩個持有者互不耗用對方的選項。
使用事實的提交點是依既有 IR／play 程序的專案表示；這些 Q&A 不單獨證明所有 rollback 情境。

## 3. 起動限次與能力實例

### 3.1 共用 LimitSpec，分開消耗點

首批起動限次使用既有 `LimitSpec {maximum:1|2, window:Turn,
scope:AbilityInstance, consume_at:PlayCommitted}`；不另造起動 counter 型別或玩家額度。
同一 authority 額度服務依 consume_at 區分：觸發是 PendingAdmission，起動是
PlayCommitted。`PlayCommitted` 在此精確對應 CR 10.6.2.7，不是收到起動要求、
選完目標、付款開始，也不是本文成功造成效果。

起動前驗剩餘額度，沒有額度即非法起動，不出題、不付款。合法起動在全部成本付完、
到達 AbilityPlayed 時消耗一格，與 played 事實原子形成。待答、重送或重新求有效能力
不能另配新實例或再扣一次。本文 false／無效果仍耗次；沒有同回合重試至有效果的權利。

付款失敗（10.6.2.5.4）回整次 play 前，不扣次、不保留部分付款；內部執行錯誤依
play 契約回整次 origin，連暫存 played、模式歷史與額度一併撤銷。這與觸發不同：
原事件已成功提交的 PendingAdmission 額度不因其能力後來無法打出而退回。
來源作成本離場仍能解決已打出能力（10.6.2.8.2.1），以原 instance 記該次使用，
不把額度記到後來的新世代。終局沿觸發／LKI 契約，成功規則終局不是 rollback。

### 3.2 計數單位與重置

| 情境 | 額度與模式歷史 |
| --- | --- |
| 同來源、同世代、同定義再次起動 | 沿用 AbilityInstanceId；ExecutionId 每次不同 |
| 兩個同名來源／同卡兩條能力 | 各有自己的實例，不共享 |
| 新實際回合 | 用新 TurnId，兩位玩家的本回合紀錄均不延用；不是只在自己的回合重置 |
| 普通換區後再回場 | 新 holder 世代、新能力實例（4.1.4）；不能把舊次數轉移過去 |
| 場間控制權轉移 | 同物件與同實例，保留當回合次數；新 controller 不獲新額度（5.22.4） |
| 進化前後印刷能力 | 5.16.2.2／.2.2.1 使舊印刷能力失去、新定義取得；即使文字相同，新定義也有新實例 |
| 進化後仍持續存在的同一能力來源 | 不因 holder 進化或重算有效能力重配實例；holder 本身不換世代 |

這沿用觸發契約的能力生命週期。第一版只准入 Printed；能力失去後重新授予、複製、
暫時消除能力、Grant 的實例重建與共享限次仍拒絕，不從「文字相同」推論身分。
[BP01-013 フェアリービースト](https://shadowverse-evolve.com/cardlist/?cardno=BP01-013)
Q158（2022-04-26）確認兩個持有者可各使用一次；
[BP03-038 オズの大魔女](https://shadowverse-evolve.com/cardlist/?cardno=BP03-038)
Q471（2022-12-09）確認自己與對手的回合各可使用一次，不因此解鎖其 Quick 與土之秘術；
[BP08-084 欲望の狂信者](https://shadowverse-evolve.com/cardlist/?cardno=BP08-084)
提供每回合兩次的卡文形狀，並不因此解鎖其生命成本。
玩家的進化／進化相當能力共享行動次數是 RulePermission，不能塞進本片的 AbilityInstance 額度。

## 4. lowering、良構與執行接線

以下為 D1 的輸入與良構義務；已驗子集見[內部執行入口](internal-execution.md#共同選擇-snapshot-與回復)。

### 4.1 輸入與 lowering

輸入為現有 DSL schema 的 `choice.n`／`upto`／`instead`／`exclude_chosen`、
`activate.per_turn`，以及能定位 owner／face／能力行／fragment／DSL 路徑的來源證據。
不要從 regex 盤點或卡名自行製造正式 authored 來源。所有未選模式、成本與能力都須
保留原來源閉包；無法映射的形狀按能力／整卡拒絕，不刪掉欄位後假裝無限次。

- `choice.upto` 與 `n` 保留區別，按 §2.1 產生 count／cardinality。`instead` 首批
  只接已驗 SC／NC 條件表示，從 schema 到窄 AST 都要保留相同語義；不接受混合 exact>1。
- `exclude_chosen: this_turn` 降為既有 Modes.exclude_used 的
  `CurrentTurnAbilityInstance`。只收 exact 1、controller、唯一頂層群組；來源沒有
  排除文字時不得自動加上，也不得把跨回合或全同名卡的排除降成它。
- 起動 `per_turn` 降為 §3 的 LimitSpec。內部欄位
  `PlaySpec.limit: Option<LimitSpec>`，僅 Activated 能力允許 Some；Spell／Triggered 能力
  及 `Action::PlayCard.parameters` 的 PlaySpec 必須 None，Triggered 額度仍在
  `TriggerSpec.limit`，不重複承載。無限次明示 None，不能以 maximum=0 表示；
  首批 Some 只收 1／2。

### 4.2 良構與准入

良構須遞迴驗新 PlaySpec.limit 只在 Activated 出現，且 maximum（1／2）、window、
scope、consume_at 為 §3.1 的精確組合；
不更改既有 Duration.usage 的 UseLimit 或其零值規則。模式排除只在 §4.1 的入口成立，
不能令封閉 Program、ResolutionAlternative 或巢狀 Mode 憑 enum 取得歷史權限。
其餘 actor、可見性、binding 支配、未選模式及來源檢查全部沿用。

執行器須從已驗整卡入口建立／保存真正的 AbilityInstanceId、TurnId、模式歷史及額度。
模式問答沿既有 CardPlay Awaiting／answer_internal_card，成功仍回既有 Activation；
不新增 caller 可填的 instance、已用次數或核可旗標。准入能接受結構，不代表當前局面
可以起動；已耗盡是 IllegalPlay，錯答是 Invalid，未支援形狀是 Unsupported。

兩個代表良構例，以結構摘要表示，非新DSL 語法：

```text
正：Activated.play.limit = { maximum: 1, window: Turn,
      scope: AbilityInstance, consume_at: PlayCommitted }
反：同一輸入把 consume_at 改成 PendingAdmission → 拒絕

正：唯一頂層 ChoiceKeyword，controller／Public／exact 1，
    exclude_used = CurrentTurnAbilityInstance
反：同一群組改成 upto 2，或改為結算時的 ResolutionAlternative → 拒絕
```

模式 body 內的巢狀 ChoiceKeyword 已由既有良構拒絕，不能代替上述排除反例。

### 4.3 最小驗收

只在既有相應測試補代表例，不建矩陣；已有證據覆蓋的程序不重建一套。

1. 合成 upto 2 的兩模式：選一個合法、選空或重複 ID 非法，反序回答兩個仍按印刷序。
   其中一個缺必要目標時只可選另一個；把 count 改成 exact 2 則不能打出。
   既有 SC／NC AtPlay 測試沿用，不複製三份。
2. 合成 exact 1 排除歷史：第一次成功後不能再選同一項，另一 instance 可選；
   新回合可再選。付款失敗後仍可選，成功但本文 false 則不可。所有選項耗盡不能打出。
3. 合成 Printed 起動每回合一次：第一次付款失敗不耗次，成功一次後第二次被拒絕；
   新回合恢復。以同一生命週期案例確認普通離場再入場是新實例、進化的新印刷定義
   也是新實例；不另做玩家／成本／能力種類交叉格。每回合兩次只補第三次被拒的短例。
