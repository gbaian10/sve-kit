# 正式卡片打出、Spell 與選擇快照

本文件是內部 typed IR 的執行義務，補足[執行語義](runtime-semantics.md)的完整 play 程序。
它不代表 executor 已完成；已驗子集以[內部執行入口](internal-execution.md)為準。
不新增作者語法、公開 program、capability 或最低版本。一般觸發與來源離場後的 LKI、
prepared play 終局接點另依[觸發與 LKI 契約](trigger-lki-semantics.md)。

依據為[日文 CR 1.28.0（2026-09-28）](https://shadowverse-evolve.com/wordpress/wp-content/uploads/2026/09/28121513/ShadowverseEVOLVE_cr_1.28.0_260928.pdf)，
核對日 2026-10-21。以下條號均指該版。`active_zones`、snapshot 與交易界線是專案對規則的
表示約定；官方規則沒有規定這些資料結構。

## 1. 有效區、打出起點與來源

三者分別驗證，不能把 `Ability.active_zones` 當作可打出區域清單。

| 入口 | 定義的有效區 | 打出權限與來源 |
| --- | --- | --- |
| 普通 Printed Spell 本文 | `AbilityKind::Spell`、`active_zones=[Resolution]`、Printed origin、具 play 與 body | authority 依卡表確認 Spell、正確 face，從自己的 Hand／EX 打出；進 Resolution 後以新世代實體化來源 |
| 普通 Follower／Amulet 卡片 play | 卡片 play 是規則程序，不另外造一個 Spell 能力 | 同樣先進 Resolution，再依 §2 入場；卡片固有 PP 費用屬卡片 play |
| 場上 Printed Activated | 通常 `[Field]`（10.3.5）；明示其他區需個別支援 | 權限驗來源所在有效區及時間；持有者不因起動而移入 Resolution |
| Printed Fanfare | `Triggered`、`Keyword(Fanfare, Printed)`、`[Field]` | 入場新世代的自身 EnterField 產生 pending；其後是另一次能力 play，不是卡片 play 的選模式／目標階段 |

Spell 的 Resolution 對映依 10.1.1、10.3.4、10.6.2.1、10.6.2.8.2；Follower／Amulet
能力的 Field 預設依 10.3.5。Fanfare 依 12.4、10.7.1–.3，不要求卡片是經 play 入場；
第一版只沿用已受驗的單一自身 Fanfare producer，不因此開放其他入場路徑或 Amulet executor。

載入整卡時驗 header、卡表類型／face／來源閉包及整樹支援；實際打出再驗 holder 世代、
owner／controller、區域、禁止與時點。普通起點限 Hand／EX，不新增 `PlaySpec` 起點欄位。
Spell 的 `[Hand, Ex]` 或 `[Field]` 即使通用良構檢查接受，也不能通過正式整卡准入。
Resolution 本身不給第二次 play 權限。Quick 的時機另外依規則入口授權；第一版可只支援
主動玩家 main，未受驗的 Quick、其他區域打出及效果巢狀 play 一律拒絕。

`ExecutionId` 在這次嘗試開始配發；`AbilityInstanceId` 與 `RuntimeAbilitySource` 由 authority
在 Spell 進 Resolution 後取得。`AtPlay` 的舊狀態值先保存，再交這次 execution；這不讓
舊區域物件參照自動變成新世代。Activated／Fanfare 沿各自實例；成本使來源離場需要 LKI
而管線未支援時拒絕，不能改綁新世代或取消已待機能力。

## 2. 程序、已打出與回復點

開始前先做完整准入與可執行性預檢（10.6.2.1.2）。以下次序對 card play 與 ability play
共用，表中移動卡片及場容量的步驟只適用 card play。任何詢問期間均不插入其他玩家行動。

| 步驟 | 規則與處理 |
| --- | --- |
| 0：保存起點 | 保存整次嘗試的 authority origin，包含 RNG、世代、producer、事件、待機、額度與觀察；在自身移區／付款前求 `play.captures` 的 AtPlay（5.18.3.1）。此時沒有本次模式、X、targets 或付款回答 |
| 1：指定與公開 | 10.6.2.1：卡片公開並移入 Resolution，發生新世代；Spell 實體化此新來源。能力 play 不搬持有者；非公開來源揭示依 .1.1，未支援者拒絕 |
| 2：先決選項 | 10.6.2.2：先處理影響追加費用／模式數的 X（.2.1），再選可選追加費用（.2.2）、ChoiceKeyword（.2.3），最後其他必要 X（.2.4）。第一版拒絕前置 X、成本替代及未支援追加費用，只執行可明確排入此程序的子集 |
| 3：目標 | 10.6.2.3：在共同選擇 snapshot 選所有所選分支的 targets；依基數及合法性驗答。需要正數目標而無法選足時回整個 play 前（.3.3）。非公開區搜尋不是這個目標階段 |
| 4：分配 | 10.6.2.4：同一 snapshot 固定 recipients／total／minimum，先適用受驗的分配取代。空域保存空分配並繼續；非空域每個已選目標至少 1（.4.1–.4.3） |
| 5：決定並付完整成本 | 10.6.2.5：先決定成本，再按卡文順序付款（10.4.2.1）。依 .5.1 的基準、替代、指定值與追加行動順序，僅執行已准入形狀；零費用與合法取代仍可完整付款（.5.2–.3）。不足回整個 play 前（.5.4） |
| 6：卡片場容量 | Follower／Amulet 在付款後依 10.6.2.6 檢查場是否已達上限；不合法則連付款一起回復 play 前。預檢不能代替這個時點的檢查；Spell 不要求場位 |
| 7：已打出 | 10.6.2.7 才形成本次 card／ability played 事實及相應打出計數／額度。移入 Resolution、回答目標或付款開始都不是已打出 |
| 8：解決 | Follower／Amulet 依 .8.1 從 Resolution 進場；Spell／Activated／Triggered 依 .8.2 按本文次序解算，被選模式按印刷序（.8.2.2）。AtResolution 在其既定入口一次捕捉 |
| 9：收尾 | .8.3：仍在 Resolution 的卡去 owner 墓地，能力則移除；不把已移走的卡之後的新世代再拉回墓地。完成本文、Resolution 與 EndResolution 收尾後才形成 resolved，再交 CheckTiming |

步驟 7 是遊戲規則上的「已打出」，不是供外部消費者提前發布部分交易的許可。本文暫停
時可保存本次暫存 facts／receipts，但成功完成前不能把它們當成已提交的完整 Activation。
步驟 9 的 check timing 先處理規則再處理 pending（10.5.2）；入場時可產生 Fanfare 待機，
不能在步驟 8 的卡片 play 中直接執行其本文或先問它的目標。卡片 play 成功後，Fanfare
有自己的 origin、ExecutionId、選擇與成本；Fanfare 失敗不撤回先前已完成的卡片付款。
本文中不新增回應窗口，也不由「沒有待答」推到 end／discard。

首批含 Distribution 的 play 只接受一組 targets，且就是該分配的目標群組；其他多群組
形狀先拒絕，不推論其回退範圍。分配無解（.4.2）回到這組目標選擇前：保留指定卡、
AtPlay 與先決選項，撤銷 targets、
各 AtSelection、分配及其 tokens，再以新 epoch 選目標；若該先決方案根本沒有合法目標
方案，依 .4.2.1 回整個 play 前，不能問無解題或當成 no-op。若第一版無法證明是否存在
合法方案，拒絕該形狀，不能用任意重試次數代替規則。

錯 actor／token／payload 是協定拒絕：維持原題、不消耗 RNG。有效回答提交後的內部執行
錯誤則回本次 origin；不是從付款後的內層 execute snapshot 回復。兩種回復都保留已外發
epoch 高水位，失效 token 不復活。遊戲規則的非法 play、協定錯答及內部 Unsupported
必須可區分；不能把內部錯誤當作「卡片已打出但沒有效果」。不提供看過隨機成本後的任意取消。

### 2.1 EX 承接與費用只付一次

10.6.2.1.3 明定 EX→Resolution 承接既有效果與 counters；.8.1.1 明定
Resolution→Field 同樣承接。這是已知規則，不是來源不明，也不是一般跨區跟隨權。
第一版可只接既有受驗 CostModifier：authority 在這兩個規則移動中把適用對象承接到
新世代，保留原 EffectId、來源、值、時間戳與期限，不重新安裝、不重設期限、不產生額外
安裝 receipt。若其他效果／counter 需要承接而尚未實作，整次 play 在移動前拒絕。
不新增泛用 RuleCarry，也不把此例外套到普通 Move 或 Resolution→Cemetery。

因此付款費用須在步驟 5 依 Resolution 上承接後的有效資訊及受驗 play-cost 規則決定；
不能先移區清掉減費再用印刷成本，也不能僅抄一個舊 EX scalar 冒充完整承接。
卡片成本資訊與僅修改付款額的效果依 10.4.4.1 區分，本片不重解舊 AST 的所有 cost 欄位。
未支援的動態成本表達或付款修正仍拒絕。

Spell 的固有 PP 成本由 lowering 明示在 `PlaySpec.costs`；authority 驗它與同一卡表及
成本規則相符，執行此單一付款計畫，不能再額外扣一次 Catalog 印刷 PP。若形狀只能承載
常值，首批可限定為唯一一個固有 PP Pay、literal 等於該 face 印刷成本；authority 只在
這個經卡表驗證的 card-cost 位置，依步驟 5 的有效成本改算實付量一次。literal 是基準，
不改寫已驗 IR，也不把一般起動 PP 成本套上卡片減費。其他 PP 步驟／追加費用先拒絕，
避免猜測哪一項是固有費用。Follower／Amulet 無 Spell
PlaySpec 時，卡片規則程序負責一次固有成本；其 Fanfare 的成本僅屬之後的能力 play。
`K.pay PlayParameter` 是選中段落的追加付款，與固有成本一起在步驟 5 處理並保存結果，
到本文時只讀結果，不再付款。未支援該形狀時整卡拒絕。

## 3. 同次 play 的快照與 binding

同次 play context 包含兩個有明確用途的狀態視圖，不把 AtPlay 與 AtSelection 合成同一微步：

- `AtPlay` 讀步驟 0 的狀態；ChoiceKeyword 模式數用它，例如 SC／NC（5.18.3.1）。
  同批 captures 同時求值、互不可讀，也不讀稍後的回答。凍結的是值；捕捉到的物件集合
  保留原世代，日後使用仍驗合法性，沒有 LKI 資訊或跨區改綁權。
- 選擇 snapshot 在步驟 1 與已支援先決處理之後、第一個 target 前固定。模式的已選集合、
  targets 與全部 play Distribution 綁同一次 execution／選擇狀態版本；決定模式數專用
  的 AtPlay 讀值仍用前項。先決回答與依序公開的 target binding 只擴充不可變環境，
  不改變該 snapshot 的遊戲狀態。`SelectionSnapshot::AtDecision` 在同次 play 的這些
  decision 指向共同版本，不能每題重新 clone 當時 live Game。
- `Decision.captures` 的 AtSelection 只指向自己的 DecisionId；驗答成功後可讀入口已可見
  binding、這次新回答與該選擇 snapshot 的資訊。同批 captures 互不可讀；不能讀未答
  decision、成本 receipt、付款後 PP／手牌／墓地，或本文結果。
- 根 PlaySpec 參數依序公開；mode parameters 依原良構作用域，只流到自己模式 body，
  不互讀 sibling，也不藉 runtime 合併環境擴張 checker 權限。所有目標須在任何分配前
  完成；首批拒絕須打亂宣告依賴才能符合該順序的參數排列。
- Distribution 只讀同次、支配它的 selected-target binding，`minimum=1`；其 total
  可以讀已合法公開的 AtSelection 值。完整答案綁 execution／scope／decision／slot
  與共同 snapshot，結算 Damage 不重選、不改量；部分目標失效按原量處理其餘目標。

ChoiceKeyword 雖存於 body 的 ModeGroup，runtime 須在步驟 2 取出決定，之後只按印刷序
執行選中 body；不能走到該 node 才問模式。預檢仍走所有未選模式／死分支的支援閉包，
動態目標合法性只驗選中模式（5.18.3.1.2、5.18.4）。需前置 X、未受驗參數相依、條件
內藏 play mode 或跨模式 binding 的形狀保持拒絕；這些是支援限制，不是另造遊戲規則。

## 4. 良構承載與兩個代表局面

現有 `Ability`、`PlaySpec`、`CaptureTime`、`Decision`、`ModeGroup` 已足以表達上述首批
形狀，無須加 IR 欄位。`WellFormedCardProgram` 保存能力入口作用域；正式執行應由它取得
已驗定義與入口環境，不能把帶 play binding 的 body 拆出再當封閉 `WellFormedProgram`，
也不能為通過檢查刪除 binding。合成封閉入口的限制不變。新的 active_zones 對映與
成本／卡表比對屬正式 lowering／整卡准入義務，通用 checker 不冒稱已驗卡表語義。

以下是後續實作的代表驗收，不新增組合矩陣：

1. 合成普通 Printed Spell，`[Resolution]`、自己的 Hand、固有 PP 1，本文抽 1：
   先入 Resolution 新世代、扣 PP 一次、played、抽牌、去 owner 墓地、resolved、CheckTiming。
   同一輸入改為 `[Hand, Ex]` 必須在正式准入拒絕，不扣 PP、不移區；有場位與否不影響此 Spell。
2. 沿[既有分配局面 D-A～C](runtime-semantics.md#14-可重現局面)驗正式 play：選定分配
   total=4 卻選 5 個目標時，回目標前而未付款；改選兩個可答 3＋1，錯答 4＋0 留原題。
   同例另以合成子情境觀察 snapshot：AtSelection 捕捉付款前 PP=4 作分配總量，支付
   固有 PP 1 後目前 PP=3，本文仍消費原 3＋1 分配，不能重讀成 total=3。這不是
   BP08-028 的卡文或付款行為。零目標採既有 Q1069 空分配，仍繼續後句；
   不把此例重建成涵蓋所有能力／玩家／付款形狀的交叉測試。
