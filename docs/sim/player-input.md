# typed 玩家輸入與提交

玩家介面位於 `sve_engine::game::player`。宿主先認證玩家，再傳入只含 P1／P2 的
`Viewer`；裁判使用權威介面。既有權威操作建立局面後，以
`Game::advance_player_input()` 完成檢查時點、pending 與 Quick 的出題，之後透過
`Game::input(viewer)` 查詢、`Game::submit_player(viewer, command)` 回答或投降。
提交本身會續行，不需在每次回答之間手動呼叫檢查或出題。

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

`QuestionShape::ActionWindow` 僅標示主要階段或回合收尾的權威行動邊界，不列合法行動，
也不接受一般決定的答案。其他未受支援的程序及仍有舊原型續體的權威狀態是
`EngineFault`，不會冒充行動視窗。本介面以既有權威入口作測試起點，不保證可從開局
打完整局；專用行動介面的需求見 [#1974](https://github.com/gbaian10/sve-kit/issues/1974)、
[#1975](https://github.com/gbaian10/sve-kit/issues/1975)及
[#1976](https://github.com/gbaian10/sve-kit/issues/1976)。

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
