# 對戰契約

本契約定義本機 Web Worker 內的 WASM 引擎與正式對戰畫面、基礎 policy 的共同邊界。
規則由唯一權威執行；完整玩家盤面是畫面狀態的依據，事件只供歷史與音畫，不由前端重算規則。
架構決定見 [ADR-0022](../adr/0022-battle-contract.md)，詞義見[名詞表](../../GLOSSARY.md)。
本文件規定實作義務，不表示所有接點已實作；既有玩家入口仍以[玩家輸入](player-input.md)、[玩家題目](player-questions.md)及[主要行動](main-actions.md)為準。

## 1. 適用範圍

首片提供本機 Worker、strict 全自動結算、現有 native 題型、完整玩家盤面、已提交事件、有限暫態呈現及正式 UI；對手可為基礎 policy 或熱座玩家。
strict 只接受完整准入的卡片與依存，不開手動操作，不用空程式讓未實作卡入局。
卡表仍只讀本局釘住的公開快照；查卡、建牌、對戰共用 sim/web，不建立另一份卡表或前端。

未來線上宿主由伺服器持有相同權威與投影，用戶端不取得完整 Game、對手秘密或 seed；線上版再定。
版本協商、跨連線身分與重連、故障關局協定、暫態恢復帳本、偏好同步、計時、通知 ack、持久恢復、觀看流重播、悔棋協商、public／referee 與同時換牌：線上版再定。

## 2. 本機宿主與資料格式

Rust wire 型別定義資料模型；adapter 連接既有玩家入口，不直接序列化 Game、native token 或 Debug 字串。
預定的 sim/battle-contract 純 DTO crate 不依賴 Game；規則投影留在引擎整合邊界，Worker 與未來伺服器共用。
下列 TypeScript 是欄位節錄；完整 tagged union 由 Rust 產生，不以任意 JSON、any 或自由 label 補足缺少的語義。

```typescript
type Opaque = string;
type Seat = "p1" | "p2";
type Nat = number;
type Int = number;
type CardRef = { printing_id: string; face: Nat };
type ContentRef = { id: string; sha256: string };
interface Ready {
  kind: "ready";
  format: 1;
  viewer: Seat;
  identity: {
    snapshot: ContentRef; programs: ContentRef;
    engine: ContentRef; rules: ContentRef; region: "jp" | "en";
  };
}
interface BattleUpdate {
  kind: "update";
  format: 1;
  viewer: Seat;
  update_seq: Nat;
  observation: BattleObservation;
  input: BattleInput;
  events: EventDelivery;
  presentation: PresentationOp[];
  notices: Notice[];
  advice: [];
}
type BattleInput =
  | { kind: "question"; question: Question }
  | { kind: "waiting"; actor: Seat; context: PublicWaiting }
  | { kind: "finished"; winner: Seat | null; reason: ResultReason }
  | { kind: "engine_fault"; can_concede: boolean };
interface Submit {
  kind: "submit";
  request_id: Opaque;
  command: { kind: "answer"; question_id: Opaque; answer: Answer }
    | { kind: "concede" };
}
interface HostError {
  kind: "host_error";
  format: 1;
  reason: "format" | "range" | "projection" | "budget";
}
interface Receipt {
  kind: "receipt";
  request_id: Opaque;
  result: "applied" | "rejected" | "reverted" | "cancelled" | "engine_fault";
  rejection: "invalid_submission" | null;
}
```

Worker 與 UI 使用同一 build，一個格式號即可；不一致回 host_error 並要求重新載入，不做 major／minor 或能力交涉。
初始化由可信宿主載入精確快照／程式包並綁定玩家通道；Ready 每次建局交付一次。
programs 引用公開整包內容，不能是可枚舉出私人牌組的程式子集合 hash。
每席一條有序本機通道；UI 只接當前觀看席，policy 綁另一席。熱座切席須清掉舊席畫面、候選與動畫，再載入該席完整基線。
重建或本機 rewind 必須銷毀舊通道與 UI 狀態；不接受舊 Worker 延遲訊息，不復用舊題／物件代號，不設線上 reset／resync 協定。

資料可用 UTF-8 JSON 或等價 structured clone；不得混入 BigInt、Map、undefined、函式或 class。
Nat 是 0～9007199254740991 的整數，Int 是 -9007199254740991～9007199254740991 的整數；face、計數及序號也受此界限。
Rust 在轉換前檢查範圍，JS 檢查 Number.isSafeInteger；JSON decoder 必須拒絕會在解析時被捨入的非整數或超界數值，不能只檢查捨入後的值。
數值超界回 host_error，不能截斷、飽和、改成字串或回繞；輸出超界則停止互動，不交付半份更新。這限制傳輸表示，不改引擎內部整數語義。
不適用欄位用 null，已知空集合用 []；所有已定欄位必填。卡號保留原 Unicode，ID 不能由名稱或前綴推算。

### 2.1 一致更新與提交

宿主逐局序列化命令，在同一穩定停點取得 observation、input、已提交事件與合格暫態，形成一致更新。
update_seq 只計本席可見變化；另一席的私有回答或略過分支不製造空更新、全局計數或隱藏批次痕跡。
第一次更新是完整基線，既存歷史不播放；其後只附尚未交付的完整事件批次。規則續行不等待動畫。

question_id 綁席位、當次 native token 與題型；OptionId 綁該題候選，保留來源索引缺口與語義順序。
原題不因無關通知而失效；受理、取消、回滾後舊題永久失效，即使畫面相同也不得復活。
query 不配置規則狀態、不消耗遊戲 RNG；代號配置由宿主交接邊界完成，不能傳出實體 ID 或內部指令座標。
每席一次只送一個規則命令；request_id 用於配對本機回條，不提供跨重啟重送保證，UI 不自動重試已送出的命令。
receipt 先於該席相關 update，另一席只收獲授權更新；重答已受理題回 rejected，不二次執行。

非法席位、題期、選項或答案統一 invalid_submission，保留原題且不消耗 RNG；本地可依已知候選提示漏選／重複。
有效輸入造成 native Reverted／Cancelled／EngineFault 分別回報，不改成一般拒答；正常終局與故障不混用。
必要盤面／題目無法安全投影、格式錯誤或資源預算失敗回 host_error，停用互動；錯誤不包含原始 payload、秘密、卡文或 stack trace。
HostError 表示宿主、數值或必要投影出錯，無法繼續提供安全的互動；projection 也包含呈現來源對應損壞。
BattleInput.engine_fault 則是 native 規則執行故障，仍由權威決定能否投降，不能把宿主錯誤偽裝成規則結果。
投降仍走原生入口，故障時只有 can_concede=true 才可提出；「結束對局」由本機關閉 Worker，不另造無效局勝者。

## 3. 玩家盤面與文字引用

BattleObservation 完整保留[玩家輸入](player-input.md)的觀察語義，現物件只在 objects 表出現一次，區域以代號引用。

| 資料 | 必須保留 |
| --- | --- |
| objects | ObjectHandle → 已知 CardRef、有效卡面、owner／controller、public／private 知識、正背面、有效特徵與狀態 |
| players | 每席生命、PP／最大 PP、EP／SEP、公開效果、區域總數、已授權成員、容量；主戰者只是席位外觀，沒有假物件代號 |
| zones | deck、evolve_deck、hand、field、ex、cemetery、banish、resolution、evolution、race、drive、trigger、equipment |
| turn／attack | 活動席、先攻席、階段、逐席回合數、公開攻擊者與目標；重畫盤面即可保留攻擊箭頭 |
| relationships | 裝備、Ride、出走／Drive 等有型別關係；兩端可辨識才帶 handle |
| processing | 外層至內層的已授權處理脈絡、stage、controller、來源、主語、程序席位、宣告與付款摘要 |
| costs／pending／history | 現行成本的已知／未決部分、公開待處理、已提交的回合歷史；不包含續體、capture 或暫態歷史 |
| remembered | 曾看過的卡面與 owner；沒有現位置、實體副本數或可追蹤暗牌標記 |

不存在的區域以 0／[] 表示。未知區域可畫通用容器，但未支援的必要規則語義須停止互動，不能猜操作。
對手未知手牌只給數量，不給固定 placeholder ID、私有張序或牌組清單；count 與已知成員的差可畫匿名卡背。
失知後舊 handle 退休；歷史卡面不得找回新物件。區域通常公開也不能擅自揭露背面卡。
同一 printing_id 的不同實體不合併；CardRef 由釘住的快照查原文及素材，不另造卡表。
缺資料用明示未知，不能用印刷值冒充當前攻擊力、生命、成本或合法性。

```typescript
interface TextRef {
  card: CardRef;
  wording_revision: string;
  section: string;
  line: Nat;
  step: Nat | null;
  granted_by: CardRef | null;
  holder: Opaque | null;
}
interface RuleRef {
  kind: "cr" | "qa" | "ruling";
  id: string;
  revision: string;
  strength: "official" | "generalized" | "inferred" | "undecided";
}
```

step 只有精確卡文對應才填；不公開 instruction path。能力授予者與持有者分開，失知後 holder=null。
Declared 保留已公開或本席可知的模式、X、目標、可選費用及卡名；未付費的宣告不是付款回條。
費用區分 CurrentContext／BeforePayment、基本／追加／模式／可選組、PP 固定額與 X、非 PP 材料；未知總額不是零，報價不保證可合法完成。
strict 的支援檢查依當局引擎、規則及程式閉包；automatic 不表示任意答案合法，也不表示官方核可。

## 4. 題目、合法行動與答案

```typescript
interface Question {
  id: Opaque;
  actor: Seat;
  purpose: "play_parameter" | "resolution_choice" | "rule_choice"
    | "quick_window" | "main_action";
  use: string;
  processing: ProcessingContext;
  shape: QuestionShape;
  auto_answer: { kind: "unique"; answer: Answer; public: boolean }
    | { kind: "not_unique" } | { kind: "unknown" };
  assistance: null;
  rules: RuleRef[];
}
```

purpose 是規則用途、use 是具體細目、shape.kind 是嚴格答案語義；各種合法組合由 native 程序決定。
ProcessingContext 至少保留 stage、controller、已授權 TextRef／subject、規則依據、逐席程序與 Declared；actor 不一定是 controller。
等待席只取得公開脈絡及骰值／剩餘次數，不取得候選、唯一性、私有題數或隱藏執行堆疊。
UI 可共用 actions、confirm、options、select、order、partition、distribute、number、declare_name 九種元件分類；declare_name 只預留，不表示首片增加卡名宣告規則。
不能因共用元件就將嚴格答案變成無限制 options 或省略進化付款、Drive、角色分派等限制。

### 4.1 現有題型對照

D 指 DecisionShape，O／C／E／M 分別指 Opening／Closing／Evolution／MainActions；每列都須有 native → wire → native 的無損映射。
Answer 用表中的語義 tag；use 可以區分同形狀的規則目的，不能由 UI 自選另一種驗答語義。

| 來源 | use | shape／答案與必要資料 |
| --- | --- | --- |
| O.StartAmulet／O.FirstPlayer | start_amulet／first_player | select 恰一個起始護符／choose 一個席位 |
| O.Mulligan | mulligan | keep 或 redraw＋整手完整 bottom_order；不支援部分換牌 |
| M（主要行動） | main_action | action_window：intent＋該攻擊意圖的 target 選項；非攻擊 target=null |
| D.Quick | quick | quick：明列 play／ability 選項及 pass；保留原始 window |
| D.Abilities／E.Form | ability／evolution_form | choose 有效能力／進化物件及目的 face |
| E.Materials／E.Discard | evolution_material／evolution_discard | select 完整材料集合 |
| E.Payment | evolution_payment | pp、use_ep、super_evolution；權威列完整合法 tuple，UI 選一整組 |
| D.ResourceEntryPayment | resource_entry_payment | resource_payment：材料＋use_ep；保留 EP 替代許可 |
| D.WardEntry | ward_entry | confirm：來源卡面、區域、accept 合法域 |
| C.GuardAct／C.Discard | end_guard／hand_limit_discard | select 守護子集／確切超額張數；提交後可再出棄牌題 |
| D.OptionalCost／D.ResolutionPayment／D.May | optional_cost／resolution_payment／may_effect | confirm：明列合法布林；宣告付款、結算付款及可選效果不混義 |
| D.BeforeDraw | draw_continue | choose continue／stop，映回 Stepwise |
| D.CostMaterials／D.Discard | cost_material／cost_discard／effect_discard | select 完整集合；按 purpose 回原付款或棄牌答案 |
| D.ExchangeRoutes | exchange_route | choose 真實路線，保留索引缺口及成本摘要 |
| D.Pending／D.TriggerQuota | pending_order／trigger_quota | choose 一個 occurrence；後者附 earlier 歷史，不能回答 earlier |
| D.Integer | declare_x | integer：有效界限／單位／來源，不擴充成任意宣告數值 |
| D.DiceReroll | dice_reroll | keep／reroll；公開 value、remaining 及 allowed，映回 Integer 0／1 |
| D.Order／D.DamageReplacements | order_cards／damage_replacement_order | order：完整排列、方向、目的地；替代項逐個 TextRef |
| D.DriveTrigger | drive_trigger | accept＋必要 target；公開 icon，拒絕時 target=null |
| D.Search | search | select：完整選取與所有群組分派，含空群 |
| D.SearchPartition | search_partition | partition：本題完整凍結集合、目的地、界限、排序與未選者去向 |
| D.MovementCapacity | movement_capacity | select 恰 available 張到達候選，帶目的玩家／區域 |
| D.Distribution | damage_distribution | distribute：每目標份額、上下限、總量及空集合語義 |
| D.SearchReplacement | search_replacement | choose 原路／替代目的地；producer 補完整目的地資料 |
| D.Targets | targets／resolution_selection | select：完整集合、群組及角色；正式指定與結算選牌分開 |
| D.TokenCapacity | token_capacity | choose 完整 token 名稱組合；未建立 token 沒有物件代號 |
| D.Modes／D.ResolutionAlternative | modes／resolution_alternative | choose 模式集合；有效 count、界限與原印刷序 |

choose 的每個選項需有具體 CardRef／TextRef／席位／有型別值及成本摘要，不能只用英文 label 或 array index。
主要意圖保留 Play、Activate、Evolve、Attack、EndPhase，來源不限手牌；選到意圖仍由原程序逐題宣告、驗答及付款。
Attack 的 target 是該 intent 題內選項，不能借另一攻擊者的候選；超進化仍在進化付款題選。
進化可帶 nullable 的超進化／EP 可能性提示，未知為 null，不用提示跳過付款題。

### 4.2 選取與複合限制

| 形狀 | 必須保留的限制 |
| --- | --- |
| Candidate | 題內 id、已授權主語與來源區域、完整 rules_names、原費與職業；未知用 null，不由卡面重算 |
| SelectionSpec | candidates、不可選 shown、整體 Bounds、每群 eligible／Bounds、分派方式、完整 roles、同名／原費／職業互異、原費總和 |
| Bounds／roles | min 與可為 null 的 max；fixed 也可為範圍，另有 if_nonempty 及 remainder；角色完整、不重疊覆蓋 selected |
| groups | existence 只要求集合有合法分派；explicit Search 必須回所有群，含空群，不混同 roles |
| order／partition | 每項恰一次；完整 bucket、Destination、界限及排序；頂段由上往下，底段最後一項最底 |
| distribute | 每候選一份非負整數、逐份上下限與總和；empty 明列 allow_without_total 或 reject |
| evolution_payment | 只能提交權威列出的完整 tuple；不能任選 pp 或把 EP／SEP 當裝飾卡操作 |
| resource_payment／drive_trigger | 保留 EP 開關與材料聯合限制／icon 導出的目標及接受域，不能降成單一 confirm |

同名依完整 rules_names 的交集，不依翻譯顯示名。有效最小張數可能因不足縮減，不拿印刷下限否決合法少選。
原生分兩題仍分兩題；前端可先編輯草稿，但不得合併或提前提交尚未出現的題。
正式完整型別須逐變體綁定 payload，所有 native 變體明確處理；無法表示的必要限制停止互動，不能交付半份候選。

### 4.3 自動作答

保留 [ADR-0003](../adr/0003-decisions.md) 的規則停點；唯一答案與空集合不是刪題許可。
unique 必須證明完整答案只有一組，包含群組、角色、順序與付款；第一個可行解、求解耗盡或尚未證明都只能 unknown。
宿主代答仍走正常 submit；主要階段即使只剩 EndPhase 也留人確認，Look 通知與暫態不能因立即代答消失。
本機偏好預設關閉，可立即切換；只影響尚未受理的代答，不回滾已受理輸入，不做 revision 或延後到回合邊界的同步協定。
公開唯一題可選擇代答；私有唯一性不決定是否縮短停點。Quick 永遠有 pass，可選全部視窗同策略 pass，不能只在沒牌時省略。
每份已授權更新／通知仍排入本席呈現佇列；人類與 policy 只讀同一對戰契約，不為 AI 開完整 Game 讀口。

## 5. 已提交事件與音畫批次

規則歷史沿用[逐觀看者事件](player-events.md)的提交、因果與發生時授權；adapter 私下追蹤 native cursor，不設網路補頁協定。
EventDelivery 是完整新批次及 animate 清單，或 unavailable；首次觀察基線不播放舊歷史。
每個事件有觀看者本地 id、cause、可空 parent、kind／typed payload、about 及可空 presentation_id；每批有獨立 id。

```typescript
interface EventBatch { id: Opaque; events: BattleEvent[] }
type EventDelivery =
  | { status: "available"; batches: EventBatch[]; animate: Opaque[] }
  | { status: "unavailable"; reason: "projection_gap" };
```

animate 中的 id 必須來自同份 batches，且以整批選入；首次基線 batches／animate 都為 []。
body 的 CardRef／位置／原因／數量及事件存在性都須授權；不能公開實體 ID、未授權成員總數或原始 fact serial。

| 事件群 | 必須保留的語義 |
| --- | --- |
| movement／revealed／facing_changed | 已授權 before／after、方法、原因與卡面；未知抽牌只有匿名移動，不能猜 destroy／discard |
| card_played／card_resolved、ability_played／ability_resolved | 執行席、來源卡面或 TextRef、能力種類；成立與解決分開 |
| attack_declared／damage | 攻擊者／目標、Ability／Attack／Combat、實際量、動作後 life；零傷害不冒充命中 |
| life_changed／stats_changed／resource_changed／counter_changed／posture_changed | 主體、資源或特徵、before／after／delta、實際姿態 |
| destroy_attempted／banish_attempted／replacement_applied／erased | 嘗試、替代與成功離場分開；失敗破壞不播死亡語音 |
| evolved／equipped／relationship_changed | 普通／超進化與 normal／effect 角色、卡面及已授權關聯雙端 |
| fusion／race／drive_gained／drive_trigger／drive_checked | 公開材料／數量、主體、icon、是否接受、目的地；不公開私有 receipt |
| payment／dice／shuffled | 已完成付款及公開成分／最終接受骰值／區域；不帶排列或 seed |
| control_changed／phase_started／turn_started／turn_skipped／result | 控制權與 owner 分開、公開階段及回合、正常勝負／平手／投降原因 |
| game_started／mulligan、choice_declared／targets_declared、trigger_pending／pending_cancelled | 公開先攻／換牌張數、已公開宣告、待機來源與有限取消原因；須有 producer，不能用盤面 diff 補造 |

當步 after 值由 producer 完成動作時凍結，不用最後盤面回填。晉升不得新增當時未授權的資訊。
歷史卡面不連回新 handle；中途曾公開又失知者，若缺獨立觀看證據不得提前披露未提交歷史。
事件投影缺口回 unavailable，安全盤面仍可續行；必要盤面或題目投影失敗則 host_error。
未知非必要呈現事件可略過動畫，未知必要輸入不可猜答；不把 producer 的授權缺口當成可降級的未知文案。

### 5.1 同時性與播放

AoE 各目標傷害同批，互打雙傷同批，之後的同時破壞另批；連續兩次傷害不能因同一能力而合併。
只有 producer 能證明同時性與批次完整；共同 cause 或相同數字 group 不足以證明。投影後只包含本席可見成員，不留隱藏佔位。
完整批次不可拆成逐張呈現或部分晉升。AoE／互打須先驗成批再接音畫，退成單件批次不算滿足同時聲音要求。
非 play 的攻擊→Quick、結束階段守護／棄牌，正常已提交前綴在停點立即交付，不因無 provisional 而失去動畫。
animate 只列本次首次可播放的完整 committed 批次；已經暫態呈現或先前快照涵蓋的普通舊動作只進歷史，費用的首次成立交付依 §6。

## 6. 有限暫態呈現

provisional 供結算中途作答前播放已完成變化，不提交規則 journal、付款、RNG、Activation 或 CheckTiming。
它使用獨立呈現代號；已提交戰報、回合歷史與規則 consumer 只讀 committed，不能用 provisional 再推演誘發或勝負。

```typescript
interface PresentationBatch {
  id: Opaque;
  events: PresentationEvent[];
}
interface PresentationEvent {
  id: Opaque;
  kind: string;
  payload: EventPayload;
  about: Subject[];
}
type PresentationOp =
  | { kind: "provisional"; scope_id: Opaque; batch: PresentationBatch }
  | { kind: "promote"; scope_id: Opaque }
  | { kind: "cancel"; scope_id: Opaque; reason: "engine_fault" };
```

kind／payload 在正式 Rust 中是相互綁定的 tagged enum，上述節錄不允許任意事件 body。
一次暫態交付必須同時滿足：

1. 本層及所有外層 card／ability play 已過[打出步驟 7](../dsl/play-semantics.md#2-程序已打出與回復點)；結構性判斷，不逐卡列白名單。
2. 整個批次已完成並封口，現在是真實輸入停點；所有 enclosing 邊界後續只有內部故障可回復，不能仍有付款／容量／取消的規則回復路徑。
3. 本席在此停點本就可從盤面、公開程序或獨立 Notice 得知每項內容及其存在性；另一席私有中間題不因此多收到更新。
4. 每個暫態批次能與後來的完整 committed 批次一對一對應，body、順序及授權不變；同批成員具相同生命期。

打出成立前不發 provisional／事件語音，盤面可作無規則含義的過場。
成立後第一份合格更新把同次打出已完成的步驟 1～6 一併呈現，按費用→成立→解決的原時序交付；每個完整同時批次各自一個 scope，不設另一套費用資格水位。
已反映於較早盤面的費用仍可在這次交付播一次，不能再次扣款；若當次已直接提交，改由 animate 播一次，不再發 provisional。
不能證明授權或完整對應者不提前發，不累積成日後補播額度；無 origin 的懸置與非 play 未提交程序不發 provisional。

### 6.1 識別、晉升與故障

scope 固定包含一個非空完整批次，後來的批次另建 scope，不追加成員；代號只在本機對局及觀看者內有效。
宿主保存行程內精確來源→presentation_id→EventId 對應，不用卡名、相同 payload 或最近事件猜測；這不是持久恢復帳本。
provisional 不取得 committed EventId、不占其 cursor、不作 committed parent；同 ID 相同內容重送不重播，異內容是 host_error。
promote 與對應完整 committed 批次同更新交付；唯一鏈接是 BattleEvent.presentation_id，集合與 scope 恰好一一相等，body 不變。
整 scope 晉升一次，僅轉入歷史、不再次播放；原動畫尚在播則完成原份，已略過者不重啟。
cancel 只撤實際被內部故障回復丟棄的整 scope；native Cancelled 是規則取消，不能拿來撤暫態。
外層已呈現後的 nested 規則取消不得撤它；若存在此種回復路徑，原本就不符合發布條件。
投降／正常終局依 native 提交已執行前綴，對應 scope promote，再呈現 result；已 committed 前綴不撤銷。

故障時本機一律停動畫、清暫態覆層並對齊安全故障快照；未實際丟棄的結果不能假 cancel，也不假 promote。
不假定所有錯誤都回 origin；宿主按真實回復結果處理，未決呈現不再等待播放，提供可用的投降／結束對局。
若失去已發布暫態的來源對應或無法完整投影後續生命期，以 events.unavailable 停止本席事件／暫態呈現並對齊安全盤面，不能用空操作假裝一切正常。
結束本機對局即可釋放呈現資料，不重啟或轉手動續跑；跨故障與重連的持久生命期：線上版再定。

### 6.2 前端消費流程

1. 驗證完整更新，先登記 promote／cancel 與播放去重；保留上一份已呈現盤面，暫停互動。舊動畫未完則依更新順序排隊，或整段略過，不混播。
2. 先按原批次序播放本次 animate 的 committed，再按交付序播放首次 provisional；同時成員同拍，不同批次不併拍，晉升不再排一份。
3. 動作後數值與歷史卡面只放呈現覆層，可顯示中間生命／PP；不改保存的 observation、不重扣資源、不復活失效 handle。
4. 播完、停用、略過、未知事件或素材失敗後，無條件換成本更新完整盤面，未涵蓋差異直接對齊；只有仍有效的最新題可啟用。故障或結束可立即中斷並對齊安全畫面。

## 7. 短暫通知、聲音與語言

Notice 保存獨立授權的 Look／短暫揭露，不能等最後盤面才回推曾看過的卡；沒有選擇題或立即代答也須交付。
通知是已獲授權的觀看資訊，不是規則事件或額外 confirm，不帶現物件連結或未獲授權牌庫位置。

```typescript
type Notice = {
  id: Opaque;
  kind: "look" | "temporary_reveal";
  source: TextRef | null;
  cards: { card: CardRef; owner: Seat; position: Nat | null }[];
};
```

本機有序交付後由 UI 保留可讀的通知佇列，直到玩家關閉或閱讀；不因下一題／下一份盤面覆蓋通知，不阻塞引擎。
同 id 只呈現一次，切席時隔離本席通知；沒有 ack_notice 或宿主補送協定。這不承諾重開程序後仍可取回通知。
骰子不另設 dice_offer：作答者讀 dice_reroll，等待者讀公開摘要；最終接受骰值由 dice 事件入歷史與音畫。
Advice 是獨立 update 欄位，首片固定 []；未來提示免 ack、過期即丟棄，不加入 Notice 保留責任。

每批最多一句專屬人聲、最多 4 個 SFX；同一批相同素材去重，超量按動作、命中／離場、一般資源順序取捨，不延後播放成另一動作。
互打同歸於盡依次為攻擊宣告語音→雙方交戰命中音→同時破壞的一句人聲與通用音，不能因同份更新到達而併一拍。
CardRef 從釘住快照解析素材：play、attack、destroyed、evolution／activated、turn、result 各有明確角色；缺映射或檔案就用通用音或靜音，不阻塞題目。
不由卡號推 URL，不讓引擎傳音檔時序；provisional 與 committed 共用音畫對照，promote 永不重播。
介面支援繁中／日文／英文；卡面地區與介面語言分開，缺翻譯依已設定退回鏈顯示非空原文或通用用途。
卡片／規則文字依 TextRef／RuleRef 與概念鍵渲染，不從英文 label 或 Debug 正規式抽取規則。

## 8. 手動與輔助掛點

| 層級 | 定義 |
| --- | --- |
| 手動沙盒 | 玩家自行操作與結算，不承諾檢查所有違規或遺漏 |
| 輔助 | 玩家手動操作，同一權威比對規則要求，指出違規與漏步並引用 CR／Q&A；提示時機可選立即、步驟後或回合末 |
| 全自動 | 權威執行規則，玩家回答必要題目；不代表替玩家選策略 |

首片只開 strict 全自動。practice 保留為允許明示手動修正／缺卡交接的練習名稱。
手動動作沿用 ManualOp／共用 primitive，照樣觸發規則、進重播；缺卡須先明示人工交接，不能做白板或故障自動接手。
輔助預留 RuleRef、blocked 與 Advice{severity: violation, code: illegal_action／missed_trigger／skipped_step, rules, expected}；提示由宿主依設定交付，不另建第二份可寫權威。
Question.assistance 固定 null，BattleUpdate.advice 固定 []；手動、缺卡准入與輔助的完整流程：後續另定，本機也可使用。

## 9. 同源生成與邊界驗證

由 Rust 型別產 JSON Schema，再由該 Schema 產 TypeScript 與 validator；TS 與驗證器不走兩份獨立推導。
Rust 型別、產生指令及人寫 fixture 進 git，Schema／TS／validator 為乾淨 build 產物，不進 git；CI 驗可重現及 round trip。
首次建置、sim/web 型別檢查、CI 與相關 pre-commit 檢查須先執行生成步驟，環境需備妥 repo 指定的 Rust 工具鏈。
Worker 路徑在測試與 dev build 跑產生的 validator；release 本機不要求每次 Ajv 全量驗證，但數值範圍、格式號與權威驗答不省略。
必要未知 tag／限制停止互動，非必要呈現才可略過；不以取消執行期驗證為由直接使用不可信 native 回傳。
新題型不能只改前端 union；需 native 產生、驗答、wire 與 UI 同時通過。網路強制驗證與能力協商：線上版再定。

## 10. 驗收

以下是契約行為驗收，不以欄位存在或最終盤面相同代替；使用合成案例及公開 fixture，不輸出私有卡表或完整權威 state。

| 案例 | 要求 |
| --- | --- |
| Rust／Worker 相同輸入，額外 query | 相同規則、RNG、候選與代號語義；query 不前進 |
| 同一快照、不同對手秘密 | 觀察、事件、批次、更新次數及等待摘要不洩露秘密 |
| 私有條件不成立而略過選擇 | 與公開資料相同的有題路徑比較非作答席更新；不因私有省略路徑洩漏停點 |
| 多群搜尋、角色、互異、原費總和 | explicit 含空群、角色覆蓋、有效上下限；顯示重排不改原座標 |
| 全部現有題型與複合付款 | 27 個 DecisionShape 加開局／主階段／進化／收尾完整映射；EP／SEP、Drive、骰子及換牌不縮義 |
| 數值邊界 | ±9007199254740991 可精確往返；超界、非整數、非有限值不捨入，回 host_error |
| 舊題、錯席、錯選項、重複回答 | invalid_submission，不二次付款、不前進 RNG；回滾後舊題不復活 |
| 一個可行解但未證唯一、私有唯一 | unknown 不假稱 unique；不刪規則題，Quick 明確 pass |
| AoE、互打、連續傷害與破壞 | 完整同時批次與真正先後；先通過 producer 成批驗收再驗聲音 |
| 傷害後再選目標、抽 2 棄 1 | 先呈現合法暫態再啟用下一題，兩席各驗授權；晉升不二次播放 |
| 費用→成立→結算停點 | 成立前無事件語音；首次合格更新補費用，與成立／效果保持順序，只播一次 |
| 付款後容量非法、整次回滾 | 沒有未成立暫態；已看資訊不收回、舊題失效 |
| 外層可回復而內層成立、nested 取消 | 不提早外發可規則回復內容；不將 native Cancelled 當 cancel |
| 無 origin 懸置、短暫公開後失知 | 前者不發暫態；後者需獨立觀看證據，沒有新 handle 連回秘密 |
| 故障回 origin／上一停點 | 停動畫、對齊真實故障快照，只 cancel 實際丟棄者；不假勝負、不等待永遠不提交的呈現 |
| 投降／終局截斷 | 提交 native 前綴、promote、result 各一次，未執行步驟不造事件 |
| committed 與 provisional 同份更新 | 先 committed 再 provisional；略過／缺素材最後也必須對齊快照 |
| Look 無題、立即代答、熱座切席 | 通知留在本席 UI、不丟失或給另一席；骰子無重複通知 |
| 事件缺口、必要投影缺口 | 前者停事件但保留安全盤面，後者停互動；不猜事件或答案 |
| 完整本機對局 | strict 合法內容可從開局至終局，全部啟用題型能回答；未實作依存拒絕准入，不用原型假引擎 fallback |
