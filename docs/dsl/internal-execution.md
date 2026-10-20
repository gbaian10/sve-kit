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
計數、checked Nat 轉換、Add／Subtract／Multiply／Minimum／Maximum、條件值與 Draw 的 Cards、Damage 的
同型 Requested／Actual quantity receipt 投影；布林支援比較、And／Or／Not／Exists。selector 只接受公開 Field／Ex／Cemetery／Banish
的 Current view、SourceHolder、Objects／Union 及明示 capture；未准入的 read／selector 不以零或空集合代替。
一般隱藏資訊 If、Repeat、付款、模式及下節以外的安裝／action 都在執行前整棵拒絕。

Game／State、區域順序、RNG、Knowledge、事件與待機仍使用原有權威狀態。
typed ObjectId 由 State 內的持久映射配發給原型物件 ID，同一局內不重新編號、不重用，存檔後沿用。
內部續體只存 typed node、帶 ScopeId 的 slot、typed quantity receipt 及 input occurrence。
入口失敗與錯誤回答還原整份 Game，包含私有續體、事件 buffer、RNG、使用紀錄與輸入 epoch；
尚未開始分配的 execution 在有效回答後執行失敗，同樣還原到回答前並保留原詢問。
已開始分配的 execution 若在有效回答後執行失敗，回復 execution 前的 Game 並取消續體；
僅保留已外發 epoch 的高水位，避免舊 token 因回復而復活。
便利入口回復到準備前；明示預備入口保留已提交的 play 與時間，可用同一 handle 重試，但舊答案 token 仍失效。
執行入口不允許原型 decide／submit 插入待答中的 typed 解算。尚無 typed save 格式；
序列化或計算 digest 待答 Game 會明確失敗，不能靜默遺失續體。

## 抽牌與傷害

Draw Exact 保留有號 requested，非正值不抽牌；正值依序呼叫共同的單張抽牌原語。
牌庫不足只記成功量及真正的 DrawEvent；失敗嘗試交由解算完成後的既有規則檢查處理。
UpToStepwise 每張前只詢問該抽牌者 Continue／Stop；每次問答都有新的 epoch。
單次執行限制為 10,000 次抽牌嘗試，超過時完整拒絕並還原。

Damage 只接受 Ability flags、Instruction batch、Current source 及 Uniform signed amount。
同指示開始時求值 recipients／amount；Union 依物件 ID＋世代或玩家去重。
物件成員無法確認為從者時整次拒絕，不推定非從者是否計入 requested；這是內部准入限制。
目前世代不在場上的成員、或捕捉時不全在場上的集合，也整次拒絕；capture slot 保存當時的場上來源證據，
不以「後來已換世代」冒充原本合法的場上選取。從場上捕捉後才失效的從者仍依下述聚合契約處理。
已換世代或已離場的從者成員不受傷害；非正量不產生 DamageEvent。
所有正量 hit 共用原型的 `apply_damage` 提交原語；typed DamageEvent 保留來源 snapshot、接受者、
量及控制者。致死與抽空的規則檢查在整段解算完成後執行，指示間不自行插入清場時點。
authority 的 `DamageQuantityOutcome` 依[執行語義 §1.3](runtime-semantics.md#13-damage-requestedactual-投影)
記錄整指示聚合量：requested 為凍結去重成員各項 `max(0, amount)` 的合計，actual 為有效成員
實際正量的合計，不以剩餘生命封頂。捕捉後失效的從者成員仍計 requested；指示開始的動態集合
已無該物件則不計。聚合、轉型、體力扣除與累積傷害加法皆 checked；溢位回復整份 authority。
同型 Damage Requested／Actual 投影在指示完整提交後可供後續指示讀取，不能當成 Cards。

Allocation／Distribution 仍在入口拒絕。IR 已以 `Distribution.minimum: Nat` 承載明示最低量；
本入口尚未執行它。來源、凍結、無解拒絕及 selected play targets 的最低 1 點限制依
[執行語義 §1](runtime-semantics.md#1-分配最低量與打出-snapshot)，不能外推為所有結算分配的預設。

## 觀察與測試種子

`internal_decision(view)` 只向 actor 或 Referee 提供目前 token／actor／輸入形狀，
不提供 receipt、binding 或剩餘程式；一般 projection 也採相同授權。
Continue 之後的問題不能重用舊 token；錯 actor、錯 payload、過期或已完成的答案完整拒絕。
抽牌後對手的目前手牌 slot 不帶固定 ID；過去已見資訊不會因此定位新的隱藏 slot。

`internal_ir` 測試使用合成局面，並以公開輸入產生器提供的印刷資料驗 BP01-065 抽二與
BP01-006 四點傷害與 BP02-052 固定手札數為 3 的 Uniform 傷害片段。BP02-052 的局面只放對手從者，
不承載混合護符的篩選、動態手札計數、進化、棄牌或打出程序。這些測試不包含 BP01-065 的
SpellChain／PP 回復，也不包含 BP01-006 的打出時目標選取與整卡准入；不宣稱整卡已遷移完成。
BP06-110 未准入。

## 合成結算分配

Distribution 僅准入必選、Public 的合成 ResolutionChoice；來源座標的 card_no 必須是
`synthetic`，不將此入口視為已審真實卡文證據。PlayParameter、付款、AtSelection、
模式及整張 CardProgram 仍整棵拒絕。真實結算卡沒有時點與 minimum 來源也拒絕。
分配與 Search／Order／UpToStepwise 混用時也在完整 requirement 預檢拒絕（含死分支）；
這些 answer handler 尚未區分錯誤回答與驗答成功後的提交失敗，不能承諾分配的整次回復。

每題求出並去重 ResolvedRecipients，凍結 total、IR minimum、snapshot 與
`(ExecutionId, ScopeId, DecisionId, binding slot)` producer。只接受目前場上從者與主戰者，
場外捕捉證據、非從者、過期域都拒絕。非空域 minimum×人數超過 total、溢位或 total
超出 Damage receipt 的有號範圍時，完整回復 execution，不發無解題目；不縮小域。
空域自動保存空分配，Allocation requested／actual 為零，後句仍執行。

題目只給 actor／Referee token 範圍的 handle 與公開印刷資訊；答案不能注入物件 ID／世代。
驗證完整 actor／token／epoch、恰好一項對應每個原成員、minimum、checked 精確總和。
漏項、重複、外來 handle、錯總量或錯回答變體保持同一待答狀態；已消耗 token 拒絕重播。
其他玩家不取得可操作題目。

Allocation Damage 只讀同 execution 的完整 producer，不重算 selector／total；requested
是答案和，actual 僅含仍合法原世代的正量。換區成員失效，不改選新世代、不把量補給
其他成員、不以剩餘生命封頂；零量不發傷害事實。同型 Damage receipt 可接既有投影。
合成測試對應分配契約的空域、最低量、凍結總量及失效成員要求，不宣稱 BP08-028、
BP16-077 的正式打出流程或整卡可執行。每項不同下限與 LKI 仍拒絕。

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
選牌、總費用、入場、觸發及整卡其他能力。成功移動 remainder、減費安裝與 Token
共享容量仍未准入；不能以此片段宣稱整能力已執行。

## 搜尋本體與單次分組回答

Search 准入搜尋者自己的 Deck、Single／Partition 目的地與同一次回答的 groups。
依[執行語義 §6](runtime-semantics.md#6-search-的輸入點回答與觀察)，開始後固定全域 filter、
各組交集、世代、有效基數與 snapshot；空牌庫或空候選仍給搜尋者一次問答。
對手的 awaiting 與物件投影不因候選是否存在而改變。

條件語法限 candidate 的 NamesIntersect、HasTrait、KindEquals 與其非空 And／Or、Not；
名稱清單須為非空常值，特徵視圖限 Original／Effective。只有無 groups 的常值 true filter
視為張數限定，minimum／maximum 截限到來源張數且須找足；可分類的條件搜尋 minimum
為零。groups 全部在同一 snapshot 回答，包含空組；驗全域與各組域、總量與各組上限、
跨組不重複及可選的 Names distinct_by。aggregate_bounds 非空、混合而無法分類的組及
其他私有述詞仍在開始前依 IR 形狀整棵拒絕。私有 evaluator 不擴充一般 If／capture／Move。

每題只提供 token 範圍的 handle 與印刷卡號／卡名；回答轉為 SearchSelectionAnswer 後
另驗基數與屬性，不以 local membership helper 通過代替完整驗證。選牌 Vec 不提供
目的地排序。找到後，條件搜尋先公開證據，再經共同 move_objects 提交，最後完整來源
牌庫洗一次；Fact::Shuffled 亦記零張／一張的 performed。洗牌撤銷先前位置與 Look 排列授權。
SearchOutcome 的 found／revealed 保留舊世代，moved 只記真正的 MoveResult；滿 EX 的
found 不變，after／actual_to 為空。手牌新 slot 不提供對手持續定位。

目的地限 Owners(Hand／Cemetery) 與搜尋者自己的 Shared(Field／Ex)。尚無容量複選
輸入，因此可找到的最大張數可能超過正數空位時，在讀取隱藏候選前拒絕情境；零空位
可記零成功移動。スペル置於場的結果尚未准入：張數限定搜尋進場在開始前整棵拒絕，
條件搜尋的回答若選到スペル即拒絕並維持待答。position／order、entry_effects、
OptionalSelfReplacement、成功投影與減費安裝仍拒絕。完整 CardProgram、lowering 與公開
capability 沒有擴充。

Partition 僅准入 Owners(Hand)→Owners(Cemetery) 兩段，且搜尋有明示 maximum，兩段
minimum 足以涵蓋該上限，以保證所有回答都會分配完 found；可能留下未分配成員的形狀
仍拒絕，其處理尚未定案。搜尋完成後固定 found 並先公開
條件證據；依印刷段序從 remaining_found 分配，基數各截限到剩餘張數，不替後段預留。
非空域即使只有一張或基數為零仍問；空段自動完成。每題有獨立 DecisionId、epoch、
snapshot 與新 handle，上一題 handle 不可沿用；錯答完整保留當前待答。分配先扣已分配
成員，再提交當段 movement；各段成功 MoveResult 合併到 SearchOutcome，最後才洗來源
一次。私有 domain 與 token 僅搜尋者可見，公開證據不給對手手牌固定位置。
合成測試驗零／一／兩張、兩席、錯答與末段後失敗的可觀察 RNG 回復；CP03-SL07 的
NameContains 尚未獲 D0 准入，不能將此程序宣稱成整張卡或其完整搜尋片段。

CP04-060 僅驗排除同名的ドラゴンズネスト搜尋、公開、進 EX 與洗牌，未接 UB、進化程序、
成功 after 減費及其他能力；BP13-035 僅驗已支付追加費用後的三組各一張搜尋、進場與
收尾，不涵蓋付款、入場觸發或整卡准入。兩者印刷資料由公開輸入產生器提供。

Public Order 依該文件 §7 維持拒絕，Repeat 的跨輪 epoch 依 §8 等待執行驗收。

## 固定數值修正

Modify 准入有號 Numeric Add、非負常值 Set 與非空遞迴 Numeric Combined；期限只接受 Installation anchor
的 NoTemporalExpiry 或 EndResolution，沒有 usage 或 interval。對象限目前場上從者，
已換世代的 capture 不適用；其他目前區域或卡片種類使整次解算回復。
同指示先固定去重對象，求完所有有號操作數，再規劃與提交。同一 Combined 中重複同 stat 的 Add 合計，相同 Set 合併；同 stat 的 Set／Add 或不同 Set 整棵拒絕，
不先截零；整數溢位拒絕並回復整份 Game。

型別化數值層與基礎值保存在 State；純求值器按 ProperPlay 時間套用固定 Add／Set，傷害另行扣除。
SetLife 安裝時逐目標固定 D_set，指定無傷基準為 S＋D_set；之後不重新捕捉。
基礎 2／3、已受傷 1 的 Set 1／1 結果為 hp=1、max_hp=2、D=1；再受傷 1 不會因重讀恢復。
提交時物化到原有物件的有效狀態，因此合法性、事件主體、view 與後續傷害共讀同一有效值；
Catalog 印刷值不變。Effective Current 的 Stats read 經同一求值器，限仍有效的 SourceHolder
場上從者。唯讀投影與讀值不分配效果 ID，也不發事件。

成功登記時 authority 分配 EffectId；沒有有效主體就不分配。同主體同指示只有一個
`Fact::StatChange`，內含非零 Attack／Life facets、實際 before／after／delta、Add／Set 操作、
instruction、execution、effect 與 commit 身分。兩個指示保持兩個事實；零差值不造 facet。
此為 authority 結果，尚未接完整 EventHeader、觸發及歷史查詢管線，不能當玩家 wire payload。

來源離場不移除非 static 修正；目標換區後原世代修正失效，不追蹤新世代。
NoTemporalExpiry 沒有額外時間截止。合成 closed Program 的全部本文完成後，先清除目標已換世代的數值層，
再移除綁定此次 execution 的整批 EndResolution 修正，從原始基礎與其餘 Add／Set 完整重算、保留目前受傷量 D，
最後執行既有規則檢查；指示之間不清場，體力降到 0 的從者在同一解算內仍是後續指示的對象。
到期不增加 stat_changed；最後一層消失後不保留過期基礎供後續重用。
閒置 Game 的基礎、固定操作與 ProperPlay 時間可存檔；待答與尚未解算的預備 play 不可存檔。
數值修正的 EndResolution 時序已定於[數值期限契約](numeric-timing.md#33-endresolution)，
目前只驗合成 closed Program 的生命週期；沒有真實 Numeric EndResolution 卡例，不由 lowering 自動補入期限。
待答不是解算完成，resume 失敗連同到期清理一起回復；真實卡的解決區清理、nested play 與完整 CardProgram 仍未接入。
持久數值層有效期間拒絕原型 decide，避免未遷移的 set／進化等寫入繞過求值器。
尚有預備 play 時也拒絕原型 decide；同一期間的 observation 在 `semantic_state` 只多一個 `typed_numeric_layers: true`
布林摘要，不含 effect、execution 或加數明細；belief 入口見到它即拒絕，
不把有效值當成原型基礎值重建另一份可 decide 的局面。

`prepare_internal_play` 是合成 closed Program 的可信適正 play 提交入口：先驗整棵樹及 context，
再保存封閉的 Program／來源／execution 關聯與單調 ProperPlay 時間，回傳不可由玩家構造的 handle。
`execute_internal_play` 只能解算該 handle 所綁定的本文及 context；缺關聯、重播或來源世代失效拒絕。
一般 `execute_internal` 將這兩步包為同一次交易；不代替真實卡的目標、付款或 play 合法性程序。
EffectId 仍在成功安裝才分配；反序安裝、唯讀查詢及存檔恢復都不改 ProperPlay 時間。

已准入的 Modify 全部固定對象及操作數，彼此不改變對方的適用對象或操作，故沒有動態依存。
跨指示同 timestamp 的 Set／Add 或不同 Set 會保守拒絕整棵樹，包含死分支，即使 selector 可能互斥；
不同 stat、純 Add 與相同 Attack Set 可交換。多次 Life Set 可能捕捉不同 D_set，亦整棵拒絕。
沒有以安裝次序、EffectId 或陣列次序補出非交換 tie；ContinuousTieOrder 的規則輸入仍待後片。

Continuous 安裝及其動態依存、CurrentTurnEnd／NextTurnEnd、interval、SetKind、
Grant／RemoveAbility、Modify usage、負 Set 與非常值 Set，包括未執行分支，仍整棵拒絕。
Set 層有效時的進化／基礎替換由既有原型 decide guard 保持拒絕，未宣稱差額換算完成。
CP02-007 與 BP01-011 只驗已選場上從者的 +1/+1 片段；不包含成本、選牌、EX、
觸發或整卡准入。BP16-049 只驗受傷對象的 Set 1／1，未承載 SpellChain capture、
RemoveAbility 或整能力；實際回合期限與非連續區間仍待後片驗收。
