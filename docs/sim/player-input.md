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
准入與可抽取的開局牌數。載入失敗回傳 `Result::Err`，尚未交付對局；局內回答的錯誤
使用 `SubmissionKind`。同版本、同牌組、同先攻選擇席與私密 seed 可重現同一開局。
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

`Question` 的用途與答案形狀分開。現有 typed 決定沿用 `InputDomain` 與 `Answer` 的
當題候選代號，`QuestionToken` 只代表輸入 occurrence，不暴露 execution 或 instruction
座標。題目不包含 authority receipts、facts 或 continuation。
玩家輸入序號在已驗答後保留單調遞增，即使整次打出回滾重現了先前的 pending 題，
也不會恢復舊玩家 token。
權威容量候選原有的實體物件字串在玩家邊界省略；當題候選仍以不同選項代號回答，
不含盤面物件代號。既有權威題目不變。候選與觀察的連結需求見
[#1973](https://github.com/gbaian10/sve-kit/issues/1973)及
[#1977](https://github.com/gbaian10/sve-kit/issues/1977)。

`QuestionShape::ActionWindow` 標示主要階段的權威行動邊界，不列一般合法行動，
也不接受一般決定的答案。其他未受支援的程序及仍有舊原型續體的權威狀態是
`EngineFault`，不會冒充行動視窗。一般主要階段行動介面仍有限，不保證可從開局
打完整局；完整行動介面的需求見 [#1975](https://github.com/gbaian10/sve-kit/issues/1975)。

## 回合收尾與終局

`Command::EndPhase` 需要當席主要階段的 token，仍重驗既有攻擊義務、權威是否閒置與
資源完整性，再啟動結束階段。階段誘發先完成，才出回合末守護與 Quick 題。
`QuestionShape::Closing` 與 `Command::Closing` 對應兩種規則程序：

- `GuardAct`：以當席代號選擇任意子集的未橫置守護從者，也可不選。這是回合末規則
  選擇；入場 Ward 仍使用不同的 `InputDomain::WardEntry` 與其原答案，兩者不互通。
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
