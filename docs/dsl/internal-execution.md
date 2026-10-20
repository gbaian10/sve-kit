# 內部已驗程式體執行入口

`Game::execute_internal` 接受 `WellFormedProgram`、來源座標及 authority 提供的來源物件／世代／控制者。
它只供合成與卡片段測試使用，不接作者 YAML、不執行整張 `CardProgram`，也不發布公開 capability。
`Registry::current().supported()` 仍為空；良構通過不能取代本入口的執行准入。

## 准入與原子性

入口以 `Requirements::derive_program` 走訪完整程式體，包含未執行分支；每一個 exact
capability code／version 及節點形狀都必須在封閉支援集合內。附加的 context 檢查不允許
既有 frame、prompt、待機、continuous、delayed、原型能力或 keyword 進入本片執行環境。
這些限制避免尚未遷移的取代、觸發或禁制被略過。卡片印刷資料仍由同一份 Catalog 取得。

支援 Sequence、If、root AtResolution 的 Int／Nat／Bool／Selector capture，以及 bounded
integer ResolutionChoice。數值支援常值、已驗 binding／capture、公開玩家資源、公開區域
計數、checked Nat 轉換、Add／Subtract／Multiply／Minimum／Maximum、條件值與 Draw 的 Cards
quantity receipt 投影；布林支援比較、And／Or／Not／Exists。selector 只接受公開 Field／Ex／Cemetery／Banish
的 Current view、SourceHolder、Objects／Union 及明示 capture；未准入的 read／selector 不以零或空集合代替。
一般隱藏資訊 If、Repeat、付款、模式及下節以外的安裝／action 都在執行前整棵拒絕。

Game／State、區域順序、RNG、Knowledge、事件與待機仍使用原有權威狀態。
typed ObjectId 由 State 內的持久映射配發給原型物件 ID，同一局內不重新編號、不重用，存檔後沿用。
內部續體只存 typed node、帶 ScopeId 的 slot、typed quantity receipt 及 input occurrence。
失敗時還原整份 Game，包含私有續體、事件 buffer、RNG、使用紀錄與輸入 epoch。
執行入口不允許原型 decide／submit 插入待答中的 typed 解算。尚無 typed save 格式；
序列化或計算 digest 待答 Game 會明確失敗，不能靜默遺失續體。

## 抽牌與傷害

Draw Exact 保留有號 requested，非正值不抽牌；正值依序呼叫共同的單張抽牌原語。
牌庫不足只記成功量及真正的 DrawEvent；失敗嘗試交由解算完成後的既有規則檢查處理。
UpToStepwise 每張前只詢問該抽牌者 Continue／Stop；每次問答都有新的 epoch。
單次執行限制為 10,000 次抽牌嘗試，超過時完整拒絕並還原。

Damage 只接受 Ability flags、Instruction batch、Current source 及 Uniform signed amount。
同指示開始時求值 recipients／amount；Union 依物件 ID＋世代或玩家去重。
不對非從者物件、已換世代或已離場者給傷害；非正量不產生 DamageEvent。
所有正量 hit 共用原型的 `apply_damage` 提交原語；typed DamageEvent 保留來源 snapshot、接受者、
量及控制者。致死與抽空的規則檢查在整段解算完成後執行，指示間不自行插入清場時點。
authority 的 `DamageQuantityOutcome` 記錄 Uniform 的原始有號 amount（requested）與實際正量合計
（actual），只作執行紀錄，尚不符合[執行語義 §1.3](runtime-semantics.md#13-damage-requestedactual-投影)
的整指示聚合契約。任何讀 Damage receipt 的投影（requested 或 actual）仍在入口整棵拒絕，
須完成聚合實作與局面驗收後才可解除。

Allocation／Distribution 仍在入口拒絕。IR 已以 `Distribution.minimum: Nat` 承載明示最低量；
本入口尚未執行它。來源、凍結、無解拒絕及 selected play targets 的最低 1 點限制依
[執行語義 §1](runtime-semantics.md#1-分配最低量與打出-snapshot)，不能外推為所有結算分配的預設。

## 觀察與測試種子

`internal_decision(view)` 只向 actor 或 Referee 提供目前 token／actor／輸入形狀，
不提供 receipt、binding 或剩餘程式；一般 projection 也採相同授權。
Continue 之後的問題不能重用舊 token；錯 actor、錯 payload、過期或已完成的答案完整拒絕。
抽牌後對手的目前手牌 slot 不帶固定 ID；過去已見資訊不會因此定位新的隱藏 slot。

`internal_ir` 測試使用合成局面，並以公開輸入產生器提供的印刷資料驗 BP01-065 抽二與
BP01-006 四點傷害片段。這些測試不包含 BP01-065 的 SpellChain／PP 回復，也不包含
BP01-006 的打出時目標選取與整卡准入；不宣稱兩張卡已遷移完成。BP06-110 未准入。

## 公開區域移動與破壞

Move 只准入 Put、Owners(Cemetery / Banish)，來源限公開 Field／Ex／Cemetery／Banish。
不接受 Shared(Field / Ex)、私人目的區、position、order、initial_posture 或 entry_effects。
Destroy 是獨立 action，僅接受仍在 Field 的目前世代；過期 capture 不移動新世代。
目前世代位於其他區域時整次執行失敗並回復，不外推場外 Destroy 語義。

同一指示先固定並去重物件，再共用 Game 的 `move_objects` 原語提交；目的區逐物件取其 owner，
不沿用來源控制者。token、advance 與連結規則尚未納入 typed 移動結果，遇到需要這些規則的
物件時拒絕整次執行。世代耗盡同樣拒絕，不能飽和後重用世代。

MoveOutcome 與 DestroyOutcome 不互換。每個輸入均保留 MoveResult 的 before、requested_to／
requested_position；沒有實際移動時 after／actual_to／actual_position 為 None。
成功跨區才填新世代與 actual_to；同區 Put 不造移區事實或更換世代。
DestroyOutcome.successful 只含本次真正破壞的 before／after，不把受禁止者算成功；
Put(Cemetery) 不產生破壞結果。成功成員的 snapshot 區分提交前後。

`Fact::Movement` 是 authority 專用的物理事實：保存 MoveResult、from、MoveMethod、Cause
與離區前控制者。這個型別沒有玩家／wire 序列化，也不是完整 EventHeader／DestroyEvent
管線。此子集 cause 僅為當次 Effect；不從目的墓地反推 AnyRule 或破壞。
取代安裝及改道尚未准入，replacements 保持空；沒有取代鏈的執行保證。
MoveAfter、成功 Before 的程式投影、MovedRef、RuleCarry、FollowEventMove 與事件 payload
read 均在完整 requirement 預檢時拒絕，包含未執行分支。receipt 與 after 世代不進玩家 packet。

InstallEffect 的有限子集為 InstructionStart 固定物件集合、Permission Forbid Destroy、
Effect(Any) cause、常值 true condition，以及 Installation anchor 的 EndResolution。
受保護世代存於同一份 typed 續體，僅阻止該解算的能力破壞；Put 與傷害致死的規則檢查
仍有效。解算結束後保護消失，沒有跨解算持久狀態；其他 cause／condition／期限與 Banish
保護整棵拒絕。這不代表 static 能力或完整 Permission 家族已執行。

BP02-116 的測試只驗一個已給定對象的破壞片段，不包含 Quick、打出目標或後續搜尋。
BP02-089 只驗能力破壞禁止述詞，測試的 EndResolution 為合成期限；原卡的 static、Aura、
進化與其他能力沒有遷移。兩者印刷資料均來自公開輸入生成器，片段不算整卡准入。

## 私有看牌與同區排列

Look 先准入 Exact、viewer 自己的 Deck，取頂端至多 count 張；不移區、不換世代。
LookOutcome 保存觀察的舊世代及 snapshot，並只向 viewer 增加觀察證據。
其他區域、他人牌庫、匿名 filler 及逐張 Look 仍拒絕。

Order 使用自己的 DecisionId，先固定 cards、chooser、單一 Owners(Deck) Placement
及完整牌庫 snapshot。可見性只准入以 chooser 為基準的 Only(SelfPlayer)；Public、AuthorityOnly
及其他 audience 的題目與結果投影尚未定義，包含未執行分支都拒絕。只准入 Top／Bottom、位於 chooser 自己牌庫的目前世代；
跨 owner、跨區與 Index 拒絕。cards 可由本次執行的私有 Look receipt、Union／Difference 及原有
公開 selector 求得；Look receipt 必須屬於 chooser，且仍有有效觀察證據。
這些新增的私有 selector 只在 Order cards 的 Union／Difference 主幹准入，不放寬一般
If／capture／Move 或主幹下其他 selector（例如 Zone predicate）的讀值範圍；未執行分支同樣檢查。

DecisionView 的 Order domain 只向 chooser／Referee 給出本次 token 範圍內的整數 handle
及印刷卡號／卡名，沒有物件 ID 或世代。Answer::Order 回答完整 handle 排列，authority
轉為 OrderAnswer 後驗完整域、重複、世代與 snapshot；錯玩家、錯 payload、過期 token、
漏牌或外來 handle 完整回復待答狀態。成功提交後同鍵重播明確拒絕，不再重排或耗 RNG。
空域自動記錄空結果，單張仍必問；待答仍不可存檔或計算 digest。

排列方向為頂至底，Bottom 的第一項位於底部區塊最上方。一次移除域並插入整個區塊，
保留域外相對順序；OrderOutcome 的 moves 空、成功成員不換世代，沒有移區／入場／離場
事實。非空重排撤銷該牌庫舊位置證據及先前 Look receipt 的排列授權，即使回答沒有改變
順序也一樣；需要再次排列時先取得新的 Look 證據。

BP07-SL09、BP06-024 僅驗「看五張、已決定零張入場、餘牌放底」片段；沒有實作中間的
選牌、總費用、入場、觸發及整卡其他能力。Search、其 groups／目的地程序、成功移動
remainder、減費安裝與 Token 共享容量仍未准入；不能以此片段宣稱整能力已執行。

Search 的靜態輸入點與三種回答型別已在 IR 定義，契約依
[執行語義 §6](runtime-semantics.md#6-search-的輸入點回答與觀察)；本入口仍未執行它們。
Public Order 依該文件 §7 維持拒絕，Repeat 的跨輪 epoch 依 §8 等待執行驗收。
