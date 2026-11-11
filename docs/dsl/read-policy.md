# 讀值與結構走訪政策

本頁約束共用讀值與 IR 走訪的責任，配合[驗證責任表](validation-responsibilities.md)使用。
結構走訪只提供節點、來源、IR 位置及進出巢狀的通知，不建立可執行許可，亦不改變現有接受集合。
執行計畫的封閉建構與 consumer 接線另須經 backend 准入；本頁不宣稱所有讀值組合已可執行。

## 讀值的政策軸

| 軸 | 必須保留的區分 | 檢查責任 |
| --- | --- | --- |
| 來源 | 原費／印刷特徵與有效費用／有效攻體；source 自身與一般候選不是同一權限 | consumer 指定屬性與 adapter，不能因共用求值而改讀另一種值 |
| 物件視圖 | 目前物件、指示開始快照、事件 frozen、receipt Before／After、離場 LKI | 原有 wellformed 與 adapter 驗證所需快照；缺少快照不能查現在物件補齊 |
| 成員語義 | 選中、發現、揭露、付款成功、實際移動成功與 follow 尚在目的地 | producer／receipt 類型及使用點動態重驗；選中不證明成功、found 不證明 moved |
| 資訊權限 | 公開區數量不等於該區成員身分；授權私有候選不等於任意私有查詢 | 原有 selection、observation、reveal 與來源名稱授權；kernel 不取得任意 Game 查詢權限 |
| scope | 同一序列的先產後用、各分支區域、迴圈區域及獨立 execution | wellformed／binding 分析保留 dominance 與資料流；不能把走訪見過的 producer 全部加入全域白名單 |
| 時點 | AtPlay、AtSelection、AtResolution、AtEvent、指示開始、已提交歷史 | capture／consumer 指定時點；同一欄位名稱不能省略時點差異 |
| 數域與缺值 | 有號請求、非負數量、未知原費、物件離場、未付或未揭露 | 保留 checked 計算及各 adapter 的缺值政策；不得一律補零 |

目前的零貢獻與拒絕均有規則含義。例如選中成員已離場的加總可依既有契約貢獻零，
follow 成員已不在目的地則須拒絕；不能以一個共用的 missing → 0 規則取代。
Search 可找不到、正式目標必選，以及場內外 keyword 的讀取差異亦由 consumer 保留。

## 結構走訪的介面

`ir::walk` 沿 IR 定義的欄位順序深度優先走訪，提供具型別的節點參照與相對 IR 路徑。
每次 enter 都有配對的 leave；`Located` 的來源也有配對通知，子來源離開後恢復外層來源。
Program／Ability 攜帶 scope，Control 攜帶各分支與 Repeat 的作用域資訊，
Action 的 Delay 及 Grant 中的獨立 Ability 會分別進出，不能默認繼承目前 execution 的讀權限。

| 結構 | 必須可見的內容 | 不由 walker 判定的政策 |
| --- | --- | --- |
| Sequence、If、May | 序列與所有分支，包含條件為假的分支 | 分支輸出能否合流、May 是否實際執行 |
| Match、Mode | 所有 body、模式成本與參數 | 選中哪個分支或模式、成本是否付清 |
| Pay | costs 及 on_paid，保留各自位置 | 只有成功付款的路徑能讀其結果；走訪先後不構成付款證明 |
| Repeat | 終止條件、Program 或 BodyReference、scope 及 exports | 跨迴圈讀取、輸出匯出與終止性 |
| EachPlayer、EachObject、Simultaneous | 各 stage/body 或 batch 中的所有指示 | 執行順序、跨席權限、共同輸入視圖與 sibling producer 隔離 |
| Delay、Grant、InstallEffect | 巢狀 Program、獨立 Ability、declaration 與 capture | 新 execution 的捕捉授權、provider 及持續效果的視圖 |

observer 可明示略過子樹以保留既有盤點範圍；略過只影響該次盤點，不是驗證成功。
能力需求盤點不略過任何分支，且保持原有 capability code、位置與來源集合。
operand 盤點若只處理既有直接事件主體，仍須保留自己原先允許的 Control／動作範圍，
不能因 walker 能看到 Pay 或獨立 Ability 就同時放寬授權。

目前使用共用 walker 的盤點範圍如下；集合中的元素是精確 IR 位置，不是可跨分支使用的 binding 授權。

| 使用者 | 保留的遍歷政策 |
| --- | --- |
| `Requirements::derive`／`derive_program` | 全部結構與分支，來源隨 Located 進出；既有 code 與位置集合不變 |
| `event_operand::paths` | 只進 Sequence／May／If／Mode 本文，保留 Modify／Grant／Token／Move／InstallEffect 原有直接主體判定；略過成本及所有指示的巢狀定義／延遲本文 |
| `event_operand::target_paths` | 只進 Sequence／May／If，保留 If 條件與 Modify／Grant／Move／Destroy 原有 target 判定；不因共用 walker 新增 Mode 或 Pay |

BodyReference 仍是參照節點；結構走訪不追蹤並展開它指向的定義，也不嘗試執行迴圈。

## 與驗證及執行的分工

walk 的 enter／leave 是語法結構通知，不是 dominance、執行時順序或授權證明。
各驗證器在各分支建立自己的環境，只有原規則明示允許的輸出才向外發布。
raw IR 仍由 wellformed 檢查，作者路徑仍保留來源閉包與 lowering；
未付、跨迴圈／execution、錯誤 producer 或未授權私有讀取的拒絕不因共用走訪而消失。
靜態計畫建立後，使用時仍重驗付款、世代、位置、follow 與實際 reveal。

## 准入結果與 token 數量

`WellFormedProgram`／`WellFormedCardProgram` 證明 IR 的型別、scope 與 producer 關係，
不是 backend 執行許可。raw 程式與全卡入口通過既有 backend 檢查後，才建立私有建構的
`AdmittedProgram`／`AdmittedCard`；它們持有不可變的受驗 IR 與一起編譯的計畫。
計畫沒有序列化、版本欄或額外 hash，也不能從候選卡片取得。
selection 共用的計畫只依賴 catalog 與受驗定義，不保存局面或遊戲快照。

printed 能力、授予定義與 Delay 本文分別持有自己的計畫。授予時保留提供者與原指示，
Delay 安裝時保留該 Delay 的本文計畫；pending 與巢狀執行隨定義傳遞計畫，
不以合成 callback 的 AbilityId 搜尋 printed 能力。runtime 合成的 Drive／Drain
能力仍由既有規則工廠建立，經明示的內部編譯入口附上計畫。

一般 Token、EachPlayer token stage 及 Delay 本文的 Token 在准入時選定數量讀者：
固定值、既有公開區域計數，或 destroy-success 的 Before controller 計數。
執行時依自己的計畫查找 InstructionId，且完整指示必須一致；缺少或不一致即拒絕，
不退回合法形狀推測。destroy 計數仍逐項驗證成功 receipt、departure fact 與離場前控制者。
付款成功、物件世代、位置及真實 reveal 仍在使用時重驗，計畫不能代替它們。

mixed EX 的固定一張 Simultaneous 家族仍使用獨立的既有准入及 projection。
Transform 也保留自己的准入，按各控制者實際成功回條算出張數後，才交共用配置核心。
配置核心只收已求值的模板與數量，保留整卡准入、種類、容量、同名上限與選擇流程；
它不接受可任意求值的數量 Expr。同名建立的 follower 種類限制仍由 Token consumer 保留。
這些界線不授予其他 token 算式或完整卡片的執行資格。
