# 內部 typed IR 程式體良構邊界

`sim/engine/src/ir/wellformed.rs` 的 `WellFormedProgram::check` 接收並持有一個
`Program`，成功後只能取得不可變參照。呼叫者無法直接構造已驗型別，也不能修改
其中的程式體。它驗證的是封閉程式體，並把輸入的 root 視為一次執行的完整本體；
沒有接受外部 binding／receipt 的入口。
`SourceHolder`、控制者與持有者仍是執行環境提供的符號。

此邊界不代表整個 `CardProgram`、能力宣告、capability 或可執行性通過。
`Registry::check_capabilities` 仍只檢查需求，不能產生此型別；已驗程式體也不能
跳過 capability 與執行環境的准入。這些 Rust 型別沒有公開 wire 格式。

## 作用域與結果

- 每個程式體的 `ScopeId`、整棵樹的 `InstructionId` 與 `ReceiptId` 不可重複。
  binding 以 `(ScopeId, id)` 識別，同一 slot 不可改用另一個型別重新宣告。
- 宣告先檢查輸入，再公開輸出。使用處須由宣告支配，並吻合型別及一般值、capture、
  本輪 receipt 三種用途。未執行分支也檢查，不依常數條件略過。
- sequence、if 的子程式不隱含匯出 binding 或 receipt。分支間不能互讀；沒有隱含
  branch join。selector／predicate candidate 只在自己的 predicate 內可見。
- each-object 的元素只在其 body 內可見。each-player 的玩家在各 stage 可見；
  stage 的其他輸出不會流到迴圈外。逐玩家 stage 之間的輸出交接（例如 BP13-043
  先各自指定、再依序交付）尚未驗證，本片暫不讓輸出流入下一個 stage，目前回報
  `UnavailableBinding`；這是本片的限制，不是 K.each-player 的語意。
- simultaneous 所有成員共用 batch-start 環境，各成員不可讀 sibling 的結果。
  完成整批檢查後，才同時公開成員輸出及 batch receipt。空批次拒絕。
  無 choice stage 的批次與 NonChoiceSimultaneous player stage 不能藏入數值 declare，
  也不能藏入 draw／look 的 UpToStepwise（逐張決定）；
  each-object 的 Chosen 順序也尚未接受。
- repeat 的 export 必須指向自己 body 結束時可見的 local receipt，並符合 producer
  outcome 型別；不能重新匯出外層 receipt 或本輪 binding。iteration 只在
  AfterIteration 的終止條件可見；Count 與 BeforeIteration 不能使用它。
  collect 只在迴圈後可見，並保留 `ReceiptList<T>` 的元素型別。
- instruction precondition 的 `protects` 必須是該 instruction，述詞在其輸出與
  receipt 公開前檢查。一般 body 不接受 Cost instruction。

## 接受的形狀

接受 sequence、if、無 choice stage 的 simultaneous、直接 body 的 repeat、
each-object 與 each-player；instruction 接受 draw、look、counter、reveal、
flip-down、posture、初次擲骰與數值 declare。capture 只接受 Int、Nat、Bool、
Selector 的 `AtResolution`，且必須位於 root body 開頭連續的 capture 前綴：
AtResolution 在進入執行時一次凍結，之後的節點會看到本體自己造成的變更，
巢狀 sequence／if 也不是新的解算。前綴各 capture 同時凍結，輸入只能用入口
可見的值，不能引用同前綴的其他 capture。其他位置回報 `UnsupportedShape`，
scope 不是 root 回報 `WrongScope`。

值檢查涵蓋這些形狀使用的 signed arithmetic、明示 Nat 轉換、條件值、部分 Live
讀值、數量 receipt／collect 投影、集合及局部 candidate。支援的式子依各運算元
遞迴檢查，不以整個型別家族推定所有變體都通過。
有號數不會先截成零；Add、Subtract、Multiply、FloorDivide 要求兩個運算元，
HalfUp 要求一個，Minimum／Maximum 與作者 `min`／`max` 相同，至少一個。
FloorDivide 只接受可靜態確認的正整數常值除數，其他動態除數明確拒絕。

其餘形狀一律回報 `UnsupportedShape`，包含尚未驗證的 decision／target roles、
付款、移動／search／replacement、事件投影、effect 安裝及能力宣告。
拒絕形狀的程式不會得到已驗包裝。這個拒絕不是「語法無效」或「引擎已實作」的宣稱。
通用 `A.comb.instead` 沒有可構造的 typed IR 節點；作者輸入的拒絕診斷仍屬 lowering
邊界，不能靠塞入佔位節點繞過。

## 診斷

失敗回傳第一個 `WellFormedError`：`stage = well-formed`、具型別的 reason、
最近 `Located` 節點的來源座標與內部 IR path。來源座標由呼叫者提供，沿用
`SourceSpan` 的公開座標契約；`Display` 只輸出 stage、reason 與程式內路徑，
不輸出卡文、表達式值或來源欄位內容。
