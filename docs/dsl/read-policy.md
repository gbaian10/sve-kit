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

## 候選條件核心

selection、Search 及已委派的費用對象判斷共用封閉的候選條件計畫。
compile 走完整個條件樹，驗證候選 binding、來源與每個原子；
不能因前面的條件為 false／true 而略過未授權分支。
核心只從指定候選的 reader 取得布林、數值或標籤匹配結果，不持有 Game 或任意物件查詢。
數值比較沿用共用核心；名稱比對保留「印刷名已命中就不必讀額外名稱」的規則。

下表描述既有來源政策，不能把其中一欄的權限套給另一欄。

| 條件／屬性 | Selection | 私有 Look Filter | Search 及使用其政策的 consumer |
| --- | --- | --- | --- |
| Not、And、Or | 保留 eager；空組合拒絕 | 同 Selection | 保留短路；空組合及以無條件 true 作子條件的歧義組合拒絕 |
| class 與 Not class | IR 要求 Effective，讀印刷 class | 同 Selection | Original／Effective 均讀印刷 class |
| 名稱 OR／排除／子字串 | 比對規則名稱；來源側只允許原有 SourceHolder 讀法 | 來源名稱須是受權 SourceInformation | 來源名稱須是受權 SourceInformation |
| kind、trait、token 交集 | Effective kind／trait 與 object token 狀態 | 同 Selection | kind 依 Original／Effective；trait 與 token 依印刷面 |
| cost／original cost 比較 | Eq／Le／Ge；分別讀有效費用與原印刷費用 | 同 Selection | IR 保留既有六種比較；原費與有效費用不混用 |
| hp／power 比較 | 有效數值；只適用 Field follower，其他候選整個原子為 false | 同 Selection | 只有 power；沿用既有 power 欄位，非 follower 為 0；hp 拒絕 |
| counter 比較 | 拒絕 | 拒絕 | 只讀同一候選的既有 counter，不跨卡相加 |
| keyword／trait／token 交集 | 經原有 keyword 授權；場內有效 keyword 含抑制，場外用印刷能力 | 同 Selection | 同一 keyword reader，保留 Evolution、Stack、EarthRite、Lastword 的既有區別 |
| 其他物件狀態 | FaceUp、Racing、EnteredThisTurn、Boxed、posture、evolution 依既有位置限制 | 同 Selection | 只允許 token 狀態 |

Comparison 的右值仍限原有 `public_numbers`，不是任意數值讀取。
公開素材另保留既有的「與持有者不同名」特例：只能是整個條件，或最外層 And 的最後一項。
該項必須是候選名稱與 SourceHolder 名稱相交的 Not，其餘部分仍按 Search 政策驗證。
准入與 runtime 使用同一個素材 compiler，列舉前準備一次持有者名稱；
此特例不適用於一般 Search、私有手牌素材或其他使用 Search 政策的 consumer。
原子計畫保留 Original／Effective 標記；reader 仍須依 consumer 決定實際來源與缺值政策。
凍結的有效資訊不能充當缺少的原資訊；缺少所需欄位或快照須拒絕，不能改查目前 Game。
四種 reader 的核心介面有合成對照測試：

| 資訊來源 | 核心對照與生產範圍 |
| --- | --- |
| 目前公開候選 | 使用 Selection adapter，外層保留公開區域、世代與位置檢查 |
| 授權私有候選 | 使用既有 Search／Look／手牌決定的政策；不取得任意私有查詢權 |
| 印刷資訊 | reader 明示提供印刷欄位，原費與有效費用仍是不同請求 |
| 凍結資訊 | 同一計畫讀已提供的保存值；缺快照、欄位或 Original 資訊時拒絕 |

一般 frozen receipt、event、history 的 producer、Before／After 與快照授權
仍由各自 adapter 接入；核心對照不新增這些 raw IR 來源。

準備操作數的時點亦保留在 consumer：

| consumer | 準備來源名稱／動態右值 |
| --- | --- |
| Search 本體與分組、材料 | 沿原有時點，在列舉前準備；私有來源名稱走 execution 的封閉授權 |
| Selection Zone、私有 Look Filter | 沿原有時點準備，保留各自的名稱政策 |
| 非私有 Selection Filter、費用對象、手牌付款、exchange、reveal-until | LiteralOnly；不新增準備，未準備操作數仍在實際求到該原子時拒絕 |

手牌選擇的外層決定、擁有者、區域及觀測授權保持原有檢查；不能只看 zone 是 hand，
就將其改成另一種 reader 政策。Search 可找不到與正式目標必選也仍由 consumer 決定，
不取決於共用布林核心。世代、位置、付款、成功回條與真實揭露仍在使用時重驗。

作者既有 ObjectFilter 簡寫繼續共用原有 lowering，不把一般算術、事件或回條查詢硬改成固定欄位。
支援表是 IR 與各 consumer 的既有範圍，不宣稱作者端所有簡寫都已串接：
Search 的作者 power 篩選仍只有上限比較，PlayingQuery 的原費也仍只有上限比較；
own-hand 同名排除的既有作者形狀限制亦保留。這些缺口不能靠共用 reader 自動放行。

## 持續效果的讀取環境

Grant、Prevention、CostModifier 的 live provider 共用 `card/provider.rs` 的只讀 execution：
來源、目前世代及控制者取自當下遊戲物件。出牌費用沿用付款 execution 的物件對應，
獨立投影及進化報價使用查詢內的對應表；兩者都不繼承付款者的 binding、receipt、
frozen／LKI 或來源資訊 producer，也不分配遊戲的 identity、execution 或安裝時點。
一般 selector／boolean／number consumer 保持原有准入及使用時的世代、位置與讀權限檢查。

此環境不預先展開來源的 keyword 或全部特徵；合法數值查詢仍走有效數值讀取，
讓 field 候選包含 provider 自身時可讀攻體而不重入 keyword 投影。
合法性、觀察與效果使用的既有 selector、有效數值及關鍵字 consumer 維持共用讀法，
不另建一份預覽快照或結果快取。

| Provider | 仍由原 consumer 保留的政策 |
| --- | --- |
| Grant | local 能力；Field、Equipment 或既有 Field＋EX 範圍；裝備授予仍受 host 抑制限制 |
| Prevention | effective 能力（含已授予定義）；Field／Equipment，保留原替代排序與來源標示 |
| CostModifier | printed 能力；Field，出牌對象依實際起始區判斷，進化仍只讀自身修正 |

live provider 每次重新列舉來源及能力，來源離場或抑制後停止貢獻。
已獨立安裝的固定成員授予、費用修正及防傷仍由自己的世代、期限與次數管理，
不因提供者離場而取消，也不把晚入場者加入原固定集合。

共用環境沒有放寬 consumer：static AddDamage 仍只接受 literal，
BP20-021 的融合 counter 動態傷害量尚未接線，不能由此推定 BP20-020 的相依閉包已完成。
installed DamageCap、手牌條件 Quick、其他新時機或新特徵層也不由本環境提供。

## 准入結果與 token 數量

`WellFormedProgram`／`WellFormedCardProgram` 證明 IR 的型別、scope 與 producer 關係，
不是 backend 執行許可。raw 程式與全卡入口通過既有 backend 檢查後，才建立私有建構的
`AdmittedProgram`／`AdmittedCard`；它們持有不可變的受驗 IR 與一起編譯的計畫。
計畫沒有序列化、版本欄或額外 hash，也不能從候選卡片取得。
selection 共用的計畫只依賴 catalog 與受驗定義，不保存局面或遊戲快照。
printed 能力共用同一份不可變受驗全卡 IR，計畫不再複製完整本文；
授予定義、合成 callback 與配對所需的原指示仍由各自計畫持有。

printed 能力、授予定義與 Delay 本文分別持有自己的計畫。授予時保留提供者與原指示，
Delay 安裝時保留該 Delay 的本文計畫；pending 與巢狀執行隨定義傳遞計畫，
不以合成 callback 的 AbilityId 搜尋 printed 能力。runtime 合成的 Drive／Drain
能力仍由既有規則工廠建立，經明示的內部編譯入口附上計畫。

既有 UB 事件的 `event:holder` 與 `it` 都投影同一個已提交 holder；
解算仍重驗該物件的世代與有效區，不從現在的來源或其他事件補值。
Equipment 的 holder 投影維持自己的事件角色；此別名不開放其他事件頭。

安裝式 Grant 可以授予既有 Field Played observer，沿用受驗事件頭、提供者、
接收者世代與安裝期限。多次安裝各保留自己的能力 instance；Play 提交時先捕捉
符合條件的觀察者，之後持有者離場不取消已排入的 pending，離場或到期後不再觀察新 Play。
持續效果授予 Played 的提供者仍限定 Equipment；其他授予事件頭與 UB 資格的准入不變。

一般 Token、EachPlayer token stage 及 Delay 本文的 Token 在准入時選定數量讀者：
固定值、既有公開區域計數，或 destroy-success 的 Before controller 計數。
固定 token 模板的數量可沿既有 Nat／Int／Bool 型別組合這些葉：
CheckedInt、FromNat、七種 Arithmetic、Conditional，以及 And／Or／Not、
CompareNat／CompareInt；作者的既有 `per` 降成正 literal 除數的 FloorDivide，
不另造 token 專用運算。每個 CheckedInt 都在載入時證明結果非負，
包括條件與兩個分支中的巢狀節點；無法證明即附來源位置拒絕。
中間 Int 可為負，最終用 Maximum 拉回非負範圍，或透過上下界證明的減法可准入；
區間端點的飽和只供編譯期證明，runtime 仍用共用 checked arithmetic。
確定的常數溢位與 Nat 轉 Int 越界在載入拒絕，獨立 Nat literal 保留 u64 範圍。
作者 JSON 仍遵守既有的安全整數界；u64 範圍指 raw IR 的 Nat literal。
And／Or 沿用 eager 求值，Conditional 只求選中的分支；編譯仍檢查所有分支。
EachPlayer 遞迴核對每個 Destroy-controller 葉的計數玩家與目的地玩家相同。
准入遮罩只涵蓋這些 Before 葉，外層運算與條件仍接受一般 capability 檢查。
PP X、Resource、Event、Dice、DistinctNames 與私有區域等新葉不在此試點內；
Name 模板仍只接受固定一張，成本 token、mixed EX 與 Transform 不隨之放寬。
公開 selector 仍須通過既有選取驗證與觀測 spine 限制，數量計畫不授予新的身份讀取。
執行時依自己的計畫查找 InstructionId，且完整指示必須一致；缺少或不一致即拒絕，
不退回合法形狀推測。destroy 計數仍逐項驗證成功 receipt、departure fact 與離場前控制者。
付款成功、物件世代、位置及真實 reveal 仍在使用時重驗，計畫不能代替它們。

mixed EX 的固定一張 Simultaneous 家族仍使用獨立的既有准入及 projection。
Transform 也保留自己的准入，按各控制者實際成功回條算出張數後，才交共用配置核心。
配置核心只收已求值的模板與數量，保留整卡准入、種類、容量、同名上限與選擇流程；
它不接受可任意求值的數量 Expr。同名建立的 follower 種類限制仍由 Token consumer 保留。
這些界線不授予未列出的 token 讀者或完整卡片的執行資格。

## 回條成員與屬性

`game/internal/receipt_members.rs` 集中 producer adapter 與回條成員讀取。
`MemberView` 只帶原成員參照，以及該來源已保存或可合法投影的部分特徵或原印刷身分；
它不建立 receipt 未保存的完整 After state。靜態的 producer／scope 證明仍由既有准入計畫
與 `receipt_counts::ReceiptReads` 檢查，讀取模組不另造可跨分支使用的白名單。

| 入口 | 保留的來源與資料政策 |
| --- | --- |
| ReceiptAny／ReceiptCount | Discard 與 latest Fusion 讀成功付款或棄牌保存的特徵；deck Move 另驗實際離開牌庫的 fact，公開 Look 必須已揭露；field→cemetery 的 Before 條件只讀保存印刷身分的原費 |
| distinct_original_costs | 只讀成功 deck→cemetery 的 Before，以不可變 base printing 讀原費；不以有效費用替換 |
| paid_material_acts | 已完成的 Cards 付款數量須與材料成員數一致；不把獨立的來源 Act 算入 |
| paid_material_number | 單張必付場上從者在付款前保存的有效費用／攻擊；只讀 Before，保留 Int 與資訊缺失，不重新讀離場後數值或以原費代替 |
| discarded_cost | 恰一張成功棄牌的凍結有效費用，仍要求已保存的非負值 |
| Destroy 動態成員計數 | 讀成功 Before 清單，逐項驗 departure fact、來源為 Field 與離場前控制者；不把選中或嘗試移動算成功 |
| 公開 Look 名稱種類數 | 先驗觀察者、整批揭露、原世代仍在己方牌庫，再依每張卡的一個規則名稱計數 |

底層 Move、Search、Discard、Destroy、Banish 五種 movement producer 仍各驗對應的
outcome，並依 after 世代改變及 actual destination 篩成功。Order 的 Before 讀取另取
successful 清單；同一牌庫重排不改世代，不能套用一般跨區移動的判定。
local／iteration、Before／After、已付分支與公開授權仍各沿原範圍，未公開 Look、
foreign producer、過期觀察與未付分支不會因取得成員讀取入口而被放行。

公開區 distinct names 仍使用規則名稱集合的聯集，公開 Look 的上述單一名稱政策則保持原樣。
這兩種已有行為不因共用 adapter 而選成同一算法；多名稱語義未有依據前不擴大其範圍。

`power_of` 與 `cost_of` 的具名材料 Before 讀值共用 `PaidMaterialNumber`，僅接受
Cost／Attack。材料必須是必付的一張己方場上從者，送往擁有者墓場；
數量、付款來源與已付支配關係由 WF 驗證，runtime 再檢查 Payment 數量與保存的
Characteristics。零費是有效值，缺失資訊則失敗；有號攻擊值仍保留。
After、多張、可選未付分支、非付款的移動與其他數值屬性不取得這個投影。

### 場上送墓的原費條件

己方場上 current selector（可扣除既有對象）整批 Put 到擁有者墓場時，
可記具名結果，並以成功 Before 成員的 `orig_cost_eq`／`orig_cost_ge` 判定是否存在。
`ir/receipt_reads.rs` 的 producer 形狀同時供 lowering 與 runtime 准入使用；
其他目的地、對手來源、After、foreign producer 與 iteration 不取得這個授權。
既有公開場上 selector 的 `orig_cost_ge` 則沿共用 ObjectFilter 讀原費。

movement fact 保留離場前的 base printing 身分；field adapter 只從這個不可變身分
讀原費，不查付款後有效費用，也不從目前物件重建完整 Before。
它沒有保存 kind／trait／class／names，因此這些 field receipt 條件仍拒絕；
它們可以用於移動的公開 selector，不能假裝也是回條保存的特徵。

成功仍須符合原 Field 的 Put fact、相同物件的新世代與實際墓場目的地。
已在墓場的卡、空結果或 Stack 替代後仍留場的成員不算成功；
從者 token 確實送墓後被抹除，仍可讀保存的印刷原費。
一般的 `count_of` 與 `did` 也能讀這個具名結果；它們只看成功移動，不讀成員特徵。
`count_of` 可進既有算式，`did` 判斷是否至少一次成功；兩者沿原 movement_before
規則（相同物件的新世代且存在 actual destination），不把目前墓場張數當成本次結果。
此入口不開放新的 RedirectMove 替代鏈，亦不開放依成員特徵篩選的
`ReceiptCount` 算術聚合。

BP17-040 所需的「己方原費至少 6 的機械從者整批送墓、具名成功結果、
原費等於 6／7 的 Before 條件」屬於這個組合。這只說明 reader 契約；
原費降費頭、觸手 token 的疾走與 Lastword、整卡其餘本文等閉包仍須各自驗證，
不能用這個片段宣稱整卡已准入。

## 觀察集合的 consumer 邊界

觀察集合的准入走訪與執行讀取共用 `ir::observation` 的葉分類器；它依 consumer 與 input／except 位置
保留不同文法，不把各入口的允許來源取聯集。分類器不建立全域 receipt 白名單，也不取代
原本的 producer／scope 證明、指示 metadata 與使用時的世代、區域及位置檢查。

| consumer | 觀察葉與位置 | 結構與外層限制 |
| --- | --- | --- |
| Order | Look Before／After；Order Before 只在 except 略過公開 Capture 檢查；其他葉仍經原完整 Capture 檢查 | Union／Difference 遞迴、空 Union 保持空；RevealUntil 仍由全域 prefix 閘門拒絕；Bound 與一般 selector 不因共用分類器增加私有讀權 |
| 私有觀察移動（墓場／EX） | Look Before／After；except 只接受 Move／Order Before | Union／Difference 遞迴；目的區、Put、position、order、posture 與 entry effects 的原限制保留 |
| 已公開 Look 移到手牌／場上 | 原有直接 local Look Before，沿觀察移動文法檢查 | 執行仍讀已公開 Look，不借用私有 reader；尚未公開的讀取仍失敗 |
| Bottom placement 後洗回 | Look Before 或 RevealUntil Revealed Before；except 限單個任意 Movement Before | Difference 只在 input 遞迴，Union 拒絕；必須緊接既有 Subset Shuffle，prefix／suffix 契約不變 |
| Look Reveal | local Look Before；私有 Look 實際選中的 Bound 仍在自己牌庫 | Bound 的選中手牌與單一整手牌仍走原獨立路徑；不授權任意 deck Bound |
| Look 選擇 | Filter 的 input 為 Look Before | actor、visibility、群組、名稱互異、aggregate 與私有候選的條件政策維持原範圍 |

Order 的一般葉必須經原 Capture／私有讀取的完整樹檢查，不能把「可求值」當成准入證明。
例如藏在另一個 selector 下的私有 Look，不會因為外層被歸到一般葉就得到權限。
Movement After 不會被誤讀為 Before；在 Order 中仍依原一般 selector 路徑處理。

`Execution::observed_objects` 保留唯一的觀察集合運算：差集兩側都求值；聯集依物件與世代去重，
保留第一次出現的順序。Look 仍檢查 viewer 並讀本次觀察；私有排除葉的 Movement Before
直接重用 `receipt_members` 的成功成員投影，RevealUntil 直接讀原有揭露結果，不重新查現在的牌庫頂。
Order 的已驗一般葉（包括其 Movement Before）仍派到一般 selector 求值器，保留原有的成員去重；
其他 consumer 沒有通用退回路徑。

同一 Look 的後續選擇只列目前仍在自己牌庫、世代相符且保有位置證據的候選；先準備完整條件，
再排除失效成員，最後才求值名字、原費等候選條件。移到手牌的牌不能重新落入公開目標路徑。
這不改 Order／Move 最後保留成員的重驗：被保留的過期成員仍使執行失敗，歷史排除項則讀成功 Before。

選中牌仍在牌庫時的揭露讀實際選中集合，與移動共用私有 Look 來源、世代、擁有者、控制者、
牌庫成員及位置證據檢查，不把候選池當成結果。原本無權揭露的 Bound、公開區成員或混合手牌／牌庫
維持 `Invalid`；新 Look 牌庫路徑失去世代或位置權限使用 `AuthorizationIntegrity`。
過期私有手牌的略過仍先執行；Look 選中後移手牌的既有揭露不要求舊選取世代等於新手牌世代。
揭露沿用原到期機制，Order 換位仍撤銷位置知識；記得已公開的卡名不代表能定位排序後的頂牌。

## 事件主體與來源條件

`game/internal/conditions/subject.rs` 把既有事件 filter 的角色與特徵條件編入共用
`ConditionPlan`，由只讀 adapter 提供各 producer 已捕捉的資料。這不把 frozen
expression 改成 current object，也不以臨時物件或 binding 取得讀取權限。

| adapter | 可讀來源與保留政策 |
| --- | --- |
| 事件主體 | 沿原事件時點的 SubjectInformation 與 controller／active／同一來源／token；入場後、離場前 LKI、攻擊與傷害來源各讀原有投影，不重新補齊缺失欄位 |
| 棄牌主體 | 角色先依原手牌事件判斷；只有 filter 需要時才捕捉移動前手牌的 kind／class，不替它補出其他可讀資料 |
| Fusion 母體 | 付款前 source snapshot 保存的 kind／traits；不從付款後物件重讀來源 |
| Fusion 材料集合 | 成功 Fusion 事件保存的 material_traits；這是原有批次聯集，不假造單張成員的其他特徵 |
| 控制者條件 | 只比對事件或來源已帶入的 controller；保留能力列舉前的原控制者閘門 |

卡種的既有差異留在 reader：一般候選依原字串包含政策比對，事件與 Fusion 來源
依已捕捉的 `CardKind` 精確相等。缺失的原費、職業或卡種不變成零費、任意職業或
任意卡種；只有實際保存的資料可用。controller、active、來源身分或 token 等角色
沒有提供時，不取得相應讀取權限。

主體條件在排入 pending 前判斷；本文的 If 仍在原解算時點求值，兩者不互換，quota
消耗政策也不因共用條件核心改變。Fusion 的付款前保存、成功付款後投遞仍是獨立
queue，不放入事件讀取 context。歷史查詢保留自己的准入與保存資料政策，不因
事件 adapter 共用核心而取得新的條件或來源。

事件主體的原費條件接受相等／以下／以上（含 0），仍只讀 producer 保存的印刷原費。
缺失原費維持不匹配；IR 的零費與範圍比較先證明原費存在，不以 `InformationOrZero`
把缺值視為零。入場原費條件仍限原有「己方其他從者」形狀，離場讀既有 Before；
歷史查詢不因共用 filter 擴張原費條件准入。

既有 `on.any_of` 的入場名稱分支，在角色、其他主體條件與時點完全相同、
僅名稱片段不同時，正規化為同一入場事件與一個名稱 OR 條件。每個 fact 最多
產生一個該能力的 pending，名稱同時命中兩片段也不重複。不同事件、角色或 EX
觀察者不取得這項組合權限；原有 Fusion 分支維持原路徑。

入場主體的 `not_class` 先確認捕捉的職業存在，再排除指定職業；未知職業不會
因為 `Not` 變成匹配。它與其他條件同在排入前判斷，不匹配不消耗每回合 quota；
不限定入場方式或只在自己的回合觀察，仍遵循事件原有權限。必選目標無解時，
保留整個能力取消的既有行為，不先執行後續回復。其他事件家族不因此增加職業排除。

## 費用 modifier 與 While 條件

卡片自身的出牌費用 modifier 與靜態費用的 While 條件，透過 `number_plan` 的 Cost policy
編譯完整組合樹，再沿既有數值與布林求值。Cost 葉保留原有公開 selector 白名單，
不繼承 Token 的廣義 Field selector 或任意 EffectAmount 葉；整區 Banish 張數仍可讀
Controller 相對的任意玩家關係，Life／PpMaximum 仍可讀原有玩家。

報價與正式付款都在原呼叫時點、以各自的 Execution 與當下狀態求值；
靜態 While 使用來源 provider 的 Execution。條件為 false 時不求值該 modifier 的金額，
負折扣合法，算術溢位仍在求值時失敗，Delta 累加後只對最終總額取零下限。
這個計畫不消耗額度、不改付款或宣告快照；未決 X、已付結果、AtPlay 與目標／分配
仍遵循各自的既有綁定與時點。靜態進化費用及宣告數值使用的 `public_numbers` 文法獨立保留，
不因共用 Cost policy 而取得手牌身份或更多數值讀取權限。
