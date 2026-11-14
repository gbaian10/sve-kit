# typed 玩家輸入與提交

玩家介面位於 `sve_engine::game::player`。宿主先認證玩家，再傳入只含 P1／P2 的
`Viewer`；裁判使用權威介面。既有權威操作建立局面後，以
`Game::advance_player_input()` 完成檢查時點、pending 與 Quick 的出題，之後透過
`Game::input(viewer)` 查詢、`Game::submit_player(viewer, command)` 回答或投降。
提交本身會續行，不需在每次回答之間手動呼叫檢查或出題。

## 建立與開局

`Game::start_player_game(selected, decks, first_chooser, seed)` 是可信宿主的建立入口。
`PlayerDeck` 只帶構築依據與主牌組／進化牌組的快照卡號，沒有卡面資料、實體 ID、
匿名填充物或任意局面。宿主先依[牌組構築契約](deck-construction.md)驗證牌組；
引擎不另做一份構築規則，而會重驗 `SelectedEffects` 的來源、每張牌的完整 typed 選用、
准入與可抽取的開局牌數。strict 要求雙方主牌組、進化牌組、規則產生的初始 EX 資源及
所有 token 依存完整准入；只有原型程式的卡片不會進局，也不以空 IR 補足缺少的效果。
宿主可先用 `Game::check_player_decks(&selected, &decks)` 檢查；開局入口仍會自行重驗。
缺少准入時回傳 `StrictStartError::Admission(report)`，宿主只把
`report.gaps(viewer)` 的卡號與封閉原因交付已認證的該席，不能把對手缺口或整份報告傳給玩家。
依存已說明的缺口不重複列為產生者不支援。來源不符或權威資料錯誤則回傳
`StrictStartError::Initialization`，原始診斷只留在權威端。
失敗時尚未交付對局；局內回答的錯誤使用 `SubmissionKind`。
同版本、同牌組、同先攻選擇席與私密 seed 可重現同一開局。
宿主把驗證後的永久卡片 ID 與張數對應至同一快照的實際版次卡號，逐張展開；
構築輸入未指定職業時，使用已驗主戰者的印刷職業值。現行 typed 對局以玩家座位及
資源表示主戰者，沒有主戰者卡片物件；其卡片資格由宿主驗證。先攻選擇席沿用既有
宿主輸入，可來自猜拳等局外程序，不從洗牌 seed 另抽一次。

`QuestionShape::Opening` 依序提供起始護符、先後攻與換牌題。起始護符按 P1、P2
逐席處理；指定席選擇先攻後，先攻再後攻決定換牌。`Command::Opening` 需要當席
token，起始護符與整手換牌排序使用當席 `ObjectHandle`，提交時再核對位置知識與
完整候選集合。換牌只能保留整手，或把整手依指定順序放回牌庫底後重抽，不能部分換牌。
另一席只取得等待摘要。起始護符在雙方換牌完成後公開，初始 PP／EP／SEP、生命與
主要階段開始沿用既有規則。接受回答後的程序故障會回到本次回答前並停止出題。

## 查詢與題目

`input` 只讀既有狀態，不配置 token、不改 RNG、不執行規則或重發事件。回傳封閉的
`Input`：當席待答、等待他席、正常終局或 `EngineFault`。非應答席不取得 token 或
私有候選；骰子重擲仍提供公開骰值與剩餘次數的等待摘要。

`Question` 的用途與答案形狀分開。現有 typed 決定以玩家專用的 [`DecisionShape`](player-questions.md) 描述操作，沿用 `Answer` 的
當題候選代號，`QuestionToken` 只代表輸入 occurrence，不暴露 execution 或 instruction
座標。題目不包含 authority receipts、facts 或 continuation。
玩家輸入序號在已驗答後保留單調遞增，即使整次打出回滾重現了先前的 pending 題，
也不會恢復舊玩家 token。
玩家候選的本題代號用於回答；可選的 `ObjectHandle` 只連到本席目前仍能辨識的
Observation 物件，不含實體 ID 字串。連結保存凍結來源的世代與辨識期，退休後
不會重新綁到新代號。完整名稱、群組、角色與原費限制見[玩家決定題](player-questions.md)。

`QuestionShape::ActionWindow` 提供主要階段的[合法行動意圖](main-actions.md)與參數範圍，
以 `Command::MainAction` 選定；不接受一般決定的答案。其他未受支援的程序及仍有舊原型續體的權威狀態是
`EngineFault`，不會冒充行動視窗。一般主要階段行動介面仍有限，不保證可從開局
打完整局；完整行動介面的需求見 [#1975](https://github.com/gbaian10/sve-kit/issues/1975)。

## 回合收尾與終局

主要階段的 `EndPhase` 意圖需要當席 token，仍重驗既有攻擊義務、權威是否閒置與
資源完整性，再啟動結束階段。階段誘發先完成，才出回合末守護與 Quick 題。
`QuestionShape::Closing` 與 `Command::Closing` 對應兩種規則程序：

- `GuardAct`：以當席代號選擇任意子集的未橫置守護從者，也可不選。這是回合末規則
  選擇；入場 Ward 仍使用不同的 `DecisionShape::WardEntry` 與其原答案，兩者不互通。
- `Discard`：題目提供當席手牌與超出上限的確切張數，答案必須是符合數量的不重複
  當前代號。棄牌觸發的能力先完成，再重新檢查手牌上限；效果回收手牌造成再次超額
  時，會再出一題，不會提前換回合或重放已完成的能力。

以上兩題均屬 `QuestionPurpose::RuleChoice`，具體規則用途由各自題型標示；非應答席
只有等待摘要。驗答不枚舉所有合法子集合，仍重驗所在區域、守護資格及確切數量。
最後一個 Quick Pass 與棄牌程序後會沿原管線完成期限到期、換回合、刷新、抽牌、
檢查時點及主要階段開始誘發，然後才提供下一個主要行動視窗。

正常敗北、同時敗北與效果終局沿用既有清理程序，不再出續行題；等待中投降保留
已執行前綴並清除所有續體。新入口沒有額外製造 resolved 結果來推動終局。

## 費用觀察

`Game::observe(viewer)` 的 `costs.cards` 只列觀看者自己可辨識、已載入 typed 效果的
手牌與 EX 卡，使用既有 `ObjectHandle`。`CurrentContext` 是目前位置與公開條件下的
報價；打出移入結算區或宣告後，權威付款會重新定價，不能把它當作保證價格。
`costs.current_play` 只提供該次 play 控制者的 `BeforePayment` 摘要，沿用原付款
讀取器、已凍結的交換報價與資源入場的 PP／EP 算式。Meal、Ride、Advance 等
keyword 入場的費用只在 `current_play` 呈現；手牌／EX 報價代表卡片費用。

摘要區分原費、模式、追加與可選費用，列出固定 PP、X 的份數及已回答值，並另列
橫置、生命、計數物、移動、捨棄、除外、公開、融合與入場資源等非 PP 成分。
X、可選宣告、模式、交換路徑、材料或入場資源仍未決時，`pending` 指明種類，
`total_pp` 留空；已知部分仍可讀。可選費用拒絕後不計入總值。這是費用摘要，
付款題的候選與回答仍由玩家輸入提供。

報價不付款、不移區、不抽隨機數，也不配置權威物件或觀看者代號。出牌前所需的
execution 上下文只在一次觀察共用的拋棄式副本建立。單張卡在該上下文無法報價時
以 `Unavailable` 表示，不輸出原始錯誤，也不阻擋其他觀察內容。報價不代表可合法
打出或負擔得起；實付與動態重驗仍由既有打出管線決定。

## 提交結果與續行

| 結果 | 語義 |
| --- | --- |
| `Applied` | 回答已接受，完成既有程序並停在下一題或終局；分配程序只回到目標題時也屬此類。 |
| `Rejected` | 錯席、過期 token 或未通過驗答的 payload；原題、狀態與 RNG 保留。 |
| `Reverted` | 已驗答後依規則判為非法打出，沿既有程序回整次 play 起點，原題失效。 |
| `Cancelled` | 依既有規則取消一個 pending occurrence，包括必要費用付不起；之後繼續檢查剩餘待處理。 |
| `EngineFault` | 執行能力缺口或內部狀態錯誤，保留原有回滾範圍與 epoch 高水位，停止玩家續行，不改判規則敗北。 |

驗答完成的時點由既有驗答／付款程序提供，不靠錯誤訊息文字猜測。驗答後的
`Unsupported` 或狀態錯誤即使已回滾，仍是引擎故障，不一律當成非法打出或取消。
安全的玩家結果不帶錯誤詳情；原始錯誤留在權威端。故障後查詢維持 `EngineFault`，
不自動重試。可信恢復或重建對局不在本介面範圍；明確的投降命令仍可正常結束對局。

`Command::Concede` 不綁 token，等待中的任一已認證席都可提出，沿用既有投降程序，
保留已執行前綴並清除續體。已終局後再次投降是拒絕。

正式打出順序及回滾範圍依[打出與付款契約](../dsl/play-semantics.md)；題目省略只依
[IR §2 單一判定順序](../dsl/ir-vocabulary-1.0.md#單一判定順序)及
[Targets 等價省略例外](../dsl/engine-boundaries.md#條件效果的選擇省略)。
Quick 在 pending 與規則檢查完成後出題，即使沒有合法行動也保留明確 Pass。

本介面沒有新增 wire 或權威恢復格式；目前不持久化玩家故障狀態，typed 對局原有的
存檔限制仍適用。

## 已提交事件

`Game::events(viewer, after)` 只讀取該觀看者已提交的[公開事件序列](player-events.md)。
懸置執行的事件延後到權威提交邊界發布；中途狀態仍從 `observe` 讀取。
事件投影缺口與 A01 的權威 `EngineFault` 分開。

## 公開效果、待處理與本回合歷史

`CardObservation.physical` 保留實體的卡片身分與面序號，`properties` 則保留目前特徵的
來源；進化從者的兩者可以不同。`face_up` 表示區域中的朝向，封閉的手牌／牌庫即使
本席可看牌面，也不因此變成表向。`public` 只依公開區域與仍有效的公開揭露判定，
不讀另一席的私有知識來改變本席摘要。開局、主要行動與收尾的卡片共用此讀取器；
必要的卡面或朝向資料損壞時停止觀察，不以預設面代替。

可辨識卡片的 `CardState.effects` 描述已安裝的數值、費用、授予、抑制、Pilot、
起身／攻擊／啟動限制與防止效果；玩家的 `effects` 描述主戰者防止、玩家費用、
除外區打出權限及 delayed 訂閱。摘要保留來源文字座標、期限、區間、剩餘次數及
目前是否貢獻；防止範圍只列持續獲授權的物件連結，缺少連結不代表效果已失效。
有效特徵與實際規則判定仍沿用原權威讀取器。靜態限制不另外建立
安裝紀錄，仍由有效卡面與原權威判定；可見安裝成分不等同於合法行動清單。

來源文字的可見性與目前物件連結分開判定。安裝與 pending 建立時記錄來源的
觀看者辨識期；失去辨識權後，已知來源文字可保留，但不藉稍後的重新揭露恢復
舊來源的連結。封閉摘要不包含 program、私有 binding、effect／instance／execution
編號或權威回條。

`pending` 只列已授權的誘發與尚在額度准入中的候選，不提供答題代號。控制者可見
自己的項目及准入群組尚待選取的數量；另一席只見來源已公開的項目，沒有私有
總數或群組餘額。delayed 訂閱只列公開事件種類與期限，捕捉值及本文留在權威端。
`CardState.quotas` 提供有效能力的每回合上限、剩餘次數及消耗時點；尚未使用能力
也能讀到上限，查詢不配置權威 instance，不消耗額度。單卡無法安全取得額度時，
`quotas` 留空值表示未知；已知但沒有限制則是空列表，不阻擋其他觀察。

`history` 只投影權威已提交並為本回合查詢保留的公開主體、傷害連結、正向能力值
變更、最終骰值及打出／UB 次數。它不包含未提交的 Combo 預估，也不是逐事件重播。
目前物件連結必須同時符合建立事實時的辨識期、原世代、仍在公開區及觀看者仍可辨識；
即使回滾還原舊世代，也不把已公開失知的歷史接到新代號。離場再回場不會
把舊紀錄連到新世代。本回合歷史沿用原回滾與回合清除時點。`remembered` 則保留
去重的卡片身分、實際看過的面序號及擁有者，不提供私有副本數量或逐實體連結。

正常進化的 `Evolve` 意圖開始後，`QuestionShape::Evolution` 依序提供進化牌面、材料、
棄手牌及付款題；宿主以 `Command::Evolution` 回當題能力代號或顯示域內的答案。
用途為 `PlayParameter`；非應答者只看等待摘要，完整規則見[主要意圖契約](main-actions.md#正常進化宣告)。

## 基礎策略 host

`host::PolicyHost` 保存兩席各自的 `BasicPolicy`，`run(&mut Game)` 只透過
`input`、`observe`、`submit_player` 推進已初始化的 typed 對局。
`Game::start_player_game` 成功回傳的 Game 可直接交給 host；受信任局面由原有啟動入口準備，host 不處理
權威載入／恢復。策略仍只取得本席 Observation 與 QuestionShape；題目 token 僅供 host
綁定 Command 與辨識重答同一題，不參與策略選擇。

外部傳輸可用 `prepare` 取得一筆 Command，依題目 actor 提交後，將該筆真實 Submission
交給 `record_submission`。一次只有一筆 pending；未配對就再次 prepare，或沒有 pending
就回報，停止為 ProtocolError。查詢／準備不計提交次數，也不使用策略的行動預算。

HostLimits 預設至多 10,000 次提交，同一題允許 1 次額外重試。
Rejected 計入總次數，但不使用策略預算；讀取最新題目後可重答，超過重試上限停止為
RejectedLimit。成功、回退、取消或題目改變會重設同題計數。
總次數上限在 prepare 強制，耗盡停止為 StepLimit。

HostReport 保留各 SubmissionKind 計數及兩席接受次數；停止狀態不再改變。
只有 Finished 表示原生正常終局，winner 為空表示平手。
EngineFault、PolicyFailure、RejectedLimit、StepLimit、ProtocolError 均不假造勝者；
Reverted、Cancelled 分開記錄並可繼續，不是正常終局。
原生 EngineFault 優先於其他結果；已回報的 Finished 優先於提交次數耗盡，最後一筆額度
上結束的對局仍是 Finished。故障細節保留於權威端，host 不公開診斷訊息。
