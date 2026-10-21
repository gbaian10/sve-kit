# 內部已驗程式體執行入口

`Game::execute_internal` 接受 `WellFormedProgram`、來源座標及 authority 提供的來源物件／世代／控制者。
它只供合成與卡片段測試使用，不接作者 YAML、不執行整張 `CardProgram`，也不發布公開 capability。
`Registry::current().supported()` 仍為空；良構通過不能取代本入口的執行准入。

## 整卡起動的內部邊界

`load_internal_card(card_no, WellFormedCardProgram)` 在同一個 Game 載入完整的手寫整卡，
不接作者 YAML。此入口目前只承載以卡號識別卡片、以十進位 ordinal 識別 face 的內部輸入。
所有能力都必須是 Field 的 Printed Activated，PlaySpec 的 captures、parameters 必須為空。
Ordered costs 僅准入 controller 的非負常值 PpCurrent Pay，以及恰好 SourceHolder 的 Acted
Posture；成本 actor 必須為 controller，instruction source 須與能力相符。不接受 materials binding。
Spell、Triggered、Static、其他成本、成本替代、模式、其他 play metadata 與介入能力整棵拒絕。
載入逐能力驗證 card／face／line 座標一致、印刷座標唯一、instruction source 相符，以及完整本文的
封閉 requirement 集合（含未執行分支）。另以正向集合限定同步 Draw Exact、Look Exact、Uniform Damage、
已支援的 Modify／InstallEffect、capture、Sequence／If；互動與可能移動來源的 Move／Destroy 亦整棵拒絕，
未接 LKI 時不以來源離場後取消本文代替規則。

`activate_internal_card(holder, AbilityId, actor)` 只從已載入的 holder 卡片取得定義；呼叫者不提供
來源、世代、instance 或 execution。所有對局物件的整卡都必須已載入，沒有能力的卡也要明示空整卡；
原型能力、keyword、continuous、delayed、待機及忙碌 context 維持拒絕。
動態准入限主動玩家 main 時點、目前場上未失能的從者、當前 face 與 controller。
同 holder 世代與定義重複起動沿用 AbilityInstanceId；新世代的鍵不同，每次起動另配 ExecutionId。

成本依序在同一 authority 交易提交，不另用一套 can_pay 預估。PP 必須足額扣除，Act 成本必須
實際把站立來源改為 Acted；後項不足、重複 Act 或本文失敗皆回復整次起動。
成功的每項成本以 `InstructionOutcome::Payment` 保存完整 quantity 與改變姿態的來源世代，
其中 quantity 為 requested 與 actual 相等的成功量；成本 receipt 的程式讀取仍未准入。
姿態成本走共同 change_posture 原語並先形成 Posture fact。
所有成本完整付款後才形成 authority 的 AbilityPlayed fact，接著執行完整本文、EndResolution 收尾，
最後形成同 producer 的 AbilityResolved fact，回傳完整 receipts 與 facts。
這些型別沒有 wire 編碼，也不宣稱已接正式事件訂閱者。執行錯誤回復整次起動前的 Game，包含
instance／execution 配發、RNG、事件、觀察與額度；本子集不會外發可撤銷的待答 token。

回傳明示 `ResumeStage::CheckTiming`，Game 同時保留該階段。在
`resume_internal_card()` 前，原型 decide／submit、另一個 typed 入口與起動都拒絕。
checkpoint 期間玩家 projection 的 awaiting 為 null。resume 才交既有規則檢查管線，
回傳該階段產生的規則事件，不重送本文事件；不自動推到 end／discard，本文中不插入致死或抽空檢查。
載入整卡的 Game 尚無儲存與 digest 格式，序列化與 digest 都明確失敗，不靜默遺失整卡或 instance。
本子集尚未承載正式卡片打出、PlayParameter Distribution、目標選取、其他成本、互動續體或真實卡准入。

## 准入與原子性

入口以 `Requirements::derive_program` 走訪完整程式體，包含未執行分支；每一個 exact
capability code／version 及節點形狀都必須在封閉支援集合內。附加的 context 檢查不允許
既有 frame、prompt、待機、continuous、delayed、未准入的原型能力或 keyword 進入本片執行環境。
這些限制避免尚未遷移的取代、觸發或禁制被略過。卡片印刷資料仍由同一份 Catalog 取得。

支援 Sequence、If、root AtResolution 的 Int／Nat／Bool／Selector／Player capture，以及 bounded
integer ResolutionChoice。數值支援常值、已驗 binding／capture、公開玩家資源、公開區域
計數、checked Nat 轉換、Add／Subtract／Multiply／Minimum／Maximum、條件值與 Draw 的 Cards、Damage 的
同型 Requested／Actual quantity receipt 投影；布林支援比較、And／Or／Not／Exists。selector 只接受公開 Field／Ex／Cemetery／Banish
的 Current view、SourceHolder、Objects／Union 及明示 capture；未准入的 read／selector 不以零或空集合代替。
一般隱藏資訊 If、Repeat、付款、模式及下節以外的安裝／action 都在執行前整棵拒絕。

Game／State、區域順序、RNG、Knowledge、事件與待機仍使用原有權威狀態。
typed ObjectId 由 State 內的持久映射配發給原型物件 ID，同一局內不重新編號、不重用，存檔後沿用。
內部續體只存 typed node、帶 ScopeId 的 slot、typed quantity receipt 及 input occurrence。
入口失敗與錯誤回答還原整份 Game，包含私有續體、事件 buffer、RNG、使用紀錄與輸入 epoch；
有效回答後的執行失敗依〈互動回答的交易分界〉回復 execution 前的 Game 並取消續體；
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

合成 ResolutionChoice 的 Allocation／Distribution 子集見下節。來源、凍結、無解拒絕及
selected play targets 的最低 1 點限制依
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

## 互動回答的交易分界

actor、完整 token、回答變體、handle、permutation、groups、基數及不同名限制先以唯讀方式驗答。
Search 的不同名限制依題目凍結的印刷名稱，不能改用付款或移動後的 live 名稱。
錯答保持同一題與完整 authority；Search 選取 Spell 入場、可選目的地未具備兩個可執行去向、
以及未能解讀的 found-local 費用，維持既有的驗答拒絕，不先公開、移動或消耗 token。

驗答成功後，snapshot／世代重驗、第一個 mutation、接續指示、下一題的建立、洗牌與
EndResolution 清理都屬提交。任一步失敗回復本次 execution 的 origin，撤銷前面已回答的
移動、RNG 消耗、事件、待機、額度及觀察；不退回最後一題。此規則也適用 Integer 與
SearchReplacement。驗答錯誤比對待答狀態，提交錯誤比對 execution 起點；兩者均須核對完整 authority 與觀察。
未完成的 Progress 是 authority 的暫存進度，不是已提交的對外事實。
抽牌遇到世代耗盡拒絕，不沿用原型的飽和世代。

整次回復保留已外發 epoch 高水位；重試須取得新 token，舊 token 不會復活。
明示 prepare 的 proper-play association 可按既有契約保留，但重試題目的 epoch 仍前進。
整卡入口只准同步本文，付款後不會在同一次起動中暫停詢問；本節的整次回復不涵蓋同次起動付款後的互動。

## 合成結算分配

Distribution 僅准入必選、Public 的合成 ResolutionChoice；來源座標的 card_no 必須是
`synthetic`，不將此入口視為已審真實卡文證據。PlayParameter、付款、AtSelection、
模式及整張 CardProgram 仍整棵拒絕。真實結算卡沒有時點與 minimum 來源也拒絕。
分配可在封閉合成 Program 中與 Search（含 Partition／可選目的地）、Order、UpToStepwise
交錯；所有互動延續共用上節的驗答與提交分界。整卡入口只准同步本文。

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

一般 Move 只准入 Put、Owners(Cemetery / Banish)，來源限公開 Field／Ex／Cemetery／Banish。
不接受 Shared(Field / Ex)、私人目的區、position、order、initial_posture 或 entry_effects；
唯一例外是〈私有看牌與同區排列〉所述，從自己牌庫放入自己 EX 的受限 Look→Move 流程。
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
Movement receipt 的成功 Before 與 SameEffectExplicitReference MoveAfter 已准入
Move／Search／Destroy 的 local receipt。只取具有 after、actual_to 與實際世代轉換的成員；
不以 found、revealed、選中集合或 requested_to 代替成功移動。Before 保留舊世代，After
只指向該次移動的新世代，再次移區後不自動追蹤。一般 Difference 依物件 ID 及世代扣除，
不依卡號或卡名。這些集合不代表 Destroy 事件或完整付款。
MovedRef、RuleCarry、FollowEventMove、其他 receipt 家族與事件 payload read
仍在完整 requirement 預檢時拒絕，包含未執行分支。receipt 與 after 世代不進玩家 packet。

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
Look 的私有 selector 只在 Order cards 或下述受限 Move 的 Union／Difference 主幹准入，
不放寬一般 If／capture 或主幹下其他 selector（例如 Zone predicate）的讀值範圍；
未執行分支同樣檢查。

獨立 Look→Move→Order 流程另准入明示自己 EX、Put、無 position／order／initial_posture／
entry_effects 的 Move。物件主幹僅由 Look receipt、Union／Difference 構成，逐張重驗
自己牌庫的目前世代及有效位置證據；不接受任意 Deck selector 或另一玩家的目的區。
EX 足夠時全批移動；已滿時每個嘗試保留無 after 的 MoveResult；僅剩部分容量而必須
另問選牌時仍拒絕並回復整次執行。真正 remainder 由 looked 減該 Move 的成功 Before
取得，再接既有私有單 Placement Order；EX 滿沒移動者仍在 remainder，同名副本互不替代。
此流程不執行 Search，不增加洗牌，也不把 Order 插進 Search 的末尾洗牌之前。

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
選牌、總費用、入場、觸發及整卡其他能力。成功移動 remainder 已有上述受限 EX 流程，
減費安裝與 Token 共享容量仍未准入；不能以此片段宣稱整能力已執行。

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
成功投影與減費安裝仍拒絕。完整 CardProgram、lowering 與公開
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

OptionalSelfReplacement 准入搜尋上限至多一張、替代目的地為搜尋者自己的 Shared(Field)
且無 placement／entry_effects 的片段。條件限布林常值，或 SearchFoundAny 的局部
Current 物件之 Effective Cost 經 InformationOrZero 與常值比較「以下」。先固定 found、
依搜尋規則公開證據，再讀有效費用；該私有讀取不授權一般 If／capture／Move。
有效費用使用目前物件的 cost 資訊，沒有覆寫時取 Catalog。Catalog 的費用只有「-」、null
或缺欄位視為無資訊而得 0；超出 i64、非整數或其他格式拒絕該回答並保持搜尋待答，
不當成 0。目前 typed numeric 僅修改
Attack／Life，原型能力、keyword 與 continuous 仍由 context 拒絕。放寬這些前提時須
重新驗證有效特徵讀取，不能把此處的 fallback 當成完整費用層。

零 found 或條件為假不問目的地，直接依原指示回 Owners(Hand)，最後仍洗來源一次。
條件為真且非空時先以隔離副本確認兩個目的地皆可執行，才發獨立 DecisionId／epoch／
snapshot 的 SearchReplacement 問題；答案只有 Original／Replacement，不重選牌。
Original 回 Owners(Hand)，Replacement 採既定 movement，沒有失敗後自動回手。
滿場、スペル入場或需要特殊卡片移動規則時拒絕該回答並恢復搜尋待答；零張或條件為假
不受滿場限制。兩種回答共用最終洗牌與 SearchOutcome，found／revealed 留舊世代，
moved 才保存成功 after；選項與續體只向搜尋者／Referee 提供。

BP03-002 コスモスファング只驗搜尋獣、公開、找到物件的有效費用條件、可選回手／入場及
最後洗牌片段；印刷資料由公開輸入產生器提供，沒有承載進化、攻擊時能力、完整取代鏈、
場滿時可選替代的裁定或整卡准入。錯答完整保留待答；選擇後的提交失敗回復整次 execution。
RNG 回復以重新起動後成功洗牌與同種子控制局比對，不能只比較 Debug 字串。

CP04-060 僅驗排除同名的ドラゴンズネスト搜尋、公開、進 EX 與洗牌，未接 UB、進化程序、
成功 after 減費及其他能力；BP13-035 僅驗已支付追加費用後的三組各一張搜尋、進場與
收尾，不涵蓋付款、入場觸發或整卡准入。兩者印刷資料由公開輸入產生器提供。

Public Order 依該文件 §7 維持拒絕，Repeat 的跨輪 epoch 依 §8 等待執行驗收。

## 固定數值修正

Modify 准入有號 Numeric Add、非負常值 Set 與非空遞迴 Numeric Combined；期限只接受 Installation anchor
的 NoTemporalExpiry、EndResolution、CurrentTurnEnd 或 NextTurnEnd，沒有 usage 或 interval。對象限目前場上從者，
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
待答不是解算完成，resume 的提交失敗連同到期清理一起回復整次 execution；真實卡的解決區清理、nested play 與完整 CardProgram 仍未接入。
CurrentTurnEnd 保存安裝當回合的身分，在 7.4.8 整批移除；end 觸發、Quick 與棄牌檢查期間仍有效。
NextTurnEnd 在安裝時捕捉玩家，等唯一回合排程實際開始該玩家下一回合時綁定其身分，
再於該回合 7.4.8 到期。skip 不配置身分，extra 每個實際開始的回合各有身分；
不以 elapsed_turns、預定回合或固定加一／加二換算。到期清理已經過去的回合不接受追認安裝。
到期造成 hp≤0 時，先執行下一實際回合的重置、PP、站立與抽牌，在 7.2.5 處理規則破壞，
再進入主要階段並收集 main_start 觸發。到期、重算不發 stat_changed。

原型 decide 可在數值層存在時繼續執行；傷害透過同一有效值重算並保留目前 D，
原型純 Add 以 checked 基礎差額重算。進化只在該主體的層全是無期限 Add 時以基礎差額接受：
此時保留層或改成差額的結果相同。有期限的層在進化後如何到期還原，屬[數值期限契約 §1.1](numeric-timing.md#11-進化時的差額)
的疑義，因此有期限層或 Set 層的主體進化整次拒絕。
prototype Set／種類或 pilot 基礎替換與有效數值層混用仍拒絕；Set 有效時任何基礎差額也拒絕，
不為原型的非交換指示捏造 ProperPlay 時間。pilot 還原未遷移，因此有 pilot 的目標不接受 typed 數值安裝。
原型運算式仍是飽和算術，因此對有效數值層的目標，原型 Add 與傷害量只接受整數常值，
傷害替換（其增減與指定量同樣走飽和算術）一律拒絕；沒有數值層的目標維持原型既有算術。
換區重建基礎時清除舊世代數值層，不把修正帶入新世代。

合成入口允許不消費 typed 事實的 prototype spell／activated／evolve 及 end／main_start 觸發並存；
其他 prototype 觸發、static 與關鍵字仍整體拒絕，尚未接 typed 事實的觸發消費。
數值層存在期間（輸入開始或結束時有層），每個原型輸入結束時以同一白名單重驗全部物件的能力與關鍵字、
待機中的能力及延遲觸發；授予、產生物件或登記非 end 延遲監看而越界時整次回復。
這避免 7.2.5 規則破壞引出的待機能力和 main_start 觸發混在同一次待機。
期限玩家可使用已捕捉的 Player 值；Capture::Player 允許 Current 來源的 controller／owner，
世代失效時拒絕，不追蹤新世代或當作缺資訊回零。live role 不直接充當 actor／audience。
NextTurnEdge、phase edge、非空 intervals、Continuous 與 SetKind 仍拒絕。
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

Continuous 安裝及其動態依存、interval、SetKind、
Grant／RemoveAbility、Modify usage、負 Set 與非常值 Set，包括未執行分支，仍整棵拒絕。
Set 層有效時的進化／基礎替換由數值寫入邊界拒絕，未宣稱差額換算完成。
CP02-007 與 BP01-011 只驗已選場上從者的 +1/+1 片段；不包含成本、選牌、EX、
觸發或整卡准入。BP16-049 只驗受傷對象的 Set 1／1，未承載 SpellChain capture、
RemoveAbility 或整能力；非連續區間與上述未承載的整卡部分仍待後片驗收。
