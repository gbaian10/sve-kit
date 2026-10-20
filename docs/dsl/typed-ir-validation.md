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

- 每個程式體的 `ScopeId`、整棵樹的 `InstructionId`、`DecisionId` 與 `ReceiptId` 不可重複。
  binding 以 `(ScopeId, id)` 識別，同一 slot 不可改用另一個型別重新宣告。
- 宣告先檢查輸入，再公開輸出。使用處須由宣告支配，並吻合型別及一般值、capture、
  本輪 receipt 三種用途。未執行分支也檢查，不依常數條件略過。
- sequence、if 的子程式不隱含匯出 binding 或 receipt。分支間不能互讀；沒有隱含
  branch join。selector／predicate candidate 只在自己的 predicate 內可見。
- each-object 的元素只在其 body 內可見。each-player 的玩家在各 stage 可見；
  同一玩家的 stage 輸出會交給其後續 stage，包含選取 binding、骰值及 receipt，
  但不會流到迴圈外。每個 stage 保留自己的 scope 與 choice 限制；後續
  ChoiceProcedure 不會繼承前一 stage 的 NonChoiceSimultaneous 限制。
  BP13-043 與 BP06-110 的合成種子只驗證這項交接，不宣稱完整卡片可以執行。
- simultaneous 的 choice stages 先於成員提交；同 stage 的 decision 讀共同入口，
  後一 stage 可讀前一 stage 的公開不可變 Nat 回答。成員共用完成 choices 後的環境，不能讀 sibling
  的結果。choice bindings 只在批次內可見，成員輸出與 receipt 完成整批檢查後公開。
  空批次拒絕；批次 choice stage 的帶前提 decision 尚未接受。
  批次成員與 NonChoiceSimultaneous player stage 不能藏入數值 declare，
  也不能藏入 draw／look 的 UpToStepwise（逐張決定）；
  each-object 的 Chosen 順序也尚未接受。
- repeat 的 export 必須指向自己 body 結束時可見的 local receipt，並符合 producer
  outcome 型別；不能重新匯出外層 receipt 或本輪 binding。iteration 只在
  AfterIteration 的終止條件可見；Count 與 BeforeIteration 不能使用它。
  collect 只在迴圈後可見，並保留 `ReceiptList<T>` 的元素型別。
- instruction precondition 的 `protects` 必須是該 instruction，述詞在其輸出與
  receipt 公開前檢查。一般 body 不接受 Cost instruction。

## 接受的形狀

接受 sequence、if、simultaneous、直接 body 的 repeat、each-object、each-player
與下述結算付款；instruction 接受 draw、look、counter、reveal、flip-down、posture、
初次擲骰、數值 declare 與下述 discard。UpToStepwise 目前只檢查情境允許選擇，
逐張回答、停止時點與可見性尚未驗證；draw 抽前可停與 look 看後決定的差異仍須保留。
capture 只接受 Int、Nat、Bool、
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

其餘形狀一律拒絕，包含 target roles、分配傷害、模式、移動／search／replacement、
事件投影、effect 安裝及能力宣告。未承載的 decision／付款時點回報 `DecisionTiming`，
其餘未驗形狀回報 `UnsupportedShape`。
拒絕形狀的程式不會得到已驗包裝。這個拒絕不是「語法無效」或「引擎已實作」的宣稱。
通用 `A.comb.instead` 沒有可構造的 typed IR 節點；作者輸入的拒絕診斷仍屬 lowering
邊界，不能靠塞入佔位節點繞過。

## 結算選擇與付款

獨立 decision 只接受 `ResolutionChoice` 的 Required 物件／Nat 域。decision 與 domain
的 cardinality 必須一致；min=0 仍是選擇，不能省掉。MayDecline、compound、模式與
打出／規則程序入口尚未驗證，不能用一般 body 假冒那些時點。

帶 precondition 的 decision 必須緊接其受保護的 instruction，兩者保存相同前提，
`protects` 指向該 instruction；前提不能讀其尚未產生的回答。這是一個指示開始點，
不是提前詢問再於 instruction 重讀前提。回答只在該 instruction 可用，之後保留
receipt；不能越過其他節點、跨 stage 或外洩回答。BP13-043 獨立指定的選擇不應掛上
後續交付的前提。

discard 接受 Chosen、Random、All、DownTo。Chosen 群組必須 disjoint；群組、population、
eligible 透過目前手牌的 zone、filter、difference、union 或有手牌限制的 intersection
證明子集。無法靜態證明的 selector 拒絕；不靠虛構物件列舉驗證動態集合。
目前只接受 actor 就是手牌持有者。選擇域的公開 zone 白名單為 Field、Ex、Cemetery、
Resolution、Race、Drive、Trigger、Equipment；Hand 只容許持有者私有可見性，包含
述詞內巢狀集合讀取。Deck、Evolve、Banish 一律拒絕；Evolve 的私有牌組與 Banish
的裏向物件需另有可見性證據，不能把整區當公開。

選擇域拒絕未驗可見性的 bound／capture／receipt 集合來源；bound 物件也只接受本次
選擇域述詞內建立的 candidate，不能從外層 each-object 帶入未知可見性的物件。
Int／Nat／Bool 的 capture 與未知來源 bound 亦拒絕；唯有先前 Public Integer decision
產生的不可變 Nat 回答可供後續選擇使用。此限制包含 aggregate bounds、群組述詞和
Random discard 的 count；例如私有或 declare 來源的 X 張隨機捨棄目前會被拒。
這些限制不取代 executor 的 Observation 投影與動態合法性驗證。

`K.pay` 目前只接受 ResolutionChoice 的非空 Ordered costs。每個 step 必須是 Cost，
已驗成本形狀為 discard、移除 counter 與 act；其他成本及 alternatives 尚未接受。
Optional 的「付不付」與成本內 discard 的材料選取都歸這個 K.pay 程序所有，
materials 只建立同一次選取的結果 binding；這不是 K.may，也不經獨立 DecisionId
或額外的 `Node::Decision`。獨立 MayDecline 仍未接受。materials 與成本 receipt 僅對後續成本及 on_paid 可見。
PaymentResult 在成本檢查後才對 on_paid 與付款節點之後公開，不能從成本讀尚未完成的
付款結果。付款完成、物理實際量與選取結果仍是不同型別。

repeat 可以匯出 PaymentResult，終止式可讀本輪 complete，迴圈後可以讀 collected
payments 的 All／Any complete。相同本輪 receipt 可匯出到不同 iteration bindings；
每個 binding 仍獨立檢查 scope 與 producer 型別。這些都是結構檢查，完整付款、原子
消耗與重播中回答一次等執行語意仍須 executor 驗證。

## 診斷

失敗回傳第一個 `WellFormedError`：`stage = well-formed`、具型別的 reason、
最近 `Located` 節點的來源座標與內部 IR path。來源座標由呼叫者提供，沿用
`SourceSpan` 的公開座標契約；`Display` 只輸出 stage、reason 與程式內路徑，
不輸出卡文、表達式值或來源欄位內容。
