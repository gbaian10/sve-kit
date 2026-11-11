# 付款交換打出費用修正

本契約補足[打出費用修正](play-cost-semantics.md) §3：只定同一次 card play 中，
一個可選材料成本換一個常值 delta／set，以及單一常值 PP 加費宣告的內部語義。它是 D0 的實作義務，並非 executor
已支援的聲明；DSL 語法仍以 `dsl/` 為權威，不改公開 program、capability 或最低版本。

依據為[日文 CR 1.28.0（2026-09-28）](https://shadowverse-evolve.com/wordpress/wp-content/uploads/2026/09/28121513/ShadowverseEVOLVE_cr_1.28.0_260928.pdf)，
核對日 2026-10-23；條號皆指該版。卡文與 Q&A 使用已保存的
[日文解析輸入](../../card-data/jp)，不以真卡片段推定整卡已符合正式來源准入。

## 1. 首批形狀

每個可打出 face 至多一組交換；材料恰為自己公開區域或手牌的一個集合、固定正整數張數，
同一實體不得重選。交換的修正恰為一個 literal Delta≤0 或 Set≥0，無另附條件或列表。
一般無付款修正仍沿原契約；所有 set 宣告合計至多一個，包括未選的交換 set。

單一 PP 加費宣告另收 `play.optional_cost: {pp: N, result: enhanced}`，N 必須是正整數；
BP13-035／SD07-021 使用 +4，BP13-060／PR-361 使用 +3。它只將正 delta 加入
同一次打出報價，不另支付一份 PP 材料；不能與材料、`one_of`、`optional_cost_delta` 或
`optional_cost_set` 混用。沒有材料的其他空交換仍拒絕。

| 材料成本 | 來源／動作 | 代表片段 |
| --- | --- | --- |
| `to_cemetery` | 自己場上 → 各材料 owner 的墓地；是 Move，不是 Destroy | BP07-004 プライマルギガント：自然卡 4 張，delta −4 |
| `banish` | 自己墓地 → 各材料 owner 的消滅領域 | BP01-P11 次元の超越（同 BP01-057）：Spell 10 張，set 7 |
| `act` | 自己場上 Stand 材料 → Act；已 Act 者不能支付 | BP07-024 王の一閃：『ナテラの大樹』2 張，delta −2 |
| `discard` | 自己手牌 → 墓場；沿用一般棄牌動作與回條 | ECP01-036 天まで焦がせ祈りの火：ウマ娘 1 張，delta −2 |

材料篩選沿受驗 ObjectSel 的公開種類、種族、規則名稱／名稱條件與原費用常值比較；
缺受驗讀值、名稱解析或 selector lowering 就拒絕該卡，不能靠卡號或私人別名比對。
三類在歷史 `(name, text)` 去重的 31 組付款交換候選中占 16 組（送墓 4、消滅 1、
橫置 11）。這是片段形狀數，包含本文不同的同名卡；不是整卡可用數或 runtime 覆蓋率。

材料擇一另窄收 `one_of` 的兩個完整付款方案：同一 class、同一固定正整數張數，
一支為自己場上直立卡送墓，另一支為自己 EX 卡消滅。BP01-103／BP01-SL15 骸の王
使用此形狀。接受後先選其中一支，再只選該支的足額材料；不能混合兩區，也不能把
從 EX 打出的來源卡算進材料。兩支共用原有常值費用修正。

以下整卡拒絕，包含未選分支：上述形狀以外的 `one_of`、多組交換、`steps`、材料 groups、
upto／any、random、下述形狀以外的 distinct_by、cost_sum_ge、`discard` 以外的材料 payload.result、動態張數、X、
其他非公開區域的材料、上述擇一以外的 EX 材料、其他操作、額外／模式內 optional 或 additional、alt、上述單一 PP 宣告以外的 +N／正 delta、
複數 set、未受驗的 aura／next-play／期限。骸の王的兩種材料必須整組承載；
開闢の予言者要求十種原費各一張。付款交換本身依 CR 屬可選追加成本，
本節拒絕的是**除此單一交換以外**的追加費用，不是把交換改稱另一種遊戲規則。

單一墓場 `banish` 交換可寫 `distinct_by: orig_cost`，從有印刷原費用的候選中選出固定張數，
每種原費至多一張。開闢の予言者以 `orig_cost_ge: 1`、`orig_cost_le: 10` 和 `n: 10`
表示每種指定原費各一張。預檢計算可用的原費種類數，出題後以題內候選的印刷原費驗證答案。
`cost-materials` 題目在此形狀帶 `distinct_by: orig_cost`，各候選帶 `orig_cost`；
同原費的不同 handle 作答無效並可重答，整組可支付才消滅材料與套用費用修正。

## 2. 決定、選材料與付款

沿 [play 程序](play-semantics.md#2-程序已打出與回復點) 的同一次 authority transaction：

| 時點 | 規定 |
| --- | --- |
| 步驟 0–1 | 保存整次 origin；公開打出的卡並移入 Resolution；它不再是 Hand／墓地的候選材料 |
| 步驟 2 | 由 play controller 決定接受／拒絕交換（CR 10.4.7.2、10.6.2.2.2）；只保存 DecisionId 對應的決定，不移材料、不扣 PP、不產生付款 receipt |
| 步驟 3–4 | 依既有規則選 targets／分配；不把成本材料冒充本文 target。首批不允許交換決定影響模式數、targets、分配或 X |
| 步驟 5a | 在任何付款之前，以同一 Resolution 狀態求全部成本修正、固定實付 PP（§3）；接受材料交換時才詢問並驗證足額材料；擇一交換先用既有整數決定選可完整支付的方案，再出該支材料題，仍不修改遊戲狀態 |
| 步驟 5b | 驗證固有 PP 與整組材料均可完整支付，再依 §3.2 消費一次完整計畫；不在本文再次付款 |
| 步驟 6–9 | 付款後檢查場容量，成功後才 played／本文／resolved／CheckTiming；付款產生的事件不在步驟 5 插入能力解決 |

PP 加費宣告沿同一接受決定、目標與付款時點，但不出材料或方案題。
報價先合計增加量與既有費用修正，最後截零；例如印刷 1、加費 4、減費 3 的實付為 2，
拒絕加費則為 0。接受宣告且完整支付後，PaymentComplete 為 true，即使折扣使實付成為 0。

材料由 play controller 選，預設所屬玩家依 CR 10.4.3；第一版只收 self，不詢問對手。
回答綁當次 execution、DecisionId、材料選擇狀態版本與世代。公開區域材料公開候選；手牌候選與選擇只送付款者，棄牌完成後才公開已支付的卡。
材料選擇是付款前的 `CostMaterials`，不是步驟 2 接受決定的一部分，也不是本文 choose。
它使用步驟 5a 的未付款狀態；不得重用本文 targets binding 或在扣 PP 後重新求候選。
這個狀態是付款階段的內部快照，不新增 capture 種類或放寬其他 play 參數的快照規則。
只公布已公開打出的卡與公開材料，不外洩其他手牌候選／固定 ID；兩次詢問間無回應窗口。

接受後少一張材料、包含已 Act 的材料、材料受禁止或 PP 不足，都不能部分付款。
CR 1.3.2.1、10.4.2.2 要求可完整執行；不能套「盡可能」或將已有姿態算完成。
非法 token／actor／重複 ID 等協定錯答維持原題；合法流程發現無可支付方案則依
10.6.2.5.4 回到選擇打出之前，包括 PP、材料、原卡區域／世代、RNG、events、pending、
receipts 與額度。不能改成拒付後繼續、補抽材料或只回 executor 內層。
回復仍保留 epoch 高水位，不能撤銷人已看過的公開資訊或復活舊 token。

預檢必須考慮合法的接受／拒絕方案；印刷 PP 不足或付款前場滿，不足以拒絕整次 play。
BP07-004 的 Q1048 明確允許場滿時先支付自然材料再入場；Q192 明確排除正在打出的
次元の超越作墓地材料。材料離場沿共同事件入口；特殊移區仍須符合既有准入。

## 3. 成本合成與次序界線

### 3.1 先決定完整成本

接受決定只決定是否把交換 change 與材料義務納入計畫；拒絕時兩者都不加入。
依 CR 10.6.2.5、10.10.2.4，所有成立的 set 先於所有 delta，沿原契約 checked 算術與
最後截零：`pay_pp = max(0, checked((set or base) + sum(deltas)))`。
這裡 set 是數值有無，不是以真假選取；set 0 必須生效。不因材料實際先後而重新求成本。
SC／公開場條件亦在付款前求值；送墓改變的場上／墓地張數或消滅掉的 Spell 不改這次已固定的值。
決定值可以控制交換 change，但不能用尚不存在的 PaymentComplete 作成本條件。

官方 Q329（BP01-057）確認 set 7 後仍可套既有 −5 得實付 2；Q996（BP07-024）確認
橫置換 −2 可與另一條公開條件 −1 合計 −3。材料仍全數支付，不能因 PP 已到 0 就省略。
PP 0 依 CR 10.6.2.5.3 視為該 PP 行動完成，不增加 PP；不代表整組交換已支付。
Cost 資訊、Search 與 orig_cost 不讀這個實付量；固有 PP 仍只扣一次（原契約 §2.3）。

### 3.2 固有 PP 與材料的執行次序

CR 10.4.2.1 規定一份成本中的多個行動依文字先後執行；10.6.2.5.1 是**決定成本**的順序。
後者本身不足以證明分置固有費用與 optional_cost 欄的行動一律 PP 先付或材料先付。
本契約不把 mapping 鍵序、CostSpec 陣列位置或「追加」一詞當成這項遊戲裁定。

只准入順序不改變合法性、付款結果及事件捕捉的組合：

- 固有 PP 與材料的成本條件在 §3.1 固定；材料選取、禁止條件及特殊移區維持既有准入。
  置換、預防、代付、連結與進化材料等未受驗組合仍拒絕。
- PP 付款不產生誘發事件。已准入的移動、捨棄與姿態訂閱只讀受驗的事件欄位、來源資訊與
  特徵；一般誘發時捕捉仍拒絕，進化歷史的既有例外不參與材料付款。
- 即時讀取目前 PP 的常駐宣告仍在交換開始前回報 Unsupported。檢查涵蓋已載入定義，
  以及已安裝授予定義的條件與目標選擇器，沿共用 requirement 推導辨認 PpCurrent；
  不按目前真假、來源是否在場或私人區域內容挑選性略過。這避免依 PP 開關的守護或
  授予訂閱改變材料事件。PP 上限、EP、SEP 不因本次 PP 付款而改變，沒有這項限制。
  授予能力的本文不在此即時計算範圍；本文中的 PP 讀值仍在完整付款後求值。

對此受限子集，實作仍在同一交易內先固有 PP、後材料操作；這是可交換操作的內部表示，
不是通用 CR 次序。若未來需要同時支援 PP 相依的常駐授予與交換，須另確認印刷 PP 與
追加材料行動的規則先後，不能以相同最終棋盤代替事件捕捉的證明。

付款材料使用共同移動、捨棄與姿態原語，在實際動作時捕捉事件與來源資訊，並進入同一
pending 佇列；不另外建立付款事件佇列。材料自己離場時封存原世代的來源，其他已待機
能力也沿共用離場更新。任何付款或外層交易失敗，待機、額度、回條、RNG 與局面一起回復。
`internal_pending_choice` 在 execution／checkpoint／prepared 忙碌時不提供選序，付款中間
不解決誘發；卡片付款與本文完成後，依既有 CheckTiming 選擇待機能力。

付款不再因已准入的事件觀測者而整體拒絕。手牌棄牌不產生場上離場事件，場上送墓則會
產生實際離場與謝幕待機。Q2052 的『真実の信者』作為ライオ材料先入墓，不觀測之後的
CardPlayed；Q979 的離場待機與 Fanfare 依共同選序處理。多組付款、材料 X 等其他形狀
仍維持原有界線。

## 4. 決定值與 receipt 的承載

以下為待實作的最小內部增補；沿既有 `FacePlayCost` 自身修正入口，不能放在 Fanfare：

```text
FacePlayCost.exchange: Option<Located<PlayCostExchange>>
PlayCostExchange {
  scope: ScopeId,
  decision: DecisionId,
  materials: Vec<Decision>,
  route: Option<DecisionId>,
  cost: CostSpec,
  change: CostChange,
  receipt: ReceiptId
}
PaymentReference.CardPlay { face: FaceId, receipt: ReceiptId }
```

`decision` 是此 exchange 專用的二值問題，actor 為 play controller、visibility 為 Public，
答案恰為接受／拒絕；authority 以當次 card execution 保存
結果，只用於本組計畫，不偽裝 `Binding<Nat>` 或付款 receipt，也不新增通用決定讀值運算。
`materials` 保存各支的 Controller／Public `D.play-param CostMaterials` Objects 決定，
selection_snapshot 是該材料決定的付款前版本；它的 binding 是材料集合的唯一定義。
單支 cost 恰為一個 Ordered step；兩支則為 Alternative 裡兩個單步 Ordered，與材料決定依序配對。
InstructionContext 為 Cost，指示以該支 binding 選取足額材料；
CostStep.materials 留空，不重複定義 binding。原子依 §1 為 Move（Put 到 owner 墓地）、
Banish 或 Posture（Acted），不降低成本文 Move／Destroy／May。現行通用成本良構不收
非來源 Move 與 Banish，materials 也只配 Discard／Posture；本入口另行准入這三種原子，
不放寬能力或其他 play 的成本。
scope 封閉材料 binding 與成本指示；既有 face 條件不繼承它。decision／材料／receipt
身分須唯一且保留各自來源座標，不以 author mapping 遍歷順序生成執行先後。
face 可以只有 exchange 而沒有普通 modifiers；兩者都空才是空項，face 唯一性不變。

PP 加費宣告使用同一 `PlayCostExchange`：`materials=[]`、`route=None`、
`cost=Ordered([])`、`change=Delta(N)` 且 N>0。良構與 runtime 共用此封閉形狀判定；
材料交換仍不接受正 delta。PP 宣告的完成回條只參照唯一固有 PP receipt，不另造付款指示。

`receipt` 是整組交換的 PaymentResult：拒絕在步驟 5 結束記 Declined／不完整；接受且
固有 PP 與材料都付完才記 PaymentComplete=true。付不起不產生可供本文消費的結果。
公開材料自身的動作 receipt 仍只供 authority 記錄。手牌交換的 `discard.result` 可具名其棄牌回條，
僅同 face、同次普通 Spell 在完整付款後可用 `cmp: {left: {count: {in: {objects_of: fuel, view: before}, orig_cost_ge: 7}}, op: ge, right: 1}`
讀取已棄卡的原始印刷費用，沿用 `Predicate::ReceiptAny`，不受目前費用或移區後狀態影響。拒付時回條為空，查詢為 false；
成本、目標、captures 與其他能力不能提前或跨次讀取。這個結果不是 Fanfare 的付款來源，亦不授權讀取其他手牌。既有合法置換可算付款成功是 CR 10.6.2.5.2 的一般規則，
但本版依 §3.2 拒絕置換，不據物理移動數替未支援置換猜付款成功。

wrapper.result 若有名稱，只降低為上述 CardPlay 的具名 PaymentComplete 讀值；
一般 Local receipt 仍不能跨 scope；CardPlay 只帶 ReceiptId，不包 Local／Iteration 位置，
也不放進本文 scope 的 receipt 表。WF 只在同 face、同次普通 Spell 的步驟 5 之後
允許 CardPlay 讀值，確認該 receipt 確由該 face exchange 定義；不從任意 execution
查歷史。Follower 沒有同次 Spell body，不能把它的付款結果直接注入 Fanfare／Lastword
或下一次 play。成本、captures、targets、模式數及材料 selector 中的讀取一律拒絕。
lowering 不為交換產生本文 `K.pay PlayParameter`；本文另有這種付款節點屬 §1 的額外
optional，整卡拒絕。同次本文除上述 PaymentComplete 外，只增加前述已棄卡回條查詢；不保留兩份付款描述。

## 5. lowering、良構與准入

- DSL 入口接 `play.optional_cost` 的單一 mapping 或 §1 的兩支 `one_of`（每支 exact Material，wrapper.result
  可省略），並且恰有一個 literal `optional_cost_delta` 或 `optional_cost_set`。
  單一 PP 宣告則只帶正整數 `pp` 與可選 `result`，不帶上述 delta／set 欄位。
  兩種入口均不帶 option、不帶 choice 關聯；材料交換缺修正、帶列表或條件式交換修正均拒絕。
  公開材料沿用物件篩選器，包含 `name_contains`；候選 binding 換到材料決定的獨立
  scope 時保留完整篩選條件。名稱包含並不放寬卡種或直立資格，例如 CP04-036 只接受
  名稱含「ペコリーヌ」的直立從者，仍須恰好一張，不能以錯名、錯卡種或已橫置的卡付款。
- lowering 把材料與修正或單一 PP 宣告構成 face 的唯一 exchange，與一般 modifiers 並列；不讓 exchange
  change 同時進普通 modifiers。Spell 的 `PlaySpec.costs` 保持唯一印刷 PP literal，
  Follower 仍走卡片規則成本。既有來源 reader 若只能看 abilities，須接 card-level play
  才能開放此功能；不新增DSL 欄位或從卡文 regex 自動由 DSL 推導效果。
- WF traversal 檢查新增樹、scope／支配、decision 與材料身分、receipt 定義及時點，
  合計 set 宣告數與 literal 值域。整卡准入另驗來源、卡表、印刷費用、所有未選分支及
  §3.2 動態局面；保留既有 qualification 與來源閉包 guard。
- 內部 requirement traversal 必須走 exchange 及 CardPlay 讀值，使用與普通自身修正／
  安裝 CostModifier 不同的內部需求識別；未實作不能因已有 `effect.cost-delta` 而通過。
  不改公開 capability／版本。Unsupported 保留原 author path，不當作拒付或 no-op。

代表 IR 正例為 `exchange(cost=墓地 Spell exact 10, change=Set(7), receipt=r)`，
與已承接 delta −5 得 PP 2；同次 Spell body 可讀 `PaymentComplete(CardPlay(face,r))`。
反例把該讀值放進 modifier condition，即使宣告順序先寫 exchange 也須拒絕，不能用
false fallback 讓卡片通過。例子只測承載，不要求次元の超越的追加回合本文已實作。

## 6. 最小代表驗收

實作時只補以下行為情境；不建立操作、玩家、條件與 receipt 的交叉矩陣。

1. 合成公開材料 Spell：接受橫置 2 張換 −2，另有成立的 −1，印刷 4 得 PP 1；材料一次
   變 Act，本文讀 true 且不再扣費。同情境拒付得 PP 3、材料不動、本文讀 false。
2. 沿 §5 的 set 7 與 −5 正例支付 PP 2、墓地十張消滅；少一張或 PP 不足回整個 origin，
   不能留下減費／部分移動。正例同時驗來源卡仍在 Resolution，不能作自己的墓地材料。
3. 合成場滿 Follower，送墓材料騰出場位才入場；本文／觸發仍沿既有准入。可合併一個
   付款前成立、送墓後不再成立的公開場上張數門檻子情境，證明付款送墓不重新計算 PP；
   不增加另一套 snapshot 矩陣。
4. §5 的提前 receipt 反例仍在靜態時點拒絕；材料誘發驗證完整付款與本文後才開放待機。
   另以 PP 相依常駐守護的離場訂閱，驗證 §3.2 在付款前完整拒絕；其他拒絕沿既有准入。
5. 單一 PP 加費 Spell：印刷 1、宣告 +4 與減費 −3，接受付 2、拒絕付 0；
   減費使總額成為 0 時，接受回條仍為 true。付不起回整次 origin，不出材料題，
   固有 PP receipt 只支付一次。BP13-035 的三組具名搜尋可少找（Q1743）；
   BP13-060 沒有必要敵方從者目標時不能打出（Q1770），不留下加費或移動。

## 7. 兩支材料的執行邊界

IR 將材料決定與既有 `CostSpec::Alternative` 的單步成本依序配對，使用獨立的方案 DecisionId；
良構檢查拒絕跨支 binding、聯集選擇器、不同張數、對手材料與巢狀選項。
未選支不綁材料、不產付款回條。選擇失敗保留同一次問題；整體無法付款回復原 play origin。

場上一般 token 可經既有 Move 送墓後依 token 規則消失（Q271）；授權只限這次已驗證
整卡交換、Resolution 來源與所選材料 slot 的當前世代，不偽造起動能力。材料事件沿 §3.2
的共同捕捉與待機；連結物件、進化材料與 PP 相依常駐宣告仍拒絕。一般單支交換與其他
token 移區不受此 token 授權。

目前職業比對沿既有候選查詢讀印刷面；尚無已准入的職業改寫效果，不能以本片宣稱支援改寫後的職業。
