# 內部已驗程式體執行入口

`Game::execute_internal` 接受 `WellFormedProgram`、來源座標及 authority 提供的來源物件／世代／控制者。
它只供合成與卡片段測試使用，不接DSL YAML、不執行整張 `CardProgram`，也不發布公開 capability。
`Registry::current().supported()` 仍為空；良構通過不能取代本入口的執行准入。

正式卡片與能力打出的程序以[正式 play 契約](play-semantics.md)為準；以下列出執行器已驗子集。
已選目標與純固定 Add 跨進化依[目標與進化契約](target-evolution-semantics.md)執行；
以下記錄已驗的穩定種類／タイプ條件與限期 Add 子集。
常見觸發與來源離場的後續義務見[觸發與 LKI 契約](trigger-lki-semantics.md)，不改變下列准入限制。

## 整卡與正式 play 的內部邊界

`load_internal_card(card_no, WellFormedCardProgram)` 在同一個 Game 載入完整整卡，不直接解析 DSL
YAML。印刷 face 的十進位 ordinal 與 lowering candidate 的 `base`（對應 face 0）都保留原本
owner、field、fragment；這不授予永久來源身分或公開 capability。整卡的良構入口環境直接帶入
本文，不把依賴 play binding 的本文重新當成封閉 `WellFormedProgram`。
正式 current 卡由 `Game::from_selected_effects()` 接受完整 accepted selection，先核對來源與
快照面的唯一對應，再自動呼叫相同整卡准入；不能只傳一份良構程式冒充 accepted 卡。
原型與 current 維持不相交，current 的永久來源座標不改寫為 `base` 或卡號；
選用結果的原型 Catalog 與 current 候選以同一份固定集合保存，替換任一者都使整次建立失敗。
能力篩選、printed characteristics、事件與離場 LKI 共用 catalog 邊界解析出的面對應；
沒有對應即拒絕。來源 sidecar 與完整程式保存在 authority，不發布 capability 或新存檔欄位。
base face 的普通打出費用必須是非負數字；缺欄、null、「-」、其他非數字及負值拒絕，明示的零費用仍有效。
非打出用的進化 face 不套用此數字限制；能力引用 face 的原有缺欄檢查仍適用。
Printed Activated 的印刷面限普通從者、官方進化從者（フォロワー・エボルヴ）或護符；卡種由同一 Catalog face 驗證。

准入接受普通 Printed Spell 的 `[Resolution]`、Printed Activated 的 `[Field]`，以及下節的
Printed Fanfare／Lastword／OnAttack 及 End Begin。Spell 的唯一固有 Ordered PP Pay 必須等於同 Catalog face 的印刷費用；
只在這個已驗位置按 Resolution 的有效成本實付一次。Activated 沿用 controller 的非負常值 PP
與恰好 SourceHolder 的 Acted Posture／Put 送往 owner 墓地，不能套用卡片減費；成本沒有 materials、precondition 或替代。
前置 X、追加／替代成本、一般 trigger、通用 Static 與任意對象 LKI 仍整棵拒絕。
Quick 僅接受 [Quick 契約 §1、§5](quick-semantics.md) 的普通 Spell 獨立 Static 許可與
普通場上從者的指定 Activated；共用精確形狀檢查，不開放一般 PlayCard Permission。
目前只由內部 typed 輸入承載；DSL 入口與正式來源 lowering 尚未完成。

載入逐能力驗證 card／face／line、印刷座標唯一、所有 instruction 的來源閉包，以及包含未選模式
與死分支的 requirement 集合。Fanfare Keyword 例外只接受已驗 instruction source 的精確位置，
不對同名 capability 全域放行。本文仍限 Draw Exact、Look Exact、Uniform／Allocation Damage、
已支援的 Modify／InstallEffect、Move／Destroy、Search、Order、capture、Sequence／If，另支援下述 ChoiceKeyword。
Move／Destroy 沿用已驗公開區來源與 owner 墓地／消滅目的地；Search、Order 與 Look→自己 EX 的
觀察 Move 只收〈私有看牌與同區排列〉及 Search 各節的同一受限形狀，整卡不另放寬。
這只是執行支援，不代表正式來源或 qualified 已驗；未受驗連結、特殊卡片、入場效果與其他互動本文保持拒絕。

`play_internal_card(holder, actor)` 接主動玩家 main、自己的 Hand／EX、base 普通 Spell 或已准入的
普通從者／護符；`activate_internal_card(holder, AbilityId, actor)` 接已載入的當前場上從者／護符能力。
來源、世代、instance 與 execution 都由 authority 建立；起動讀目前有效的印刷定義，`base` 比對 face 0。
Printed、Field 起動可帶 `PlaySpec.limit` 的每回合 1／2 次額度；以 AbilityInstanceId 與實際 TurnId
計數，全部付款完成的 AbilityPlayed 才耗次。耗盡回 IllegalPlay，不出題、不付款；失敗回復沿下節。
current 局面可保存未載入整卡的物件；單純存在或不讀取能力的快照計數，不因缺 program
整局拒絕。打出、攻擊、指定能力、Quick 許可及完整事件能力資料／LKI 的能力集合查詢仍要求載入，
真正無印刷能力的普通卡也須明示整卡，不能將缺 program 當空能力集合或回退原型效果。
進化資源沿既有角色准入，可先完成數值與資源程序，再於未准入能力的 pending 邊界停止；
該角色不代表能力完整載入。typed printed set 與完整事件能力資料／LKI 仍須有 program 或既有的明確空卡文證明。
`Game::query` 的既有公開區域張數路徑（例如 `P1.hand_count`）直接讀取區域，不先建立可能需要能力的完整投影；
所有視角得到相同公開張數，不回傳隱藏物件身分，也不新增 packet 欄位。

current 不以原型 header 判定未載入卡的 active zones。需要列舉訂閱、限制、取代或交換付款介入時，
若不能證明某物件的能力與該次操作無關，即以卡號、有效面、holder 世代及查詢用途精確拒絕。
移動取代、關鍵字／能力授予、減費、觸發抑制與重複觸發等直接掃描場上來源的讀取，也須先檢查此能力需求。
這仍可能使有未載入牌庫／手牌卡的實際事件或回合推進停止，不保證任意局面都可完成；
不能只排除非場上卡而漏掉手牌棄置等觸發。沒有實際 subject 的非 phase 事件不查訂閱。
已知 definition 的要求保留 AbilityId；未知能力集合明示定義不可用，不捏造來源行。
authority 保留完整錯誤，玩家 projection 的 unsupported 錯誤不包含隱藏卡號或物件身分。

未准入的原型 keyword、continuous、delayed 與忙碌 context 仍拒絕。
已載入 Spell／Fanfare 時封住合成 prepare 與原型打出，不略過正式 producer。

上述入口回傳 `CardPlay`：`Awaiting` 只有本次 token、actor 與授權的題目；`Resolved` 含完整
`Activation`；從者／護符卡片 play 的 `Entered` 含 `CardEntry`（本次 execution、完整付款／card facts、
事件與下一階段）。`answer_internal_card(actor, token, payload)` 不收 caller 的 source、snapshot
或環境；合成 `answer_internal` 不能回答正式 play。待答期間不發布部分完整 Activation。
從者／護符規則付款以該次 execution 的 `ReceiptId(0)` 保存，Fanfare 另有 execution 與 receipts。

### 普通護符的執行子集

此子集限印刷攻／體皆為「-」的普通護符；有印刷數值的護符（如 BP16-U06）在載入時拒絕，
不以缺值零規則誤讀其資訊，也不開操縱／種類變更。普通護符與從者共用上述 Hand／EX → Resolution → Field 程序、固有 PP 付款、付款後五格
場容量及失敗回復；護符自身的打出費用修正（`card_play_costs`）仍在載入時拒絕，[自身打出費用修正](#自身打出費用修正)只承載從者與 Spell。
護符起動沿相同的 `[Field]`／Printed 准入，支援非負常值 PP、SourceHolder
的 Act 與 Put 至 owner 墓地；目標先選完才付款。自身送墓仍以離場前世代作為傷害來源，
來源數值沿既有 SourceInformation／LKI 讀取；缺少攻／體的資訊直接讀值為零（CR 1.3.6.3），不將護符當從者。
[數值期限契約 R-B](numeric-timing.md#5-可執行局面驗收清單) 要求的比較為 false 尚未承載，lowering 也還不產生這類讀值。
攻擊、被攻擊與從者目標仍由既有卡種檢查拒絕。

護符的 Fanfare、Lastword、已支援的 Printed EnterField 與 End Begin 沿共同 producer、
pending 與 LKI，不另設護符待機。護符入場仍可被「從者入場」條件排除。
lowering 與良構檢查原已按能力形狀而非卡種承載上述成本／觸發，這片不新增DSL 語法或 IR 欄位，
亦不擴大 `Registry::current().supported()`。

BP01-041 的候選起動片段由 C0 展開、lowering、良構與整卡執行驗 PP／Act；BP20-100
只以合成來源座標驗抽牌 Fanfare 與 Act／自身送墓／傷害片段。後者的 `usable_if`
（EX 的クレスト至少三枚）尚未由 lowering 承載，完整能力仍拒絕，不能刪掉條件宣稱整卡可用。
逐卡候選報告中的 `runtime=supported` 只表示機制子集可執行；C1 來源／資格／完整能力閉包
尚缺時，外層准入仍為 rejected。起動 `per_turn`、Quick、X、追加／替代成本、Static
及其他未准入觸發保持拒絕。

### 共同選擇 snapshot 與回復

`play.captures` 的 AtPlay 在自身移區前求值；同批 captures 先各自從入口求值再一起公開，
值域沿用良構 checker 與 executor 的交集。卡片先移入 Resolution；Spell 從這個新世代建立
RuntimeAbilitySource。ChoiceKeyword 在普通參數前選定，本文只按印刷順序執行保存的模式。
count 支援常值 exact／upto 與良構的 SC／NC 條件樹；在 AtPlay 固定計數，SC 數墓地 Spell、NC 數墓地全部牌。
常值按印刷模式數截斷，upto 正數至少選一個，零則保存空模式結果並繼續成本與其他本文。
先證明模式的必要公開目標與共同成本可行，再出題；不可選模式不縮減 exact 必選數。
只接受唯一頂層、controller、Public 的 ChoiceKeyword；模式 targets 限彼此獨立的公開 selector，
沒有 mode 內 Distribution、capture 或與根 parameters 混用，未選模式仍通過整樹支援閉包。
exact 1 可帶 `CurrentTurnAbilityInstance` 排除；Authority 以既有 ModeUse 保存
(execution, ability_instance, mode, turn)，AbilityPlayed 才原子提交，本文 false 仍保留紀錄。
DSL lowering 接常值 n／upto、單一 SC／NC instead 及 this_turn 排除；混合 exact>1、
多個 instead 的優先序、改計數母體的 rule modifier、Handoff、未支援的參數依賴、其他 actor 與任意私有 choose 保持拒絕。
Quick 起動的模式歷史排除在整卡准入保持拒絕；既有 Quick preflight 尚未承載歷史。
Quick 起動的 PlaySpec.limit 已由 Printed 限制拒絕，不因本片擴張 Quick。
完整額度與模式語義見[チョイス擴充與起動能力限次 §1–4](choice-limit-semantics.md)。

所有 targets／play Distribution 共用移入 Resolution 及先決選項後的一份選擇 snapshot。
AtSelection 在驗答後讀這份 snapshot 與已公開 binding；不讀付款後的 PP。每個 target decision
限一個 selector group、公開場上未進化從者（候選含已進化從者時整次拒絕）、無 roles／distinct／aggregate；場上區域選取只接受 Current
視圖，玩家基準固定為 Controller；predicate 接受恆真、candidate 的 Effective KindEquals／HasTrait
及非空 And／Or、Not。有效種類與タイプ由目前特徵讀取；已准入的完整成本、本文與 context
閉包不含控制轉移、種類／タイプ修改、static 或未受驗 continuous，故仍在場的原世代條件不變。
不能以這次候選剛好匹配取代預檢；可能翻轉的管線及數值、姿態、進化狀態、名稱等 predicate
仍在付款前拒絕。lowering 保留DSL target 的 kind／trait 為 typed predicate 與原來源座標，其他欄位仍拒絕。
沒有分配時可有多個 target decision。Bound 只讀受驗的 target producer，與 InstallEffect affected 局部 binding 分開。
每個 Damage／Modify／Move 使用點固定完整輸入，核對原世代及存活目標的選擇特徵；
若特徵意外翻轉，整次回復 origin。離場換世代只使該指示無實際作用，不刪原 binding、不重綁或補選，
Damage requested 保留原量，Movement 成功投影不含失效者；獨立後句繼續，本文 If 與 AtSelection 不改位置。
存在 Distribution 時整棵僅接受唯一一組 targets，recipients 恰讀它的 binding、minimum=1，
且所有 targets 在分配前完成；不能為符合順序而重排相依參數。

非空分配無解時撤銷 targets、AtSelection、allocations 與舊 tokens，保留 AtPlay 與先決選項，
以新 epoch 重問目標。執行器在第一次問目標前，以兩席場位上限界定的有限候選集合證明是否有
合法方案；完全無解時不出題，直接回整次 play 前。空目標保存空分配並續後句。完整分配仍綁定 execution／scope／decision／slot
與共同 snapshot；Damage 保留原量，重驗原世代，不重綁或補分。

完整付款後才形成 CardPlayed／AbilityPlayed；從者／護符於付款後驗場容量，Spell 不要求場位。
Spell 本文完成後，僅把仍在 Resolution 的同世代卡移去 owner 墓地，再做 EndResolution 收尾，
最後形成 CardResolved／AbilityResolved 並交 `CheckTiming`。來源記錄保留打出時的 Resolution
世代，不把墓地新世代冒充來源。本文中不做致死／抽空檢查，不插入回應窗口，也不自行推到 end／discard。

錯 actor／token／payload 保持原題與 RNG；合法回答之後的付款、本文或收尾失敗回復整次 origin，
包含來源／instance／execution 配發、RNG、事件、待機、額度與觀察，僅保留已外發 epoch 高水位。
能力 origin 在付款與 prepare 前；選取待機能力的 origin 則是保存原 pending 與問題的狀態，不撤銷之前的卡片 play。
`IllegalPlay`、協定 `Invalid` 與執行器 `Unsupported` 分開回報。沒有任意取消或隨機成本重抽入口。

成功 Activation 明示 `ResumeStage::CheckTiming`，Game 同時保存 checkpoint；`resume_internal_card()`
才做既有規則檢查。在 checkpoint 前不能插入原型 decide／submit 或另一個 typed 入口。
整卡與待答續體尚無儲存／digest 格式，相關操作明確失敗，不靜默遺失程式或 producer。

### Fanfare／Lastword 的 authority 待機

Fanfare 保留 `Keyword(Fanfare, Printed)`，限定 AfterEntry、Field、PerFact，EnterFieldObject 恰等於
SourceHolder。Lastword 保留 `Keyword(Lastword, Printed)`，限定 BeforeExit、Field、PerFact，
LeaveFieldObjectBefore 恰等於 SourceHolder 且實際目的區為 owner 墓地。兩者都不接受 event captures 或 PlaySpec；限次只收下述共同正規形。其他 trigger 與整棵本文仍各自准入。Fanfare 的 Continuous 來源形狀仍拒絕。

普通從者／護符先入 Resolution，承接受驗 EX 費用後才決定並支付固有 PP，付款後驗場位。
EX→Resolution 與 Resolution→Field 只承接既有 CostModifier 的完整實例與目標新世代；其他效果／counter
承接未實作時拒絕。從者可以沒有 Fanfare，或在同一 face 有多個已准入的能力；不能用定義序決定結算序。
Fanfare 由共同入場 producer 保存原來源、controller、instance 與公開的入場來源區域。

共同移區原語在同批成功移區前，保存既有 pending 來源資訊與移動前有效的 Lastword 能力；
真正 Field→Cemetery 才建立 Lastword occurrence。成本 Put 與規則破壞皆走此處，不靠 Destroy facet。
回手／消滅不符合；完全被取代而未離場的動作不製造 occurrence。
已有進化關聯的持有者，其有效印刷能力來自受驗進化卡；離場前保存該印刷定義與來源資訊，
不沿 base 定義觸發。每個成功成員保留獨立原事實與
before／after、直接 cause；規則移區的 source=None，不冒充造成先前傷害的能力。
正式本文承載公開區 owner 墓地／消滅移動，以及下述公開自方從者入場。另依
[第二批觸發契約](trigger-second-semantics.md#5-自身離場)承載公開 Field→Owner Hand 的 Put；
回手後只保留公開 before 與 LKI，不讓對手定位手牌新世代。一般 Move receipt 的 MoveAfter
若指向實際回手成員，求值時拒絕並回復整次嘗試；這是執行期拒絕，不是載入期靜態拒絕。
其他一般回手及未准入的跨世代跟隨仍拒絕。

Fanfare、長寫 EnterField、Lastword、OnEvolve、OnSuperEvolve、OnAttack、Main／End Begin、
自身 LeaveField 與 Damage 共用一個私有待機集合。`internal_pending_choice()` 在穩定檢查時點建立題目，
回合玩家有待機就由該玩家選一個，否則由對手選。`answer_internal_card` 接 `Answer::Pending(handle)`；
候選只投影當次 handle、已公開卡號、印刷行與事件的過去公開場位／入場來源，不交出 source／instance／snapshot。
錯 actor、token、handle 或 payload 保留原題；選中後是獨立 execution，完成再交 CheckTiming。
`resume_internal_card()` 先跑規則處理到穩定，再讓下一題依當下待機重新決定玩家；不一次預排雙方。

已待機能力不因來源離場或換世代而消失；來源資訊讀取已封存 LKI，controller 仍固定為觸發當下。
內部錯誤回該次能力嘗試前，保留原 pending、題目及已成功的前次 card play，epoch 高水位不退。
正常敗北在 CheckTiming 成立時停止其餘待機並使舊題目失效；已完成本文的 resolved 事實仍保留。
一般 trigger play 目標／成本及 PerBatch 尚未准入。

BP01-118 與 BP01-P25 的測試由 C0 展開、lowering、整卡載入及 authority 程序驗證相應傷害片段；
P25 的送墓成本來自明示的合成測試輔助能力。來源座標為候選 fixture，不宣稱永久來源驗證或公開 capability。

`internal_rule_action` 僅接受 evolve、attack、end-phase、end-discard 與開局 mulligan 的既有規則動作，
沿原有時機、目標與支付／姿勢驗證；忙碌、待機及未完成 CheckTiming 時拒絕，不接原型 play／activate。
守護屬原型 keyword，載入整卡的局面整體拒絕，所以不開 guard-act。時機不符的動作回 `cannot-play` 等拒絕結果，不視為成功。
OnAttack 限 Printed 關鍵字、自身 Attack、AfterEntry、PerFact、無 PlaySpec；事件在合法目標與 act
完成後、Quick 與傷害前保存精確 attacker、target、controller 與當回合 ordinal。
End Begin 限 Printed、Phase(End, Begin)、自己的／對手的／每個 end phase，依事件回合玩家匹配；
只在 start_end 發一次，先待機再進守護 act、Quick、棄牌及到期，不因重入檢查重發。
此處 AfterEntry 指事件完成後，沒有把攻擊或 phase 當成入場。進化持有者從有效進化卡取得上述印刷能力。
選題入口不再自行執行會產生事件的規則處理；各 producer 或 resume 先完成規則穩定處理，避免遺失 Step 事件。

BP10-122 的 attack-time 抽一片段與 BP09-P17 的 end 抽三片段經 C0／lowering 驗證；後者只取抽牌指示，
不宣稱其完整能力已支援。攻擊測試加上合成待機能力，驗結算前來源加攻送墓後讀 LKI 4。

### 本文結算選擇場上物件

本文 `choose` 接 `ResolutionChoice`，在執行到該步時讀取目前的公開場上集合，使用既有
`Targets` 詢問封包及 token scoped handles。這與 `targets` 的打出參數分開：入場能力
不會因本文暫時沒有候選而阻止卡片進場，付款與進場保留在結算選擇之前。

准入範圍為控制者選擇、單組 Field、固定正數 `n` 或固定 `upto`，以及既有種類／trait
述詞。一般玩家別名、隱藏來源、可拒絕整項、群組、roles、回答時捕捉及精確零單位
形狀不在此子集。確切數量不足時依 CR 1.3.2 將詢問最低量降至目前可選量；尚有候選時
不能省略可完成的部分。`upto` 保留零下限。開始的選擇即使唯一或空域仍要回答，符合
[輸入點契約](author-syntax-1.0.md)；打出時 Target 的嚴格最低量不變。

回答檢查 actor、token、數量、重複及外來 handle，綁定同世代的公開物件集合，後續本文
重用既有 selector 的世代與特徵守衛。失敗回答回復至這次詢問，不能倒退已完成的卡片
進場或付款。公開場上從者與護符共用選項守衛；這項接線不授權取得秘密區域物件。

### 第二批自動能力入口

有限形狀與排除以[第二批觸發契約](trigger-second-semantics.md) §2–7 為準；
lowering、良構檢查與執行器沿用既有事件型別，不擴張公開 capability 或整卡准入。
OnSuperEvolve 在受驗超進化付款及加值後，與 OnEvolve 共用同一 Evolve 事實，由 controller 選序；
普通進化不觸發 OnSuperEvolve。首回合與正常回合完成 start 檢查後共用 Main Begin 入口，
待機完成前不開自由行動，返回 Main 不重發。

自身 LeaveField 與 Lastword 共用移區前的同批快照與 departure plan；僅實際離場才待機，
完全防止或取代後留場不發生。自身 Damage 在共同實際扣血提交點收能力與戰鬥正傷害，
0 或全防止不發生；致命傷先跑規則破壞，再選取仍保留原來源資訊的待機。
DamageAmount 只准入自身已綁定 Damage 事件的有限讀值路徑，不授權 closed Program 偽造事件。

DSL 的 `whose=each` 尚不符合現有 A.player-ref schema，DSL lowering 維持拒絕；
內部 typed Phase 可表示不限制回合玩家。其他物件監看、LifeChange、一般 Move 訂閱、
Grant／Delay、非 Field 入口、離場取代與 BP01-083 均維持拒絕。

### Quick 窗口

`internal_quick_choice()` 在攻擊或 end 的共同檢查與兩方待機全部完成後，建立
`Procedure::Quick` 的私有 `InputDomain::Quick` 題目；actor 固定為非回合玩家。
候選只列當前可付款且有合法模式／目標方案的受驗 Quick，以純讀取預檢證明，不試跑本文。
沒有合法候選仍等待 `Answer::Quick(None)` 明示 Pass；選行動用當次 handle，
模式、目標及付款仍依原 play 詢問。自己 main 繼續用普通打出與起動入口。

`answer_internal_card` 的 Quick 回答驗證 actor、完整 token、程序續體與固定 domain；
外部不能自製 source 或 window，原型 pass 與直接 Quick play 都不能繞過題目。
成功 play 仍交 `CheckTiming`，排空待機後同一窗口再配新 token；攻擊者離場也不提早關閉。
Pass 回傳 `CardPlay::Continued(Step)`，才續接原攻擊或 end 棄牌程序；戰鬥比對宣告時的
攻擊者及目標世代。合法選擇後失敗則回本次 play origin，以新 epoch 恢復原窗口，
先前成功行動保留。終局清除窗口，私有候選不投影給對手。

進化規則入口只承接受驗的固有進化正規形：同名、自方未公開 EvolveDeck 資源、Reveal 後支付固定 PP，
以及單一普通 Evolve 指示；不開一般本文的 Evolve 或 EachObject。只有已載入整卡的持有者走此入口，
未載入的原型持有者照舊使用原型進化。來源及資源只收 face 0，
沿既有時機、每回合次數、PP／EP／SEP 規則驗證；一般與合法超進化的 evolve facet 由實際付款決定，
不信任輸入的 kind。全部程序完成後才從進化卡有效印刷面取得 Printed OnEvolve，自身、AfterEntry、PerFact、
無 PlaySpec。進化相當能力仍拒絕；OnSuperEvolve 見〈第二批自動能力入口〉。

進化保持場上持有者世代；進化卡另有物件世代與受驗關聯。事件保存完整 before／after characteristics，
新印刷定義依卡號、face、局部 AbilityId 配發新 instance，不與進化前同號能力相撞。
成本依規則 5.16.1.2 保留 base face 0 的印刷值與既有成本修正；Original 攻／體改讀進化卡。
跨進化僅接受下文的純固定 Add 子集，Set 與其他期限／區間仍拒絕。BP20-R14 只取進化時抽一片段，
固有進化 PP 入口為明示合成案例；同一代表局面驗一般／超進化 facet、新 instance 及進化後 Lastword。

長寫 EnterField 限 Printed、自身或自方／對手的從者入場，可排除自身、匹配受驗特徵及自回合條件。
只收凍結事件資訊的正規形；同批所有成員完成移區、數值清理與入場 counter 後才匹配，
新入場監看者也觀察同批其他成員。Fanfare 共用此 producer，不額外要求 play。
沒有監看者匹配的入場不產生事件；有匹配時才讀入場來源區，Deck 等未准入來源（含被放入者自身的
Fanfare）拒絕整個動作，不靜默略過。
事件保存新世代、真正來源區、controller、有效 characteristics 與 Play／AbilityPut；經 Resolution 仍保留原 Hand／EX。
場間控制移轉不產生入場；未受驗入場效果、Creation、隱藏來源與一般事件條件拒絕。

新增的正式 Move 只收自方公開 Cemetery／EX 的普通 base face 0 從者送至 Controller Field，
容量須直接足夠，不接受選位、初始姿勢、entry effects、特殊卡片或連結。這是 Move 的窄子集，
沒有開放一般 Shared 目的地或觀察牌以外的隱藏區操作。

限次只收 `maximum=1 / Turn / AbilityInstance / PendingAdmission`。用實際 TurnId 與來源 instance 記錄，
進入待機就占額度，本文條件不成立或依法不能打出不退還。同回合再次掃描不重置；新實際回合才可再占用。
同一批次對同一 instance 有多個候選而只餘一格時，由 controller 選一個事實；回合玩家先，
後選者可看到先前公開選擇。額度與所選待機在全部選擇完成後原子提交。
`InputDomain::TriggerQuota` 與 `Answer::TriggerQuota` 使用當次題目 handle，不接收 event／object／snapshot。
這是原事件交易中的續體，先選完才執行後續本文，不開 Quick 或提前解決待機。錯答保留原題，
內部失敗回外層 origin，連同移區、額度、pending 與觀察一起回復；epoch 高水位不退。

事件的「それ」只准本次 EnterField binding 的 `ObjectFrozen::EnterFieldObject`，作為直接 Move／Modify
的 Objects subjects（含 Sequence／If 本文）；authority 在 pending 轉 execution 時保存原入場 ObjectRef。
之後已離區就依舊世代處理，不追同實體的新世代；其他事件讀取、任意 AtEvent／TriggerEvent receipt、
泛用 MovedRef／RuleCarry 仍拒絕。BP20-028 的一次／兵士從者入場抽一種子及兩個代表局面驗同批新監看者、
兩席額度選擇、實際回合重置、origin 回復，以及舊 LKI 4／後續入場 LKI 7 與新世代有效值 8。

`concede_internal(actor)` 依投了規則立即結束，即使正在等待正式 play、合成 prepared 本文或待機選序也可用。
回傳 `Terminal` 與已執行前綴的 `Progress`（complete=false）／Step；保留現有狀態、付款與已發生的事實，
釋放待答續體、外層 origin、prepared handles、所有 pending 與問題，舊 token 不再可答。不回復 origin，
不執行剩餘本文或結束清理，不生成尚未完成的 CardResolved／AbilityResolved。若先前已完成並回傳 resolved，
其事實不因稍後投了而撤銷。無效玩家與已結束對局拒絕且不變狀態。
正常生命歸零／抽空仍在完整本文之後的 CheckTiming 處理；來源消失不當作終局，卡文直接勝敗仍拒絕。

### 正式能力來源資訊的受限 LKI

`ObjectView::SourceInformation {}` 只在受驗整卡能力本文讀 Cost／Attack／Life（Original／Effective）
及作為能力傷害 source。封閉 `WellFormedProgram` 即使帶相同 instruction source 也拿不到資格；
play 時點的 captures／參數、role／selector 等其他資訊用途、任意 `LastKnown` 與 caller snapshot 仍拒絕。
Original 只在讀 SourceInformation 的數值位置放行，其他位置仍屬私有讀值。
正式 lowering 的傷害來源使用此正規形，合成 Current 的意義不變。

能力 execution 固定來源的精確世代與參照區域。來源仍在該區域時即時讀有效資訊；本文 Move／Destroy
真正移區前，在同一批 before 狀態保存已算出的有效數值與卡表 Original 數值，成功提交才封存。
後續讀值與傷害沿原 execution 使用該 LKI，不重新付款、不查新世代；同批清理的修正不會提前消失。
Spell 的 Resolution 收尾也保存來源資訊，且只收尾本次原世代。來源資訊隨 execution 與 origin 保存，
執行錯誤連同付款及已執行本文一起回復。stale Current 或缺 producer 是錯誤，只有合法資訊中
不存在的欄位（例如 Spell 的攻擊力）才按規則為零。Current 數值讀取維持只讀 Effective。
Spell 的來源資訊綁定進 Resolution 後的新世代；本文 Move／Destroy 不接受 Resolution 中的物件。

此子集也保存上述待機來源的後續離場資訊、受限成本送墓與共同規則移區。公開入場與回場新世代也更新此生命週期；其他隱藏區入場、Creation 與未受驗的跨區承接仍拒絕。

整卡能力的 `Action::Move` 到 controller field 可將 own follower Advance 從 EX／evolve deck
置入場，保留 face 0、owner／controller 與場容量檢查。evolve-deck 僅在該指令的精確 selector
位置接受 SelfPlayer／Controller、Current、單玩家與常真集合；不開放一般私有區查詢或具名選牌。
進場走完整 EnterField／AfterEntry／SourceHolder Fanfare pending，不直接執行能力本文。
這不授予新的玩家動作或未印的 AdvanceActivate；evolve-deck 直接打出維持拒絕。
此子集以明示合成完整 current 卡驗證 runtime，不代表 BP19-P32 等官方卡已完成作者遷移、
關鍵字承載或整卡 executed／qualified。

### 成功 After 的當回合減費片段

`InstallEffect CostModifier` 只接受 InstructionStart、成功 `MoveAfter`、精確 affected binding、
常值非正 Delta 與 CurrentTurnEnd／Installation；內外 Duration 必須相同，不接受 usage、
intervals、Set 或其他期限。每個成功的新世代必須仍在 EX；EX 滿、沒有找到或沒有實際移動時，
安裝 receipt 為空，不配發 EffectId，也不把 found／revealed／chosen 當安裝對象。
修正保存該 execution、ProperPlay timestamp 與目標世代；下一次移區後不沿同實體追蹤。

搜尋的 found-local 費用條件、IR Effective Cost 與 SourceInformation 只讀卡片 Cost 資訊，
不套用 `when_playing` 修正；原有無費用資訊的搜尋 fallback 見搜尋替代目的地一節。
進化來源依受驗關聯讀 base face 0 的成本，未知或無效關聯拒絕。
原型付款仍承接已安裝的減費，typed 與 prototype continuous 費用層不得混用。

### 自身打出費用修正

`CardProgram.card_play_costs` 以每 face 唯一的非空 `FacePlayCost` 承載自身修正，
`PlayCostModifier` 保留各自來源。lowering 只收自身 `play.cost_delta`／`cost_set` 的常值，
不生成 InstallEffect 或 EffectId；新入口獨立推導 `card.play-cost` 內部需求碼，公開 registry 仍不開放。
准入限 base face、非正 delta 與至多一個非負 set 宣告，包含條件不成立的分支。

付款前在 Resolution 的同一狀態求值一次，單一 set 先於全部自身及承接 delta；checked 算術後
最後截零。Spell 只在經印刷成本核對的唯一固有 PP 位置支付，從者由卡片規則程序支付；
Activated、Fanfare 與進化不讀這個入口。完整付款、場容量與回復沿正式 play 程序。

純條件限 Bool 常值、And／Or／Not、公開 Field／Cemetery 的 Exists 或計數對常值比較；
查詢可按同一卡表的 kind、trait、name 過濾。這個入口使用隔離的封閉求值器，
不擴大一般 selector 的 predicate 子集。分段區間由DSL 明示，原樣保留；兩條獨立減費各計一次。
SC 可明示為墓地 Spell 計數；未展開的關鍵字及缺少封閉原區域讀值的 EX 限定自身條件仍拒絕。
付款交換、additional／optional／alt、X、動態值、正 delta、複數 set、aura、next-play 共享額度，
以及無明示期限的特定卡修正維持拒絕。完整合成、時點與來源義務見
[打出費用契約 §2–4](play-cost-semantics.md#2-決定實付-pp)。

Numeric 與 CostModifier 共用實際回合的 EffectEnd，減費在 7.4.8 清理，不提早於 end／Quick。
安裝與到期不產 stat_changed。測試採 CP04-060 的 Search→EX→當回合 -2，以及
BP18-059／SL14 的 Look→實際 Move→減費形狀；只驗片段與付款、到期、容量，不宣稱
這些卡的完整能力、lowering 或 shared usage 已可執行。

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

Damage 只接受 Ability flags、Instruction batch、Current source（正式整卡亦接受上述 SourceInformation）
及 Uniform signed amount。
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

## 本文固定 PP 回復

本文 `pp: {who: self, delta: N}` 接受正整數常值 N，降低為控制者目前 PP
的 Resource(Add)。實際回復量為 `min(N, 最大 PP − 目前 PP)`，最大 PP 不變；
已滿時仍產生零實際回復的完成收據，不視為付款，也不產生回復事件。

`InstructionOutcome::Resource` 為 authority-only，保存玩家、資源、請求量、
前後值與實際增加量；沿既有公開回復事件，不新增玩家協定。本文不接受 Pay、
Set、RecoverToMax、其他資源、其他玩家或動態數量；打出費用仍走原付款路徑。

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

一般 Move 只准入 Put、Owners(Cemetery / Banish)，來源限公開 Field／Ex／Cemetery／Banish；
Owners(Hand) 僅限公開 Field 來源，見[第二批觸發契約](trigger-second-semantics.md#5-自身離場)。
不接受 Shared(Field / Ex)、私人目的區、position、order、initial_posture 或 entry_effects；
唯一例外是〈私有看牌與同區排列〉所述，從自己牌庫放入自己 EX 的受限 Look→Move 流程。
Destroy 是獨立 action，僅接受仍在 Field 的目前世代；過期 capture 不移動新世代。
目前世代位於其他區域時整次執行失敗並回復，不外推場外 Destroy 語義。

同一指示先固定並去重物件，再共用 Game 的 `move_objects` 原語提交；目的區逐物件取其 owner，
不沿用來源控制者。整卡能力可承載 follower Advance／token 的離場：先完成原本的 owner 目的地
移動，再依 CR9.2.2 讓 Advance 立即回 owner evolve deck 表面，或依 CR9.1.4.4 立即消去 token。
原指示回執保留該次移動，不把立即規則當成原指示的直接目的地。片段 execution、其他特殊卡型、
連結／進化關聯仍拒絕；世代耗盡同樣拒絕，不能飽和後重用世代。

立即規則已移除或替換端點時，`DestroyOutcome.successful.after` 為 None；`MoveResult.after`
仍保存首次移區的世代。後續 follow 不自動跨越 Advance 的規則移區；token 消去使舊引用失效，
不能藉舊 receipt 對已消去的 token 執行後續動作。

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

## 牌組頂固定張數送墓

本文 `move: {top: {who: self, n: N}, to: cemetery}` 接受正整數常值 N，
降低為自己 Deck 的 Top／Forward Ordered selector 與 Put 至 Owners(Cemetery)。
結算時直接取當前牌組頂端至多 N 張，沿既有移動原語產生 MoveOutcome 與 MovementFact；
不先 Look、不詢問、不抽牌，牌組不足或為空仍盡可能執行，不造成抽牌失敗。
移入墓場的卡成為公開資訊，剩餘牌序與未移動牌的秘密性維持。

此權限僅屬於已驗證的送墓 selector；其他動作、指示 metadata、任意秘密區域
selector、牌底、非固定張數與其他目的地不因此准入。作者入口不接受 result、
placement 或額外進場效果，精確零張亦未開放。

## 自己選定手牌棄牌

本文 `discard: {who: self, n: N}` 接受固定正整數 N；降低為自己目前 Hand、
無篩選、單組與精確印刷張數的 Discard(Chosen)。結算才列出當前手牌候選，
不足時降低最低量以盡可能執行；空域與唯一候選仍詢問。

沿用 actor 專屬的物件問句與 token-local handle，沒有新增 wire 欄位。
對手不取得手牌候選，Referee 仍可檢視權威；actor／token／張數／重複／外來 handle／目前
手牌世代與持有者均在提交前檢查，無效回答保留已完成的抽牌及付款。
通用物件 selector 不因此取得 Hand 讀取能力，既有無篩選張數讀值仍可用。

提交沿真正 discard 原語記錄棄牌計數、DiscardEvent、DiscardOutcome 與
MoveMethod::Discard 的移動事實；只有實際離手的卡成為公開資訊，未棄牌仍秘密。
棄牌前的從者特徵快照沿卡面讀取攻防；手牌數值修正未准入，一般場上數值讀取仍不接受 Hand。
本片不准入他方手牌、隨機、帶篩選、任意張數、零張、作者 result 或 after
投影；一般棄牌事件 consumer 與完成量接續由各自能力另行准入。

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
此 EX 滿的 remainder 行為採 2026-10-23 維護者的 `inferred` 專案解讀，官方未回答；
CR 4.8.3.2 僅規定放不進者不移動，依據與邊界見 [runtime §3.2](runtime-semantics.md#32-精確成功-before)。
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
成功投影與減費安裝仍拒絕。整卡本文可承載本節 Search；公開 capability 沒有擴充。

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
場滿時可選替代的裁定或逐卡准入。其 C0／lowering 的 Fanfare 片段另經整卡載入、打出、
Fanfare 待機、Search、可選入場到 CheckTiming 驗證；來源座標為合成且 source_proof=none，
不是正式來源或 qualified 證據。錯答完整保留待答；選擇後的提交失敗回復整次 execution。
RNG 回復以重新起動後成功洗牌與同種子控制局比對，不能只比較 Debug 字串。

CP04-060 僅驗排除同名的ドラゴンズネスト搜尋、公開、進 EX 與洗牌，未接 UB、進化程序、
成功 after 減費及其他能力；BP13-035 僅驗已支付追加費用後的三組各一張搜尋、進場與
收尾，不涵蓋付款、入場觸發或整卡准入。兩者印刷資料由公開輸入產生器提供。

Public Order 依該文件 §7 維持拒絕，Repeat 的跨輪 epoch 依 §8 等待執行驗收。

## 固定數值修正

Modify 准入有號 Numeric Add、非負常值 Set 與非空遞迴 Numeric Combined；期限只接受 Installation anchor
的 NoTemporalExpiry、EndResolution、CurrentTurnEnd、NextTurnEnd，或明示 SourceHolder／Field 的 SourceZoneExit；沒有 usage。
CurrentTurnEnd 另可帶 DuringNextTurn 附加區間，詳下節。對象限目前場上從者，
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

除明示 SourceZoneExit 外，來源離場不移除非 static 修正；目標換區後原世代修正失效，不追蹤新世代。
NoTemporalExpiry 沒有額外時間截止。合成 closed Program 的全部本文完成後，先清除目標已換世代的數值層，
再移除綁定此次 execution 的整批 EndResolution 修正，從原始基礎與其餘 Add／Set 完整重算、保留目前受傷量 D，
最後執行既有規則檢查；指示之間不清場，體力降到 0 的從者在同一解算內仍是後續指示的對象。
到期不增加 stat_changed；最後一層消失後不保留過期基礎供後續重用。
閒置 Game 的基礎、固定操作與 ProperPlay 時間可存檔；待答與尚未解算的預備 play 不可存檔。
數值修正的 EndResolution 時序已定於[數值期限契約](numeric-timing.md#33-endresolution)，
目前只驗合成 closed Program 的生命週期；沒有真實 Numeric EndResolution 卡例，不由 lowering 自動補入期限。
待答不是解算完成，resume 的提交失敗連同到期清理一起回復整次 execution；解決區清理另沿上述 Printed Spell 程序；nested play 與其他完整 CardProgram 形狀仍未接入。
CurrentTurnEnd 保存安裝當回合的身分，在 7.4.8 整批結束主區間；沒有未來附加區間的效果才移除，end 觸發、Quick 與棄牌檢查期間仍有效。
NextTurnEnd 在安裝時捕捉玩家，等唯一回合排程實際開始該玩家下一回合時綁定其身分，
再於該回合 7.4.8 到期。skip 不配置身分，extra 每個實際開始的回合各有身分；
不以 elapsed_turns、預定回合或固定加一／加二換算。到期清理已經過去的回合不接受追認安裝。
到期造成 hp≤0 時，先執行下一實際回合的重置、PP、站立與抽牌，在 7.2.5 處理規則破壞，
再進入主要階段並收集 main_start 觸發。到期、重算不發 stat_changed。

原型 decide 可在數值層存在時繼續執行；傷害透過同一有效值重算並保留目前 D，
原型純 Add 以 checked 基礎差額重算。普通／超進化接受固定對象、已捕捉操作數的純 Add，
期限限 NoTemporalExpiry、CurrentTurnEnd、NextTurnEnd，無附加 intervals。只替換基礎，
保留 EffectId、ProperPlay timestamp、原期限錨與目前傷害，不重複加上修正差額；超進化 +1/+1 獨立保留。
到期從進化後基礎與其餘修正重算，不另發 stat_changed；限期到期還原是目標與進化契約 §3 的
`inferred` 專案解讀，沒有宣稱官方裁定。尚可能適用的 Set、其他期限／區間與基礎替換仍在進化付款前拒絕；效果本文進化仍只接受無期限 Add。
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
NextTurnEdge、phase edge、DuringNextPhase、SetKind 與下節以外的 Continuous 仍拒絕。
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

Continuous 的 Live 條件與操作數、一般動態依存、下節以外的 interval、SetKind、
Grant／RemoveAbility、Modify usage、負 Set 與非常值 Set，包括未執行分支，仍整棵拒絕。
Set 層有效時的進化／基礎替換由數值寫入邊界拒絕，未宣稱差額換算完成。
CP02-007 與 BP01-011 只驗已選場上從者的 +1/+1 片段；不包含成本、選牌、EX、
觸發或整卡准入。BP16-049 只驗受傷對象的 Set 1／1，未承載 SpellChain capture、
RemoveAbility 或整能力；上述未承載的整卡部分仍待後片驗收。

### 常值 Continuous 與非連續回合區間

本文 Continuous 限 InstructionStart 的固定 affected、Numeric 層、常值 Add／純 Add Combined、
ProperPlay(Current)、空 active_zones；條件只接受省略或常值 Bool。false 仍保存同一效果實例，
不因當前不適用而丟棄。操作數與主體固定，這些效果不會改變彼此的適用對象或操作，
因此沒有動態依存；不把此子集外推為 Live 條件的通用求值器。
安裝、到期、區間切換與重算不發 stat_changed，也不設定明示加攻體指示的增值旗標。

SourceZoneExit 在安裝時固定來源的 Field 世代；離開後經共同移區清理重算有效值，
保留目標目前受傷量，不因相同實體的新世代重入而復活。這只承載明示來源在場期限，
不提供來源離場後的 Current／LKI 讀值。

CurrentTurnEnd 的主區間與 DuringNextTurn 附加區間採聯集。主區間在 7.4.8 結束後保留
尚未發生的指定玩家下一實際回合；中間其他玩家的額外回合不適用。下一實際 begin 才固定
該 occurrence 並重新適用，到其 7.4.8 結束；不使用 turn.phase、elapsed_turns 或預定序號冒充。
目標換世代仍清除整個舊實例，後面的附加區間不能讓舊目標復活。
本子集不接受 interval-only、EndResolution＋非空 interval 或 phase 區間；缺少 phase occurrence
時保持拒絕。Set 及帶附加區間 Add 的進化限制仍維持；純固定限期 Add 僅開放上節所列子集。
