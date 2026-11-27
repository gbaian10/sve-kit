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
sim/battle-contract 純 DTO crate 不依賴 Game；規則投影留在引擎整合邊界，Worker 與未來伺服器共用。
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
    | { kind: "withdraw"; question_id: Opaque }
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
引擎的 `Game::battle_input(viewer, question_id, references)` 回傳 `InputProjection`，
以同席、同停點的 `observation()` 與 `input()` 提供完整盤面和所有玩家輸入。
`command(viewer, &Command)` 使用私有配對解碼 wire 答案或投降，再交 `submit_player`；
主行動的攻擊目標代碼各自綁定該攻擊意圖，其他主行動不帶目標。
Waiting 的 processing 只摘要該席已獲授權的最後一格，不表示隱藏的最內層 frame；骰值沿用原生公開值。
Finished 的原因取自權威終局結果，停止作答與投降；EngineFault 不帶診斷或勝者，僅依原生能力提供 `can_concede`。
必要盤面無法投影仍回 `ProjectionError`，不製造空盤面或改判 EngineFault；完整程式閉包 preflight 仍須在互動前完成。
update_seq 只計本席可見變化；另一席的私有回答或略過分支不製造空更新、全局計數或隱藏批次痕跡。
第一次更新是完整基線，既存歷史不播放；其後只附尚未交付的完整事件批次。規則續行不等待動畫。

question_id 綁席位、當次 native token 與題型；OptionId 綁該題候選，保留來源索引缺口與語義順序。
原題不因無關通知而失效；受理、取消、回滾後舊題永久失效，即使畫面相同也不得復活。
query 不配置規則狀態、不消耗遊戲 RNG；代號配置由宿主交接邊界完成，不能傳出實體 ID 或內部指令座標。
每席一次只送一個規則命令；request_id 用於配對本機回條，不提供跨重啟重送保證，UI 不自動重試已送出的命令。
receipt 先於該席相關 update，另一席只收獲授權更新；重答已受理題回 rejected，不二次執行。

非法席位、題期、選項或答案統一 invalid_submission，保留原題且不消耗 RNG；本地可依已知候選提示漏選／重複。
有效輸入造成 native Reverted／Cancelled／EngineFault 分別回報，不改成一般拒答；正常終局與故障不混用。
withdraw 只對 `can_withdraw` 為 true 的當前題有效，受理後回 reverted，與違規回滾走同一條回到原點路徑；其餘情況回 invalid_submission。
必要盤面／題目無法安全投影、格式錯誤或資源預算失敗回 host_error，停用互動；錯誤不包含原始 payload、秘密、卡文或 stack trace。
HostError 表示宿主、數值或必要投影出錯，無法繼續提供安全的互動；projection 也包含呈現來源對應損壞。
BattleInput.engine_fault 則是 native 規則執行故障，仍由權威決定能否投降，不能把宿主錯誤偽裝成規則結果。
投降仍走原生入口，故障時只有 can_concede=true 才可提出；「結束對局」由本機關閉 Worker，不另造無效局勝者。

### 2.2 開局前支援檢查

可信宿主先取得同一驗證 reader 的 `SelectedEffects`、兩席 `PlayerDeck` 與 `BattleReferences`，
在建立或發布任何玩家輸入前呼叫 `Game::check_battle_decks(&selected, &decks, &references)`。
只有回傳的 `StrictAdmission::is_admitted()` 為 true，才以同一份 selection／decks 呼叫 `start_player_game`。
未准入時各席只取得自己的 `gaps(viewer)`；引用背景損壞或必要來源無法投影是宿主錯誤，原始診斷不傳給玩家。

檢查沿用 strict 開局的主牌組、進化牌組、規則產生的初始 EX 資源與完整 token 依存閉包。
整張程式的准入檢查遍歷所有分支及巢狀定義；目前已准入的玩家題型都有正式 adapter，不另建立一份候選合法性求解器。
之後逐一走訪該閉包內已准入能力的來源，以實際印刷面映射及同一個 SourceRef／TextRef resolver 核對；
未走到的分支與可能生成的 token 也檢查，不執行規則、不試跑秘密條件或 RNG。
未選入當局閉包的作者不影響這次支援判斷，完整印刷／卡面身分目錄仍不可裁切。

preflight 不推論規則引用的完整性：公開規則身分目錄只證明可定位，不能把單條 CR／QA／裁定或空目錄當作完整依據。
現有題目沒有提供完整依據集合的可信 producer，因此 `Question.rules` 保持 null；
單獨驗證 RuleRef 時仍由同一 resolver 核對精確 revision 及呼叫點明示的 strength。
原文未提供時沿用已知卡面提示；有精確卡文閉包卻不符 owner／revision／field 時拒絕，不改拿其他版次。
執行後的數值範圍、物件世代與授權仍逐停點重驗；preflight 不保證任意答案合法，也不把執行期故障改判敗北。

## 3. 玩家盤面與文字引用

BattleObservation 完整保留[玩家輸入](player-input.md)的觀察語義，現物件只在 objects 表出現一次，區域以代號引用。

| 資料 | 必須保留 |
| --- | --- |
| objects | ObjectHandle → 已知 CardRef、有效卡面、owner／controller、public／private 知識、正背面、有效特徵與狀態 |
| players | 每席生命、PP／最大 PP、剩餘 EP／SEP、EP 容量／已用格、SEP 開放回合、公開效果、區域總數、已授權成員、容量；主戰者只是席位外觀，沒有假物件代號 |
| zones | deck、evolve_deck、hand、field、ex、cemetery、banish、resolution、evolution、race、drive、trigger、equipment、limit_break |
| turn／attack | 活動席、先攻席、階段、逐席回合數、公開攻擊者與目標；重畫盤面即可保留攻擊箭頭 |
| relationships | 裝備、Ride、出走／Drive 等有型別關係；兩端可辨識才帶 handle |
| processing | 外層至內層的已授權處理脈絡、stage、controller、來源、主語、程序席位、宣告與付款摘要 |
| costs／pending／history | 現行成本的已知／未決部分、公開待處理、已提交的回合歷史；不包含續體、capture 或暫態歷史 |
| remembered | 曾看過的卡面與 owner；沒有現位置、實體副本數或可追蹤暗牌標記 |

`limit_break` 是公開無序區域。素材成員的 `position` 為 null，`relationships` 的
`limit_break` 由素材指向場上的關聯卡。雙方的完整盤面都以必填的
`CardState.limit_break` 表示場上從者的目前階段：`none`（未解放）、`broken`（已上限解放）、
`final`（已最終上限解放）。其他有 `CardState` 的卡片使用 `none`。
階段取自仍在上限解放領域且與從者關聯的素材，最終階段優先。同一張素材可以同時提供兩個階段。
從者離場時失去階段。素材移走時，其提供的階段也會消失（綜合規則 4.17、11.12、14.6.4）。
完成解放時公開 `limit_broken` 事件，以 `final_break` 表示該次解放是否為最終上限解放。

玩家資源皆由引擎狀態提供，畫面不硬編先後手的配置或開放時機：

- `ep`／`sep` 是目前剩餘量。`ep_capacity: Nat` 是本局 EP 顯示槽容量：初始配置保留為容量，付款不縮小它，回復重用已耗用槽，合法增加超過既有容量時擴充槽數。引擎原有 EP 增加沒有固定硬上限，此欄不新增遊戲限制。
- `ep_used: Nat` 是目前已耗用槽數，滿足 `ep + ep_used = ep_capacity`；EP 回復會減少此值，它不是累計付款次數。
- `sep_available_from: Nat | null` 是本席自己的第幾回合起可用 SEP；配置後即使 SEP 用完也保留該回合。`null` 表示目前尚未配置 SEP，例如開局配置前的狀態；`sep = 0` 且回合非 null 則是已配置但耗盡。開放時機與原生合法性判斷讀同一份引擎狀態。

三個新欄位皆必填（SEP 時機可明示 null），不得省略、補假零或由客戶端推規則；數字使用 checked Nat，無法投影則拒絕整份盤面。

不存在的區域以 0／[] 表示。未知區域可畫通用容器，但未支援的必要規則語義須停止互動，不能猜操作。
對手未知手牌只給數量，不給固定 placeholder ID、私有張序或牌組清單；count 與已知成員的差可畫匿名卡背。
失知後舊 handle 退休；歷史卡面不得找回新物件。區域通常公開也不能擅自揭露背面卡。
同一 printing_id 的不同實體不合併；CardRef 由釘住的快照查原文及素材，不另造卡表。
缺資料用明示未知，不能用印刷值冒充當前攻擊力、生命、成本或合法性。
可辨識的場上與 EX 區從者以 `state.stats` 保留現值攻擊與剩餘體力，與引擎對相同區域的 Current 規則讀值一致；EX 目前讀引擎狀態的 power／hp，未新增 EX 攻防修飾的准入。其他區域或非從者的 stats 為 null。

TurnView 在開局尚未選定先攻時，active／first_player 皆為 null，不輸出引擎內部的暫存席位。選定後保留已建立席位，phase 在開局程序完成前仍為 null；逐席回合數依權威席序保留，不因額外回合或跳過回合推算交替。
計數在回合開始時計入，包含目前回合；被跳過的回合不計入。
中立局面省略的逐席回合數視為 0；已提供但不是自然整數的計數、錯型席位或階段必須拒絕觀測，不以 0 或 null 掩飾損壞的權威資料。

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

section 固定為 `effect`（該版主要效果原文）或 `section:<ordinal>`（具名區段的原始 ordinal，例如 `section:0`）；ordinal 是無前置零的非負十進位整數，歷史欄位不可重編。line 沿用該原文已准入 SourceSpan 的正整數；step 只有精確逐步對應才填，未提供對應時為 null，不公開 instruction path。

只有明確 Printed／Granted 證據才能輸出 TextRef：印刷能力的 granted_by=null；授予能力填權威凍結點的直接 giver 有效 CardRef，與文字 owner、目前 holder 分開。同印刷與自授仍屬 Granted。giver 對該席不可知或缺來源證據時保留 Card 提示，不改寫為印刷；holder 依既有識別連結，失知後為 null，不重接新 generation。

RuleRef 使用公開來源的穩定身分：CR 的 id=cr_clause.number、revision=cr_version.id；QA 的 id=qa.id、revision=qa_version.id；裁定的 id=ruling_id、revision=ruling_revision.id。人可讀版本標籤、QA 官網號及內部 RuleId 都不是 wire 定位值。resolver 只驗可定位，呼叫端必須明示逐使用點 strength，裁定須符合該 revision 的登錄，不預設 official。
引用輸入由 `sve_carddb.export.read_api.load_battle_inputs` 讀取同一份已驗證的公開匯出閉包產生：native JSONL 的精確 bytes 與 `ReferenceInput` 一起回傳，後者包含地區、native bytes 的 SHA-256、完整印刷／面映射，以及綁定公開 manifest 的永久卡面／卡文版本／原文欄位和規則來源身分。目錄不依牌組或已降低的作者縮減，可到達 token 與未選用面仍須保留；人工 EP／SEP 資源版次不成為 native 卡片。
本機 `build:engine` 以 `sve-carddb snapshot battle-inputs --published` 取得同一 4.2.0 快照的目錄、引用、引擎規則及程式附件描述。Worker 依該描述從快照快取讀取並核對 DSL 程式包，宿主核對程式、規則及引用的雜湊與身分，依永久版次準備對局。前端以該 manifest 顯示對局的目錄與原文。

native 卡文讀該版次唯一可用的來源觀測。4.2.0 宿主投影遇官網版次的觀測非空且全為 missing_effect 時，可用該面同區唯一採納的卡文版本取得規則事實。無法表示完整目錄、來源模糊或關係不完整時拒絕整批；原文缺失保持 null，不當成空文字。wording 輸入未提供時，印刷引用仍可用，但不能成立精確卡文引用。`BattleReferences` 檢查同一 native 背景、完整面閉包與卡文身分的一致性；公開 manifest／原文欄位存在性的信任邊界仍是既有驗證 reader，不是任意宿主 JSON。確認 owner 與 field 只代表位置存在，不自行證明 line、step、授予者或觀看者授權。

規則身分閉包保留已發布的歷史版本，CR／QA 依地區篩選；沒有地區欄位的裁定僅列為可定位身分，不推論適用範圍。wording 與 rules 同時提供時須來自同一公開 manifest；兩者都只提供身分，來源存在不等於本題的完整依據。非 null Question.rules 仍須有可信 producer 提供逐使用點 strength 與經驗證的完整依據集合，不能由身分閉包或空目錄推定。

只有 rules、沒有 wording 時，manifest 一致性由宿主核對：`BattleReferences::manifest_sha256()` 必須符合建局使用的已驗證 `Snapshot.manifest_sha256`。Rust resolver 的身分檢查不能代替公開 reader 對來源與閉包的驗證。

已證明 Printed／Granted 且帶有 wording 座標的來源，若提供精確 wording 閉包卻無法核對 owner、revision 或 field，則拒絕投影，不以其他版本替代。沒有精確卡文時，SourceRef::Card 保留已知印刷與座標；printing_id 可指文字提供者、holder 可指授予接受者。引擎投影拒絕有 line 卻沒有 face 的來源，不推定面 0；face 與 line 都未知時保持 null。
Declared 保留已公開或本席可知的模式、X、效果要求宣告的數值、目標、可選費用及卡名；未付費的宣告不是付款回條。
題目存在本身只授權回答席：若來源 frame 未獲授權，Question.processing 僅保留 actor，其餘脈絡未知，不在盤面補空槽或改用可見父層。
processing 的 actor 只屬於本席正在回答的效果或規則脈絡。TriggerQuota 由額外的 TriggerAdmission 脈絡承接；暫停的 play frame 不取得 actor 或私有程序輪次，stage 僅保留已授權公開里程碑（未知時為 null）。已知來源不因 stage 未知而被丟棄。source 使用 SourceRef：有精確證據時為 TextRef，否則保留已知卡面／卡文座標提示。
processing.declared 保留各次實際宣告席與已接受內容；這與事件的 Declared 分開授權。
自己發起、尚未過[打出步驟 7](../dsl/play-semantics.md#2-程序已打出與回復點)的打出或起動（不含自動能力與結算中被打出者）仍可撤回或回滾，其他席在此之前看到的是宣告起點的盤面與 processing；成立後才一併看到。一般進化宣告期間，其他席的 processing 同樣只有行動視窗的 declare，不含進化主語。回到原點時，其他席只保留起點時的知識，物件代號的發號水位不倒退。payments 是各筆已完成扣款，包含付款席、實際數量／單位與已知費用群組，不使用報價的 CostSummary，也不宣稱整次 play 已付款完成；報價仍在 costs。
費用區分 CurrentContext／BeforePayment、基本／追加／模式／可選組、PP 固定額與 X、非 PP 材料；未知總額不是零，報價不保證可合法完成。
strict 的支援檢查依當局引擎、規則及程式閉包；automatic 不表示任意答案合法，也不表示官方核可。

### 3.1 已安裝效果

卡片與玩家的 effects 保留已安裝的凍結運算，不從目前攻防或費用倒推。來源失去目前物件連結時不重接；未授權的來源保持 null。expires、main_live、intervals 與 active 分別保留主效期、主效期是否仍存續、重複區間及目前是否生效，不以單一布林值替代。

授予定義 definitions 的 line 是定義清單的提示，依同一 effect 的 source 印刷與卡面定位；source 未授權或缺少卡面時，該提示不能定位卡文、推定面 0 或當成精確 TextRef。這與 SourceRef::Card 自身的座標分開，後者不得有 line 卻沒有 face。

傷害替換描述的 Cap 保留 threshold 與 replacement：每次涵蓋傷害至少達 threshold 時，改成 replacement；兩者皆為 checked Nat，不近似成固定加減量。recipients／sources 的 null 代表沒有該項限制，空陣列則代表目前沒有可授權連結，不可互換；保留已授權向量的順序及重複項。

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
  can_withdraw: boolean;
  auto_answer: { kind: "unique"; answer: Answer; public: boolean }
    | { kind: "not_unique" } | { kind: "unknown" };
  assistance: null;
  rules: RuleRef[] | null;
}
```

purpose 是規則用途、use 是具體細目、shape.kind 是嚴格答案語義；各種合法組合由 native 程序決定。
rules 必填：null 表示沒有經完整驗證的規則依據（未提供），[] 表示明確已知沒有規則引用，非空陣列表示經可信來源驗證的完整依據；不完整的單條不得當作完整集合。null 不代表已確認合法，也不解除其他 strict／preflight 互動阻擋；引用的 strength 不預設為 official。
ProcessingContext 至少保留已授權 stage、actor／controller、SourceRef／subject、規則依據、逐席程序與 ProcessingDeclaration；actor 不一定是 controller。
等待席只取得公開脈絡及骰值／剩餘次數，不取得候選、唯一性、私有題數或隱藏執行堆疊。
can_withdraw 為 true 的題，回答席可改送 withdraw 撤回自己的宣告：上述尚未打出的打出、起動（含 Quick 發動）的每一題 play_parameter，以及一般進化／超進化宣告的每一階段；判斷是結構性的，不逐題型列白名單。
撤回後盤面、亂數流與知識回到宣告前，並以新題號再出同一個行動視窗（Quick 發動則回到該 Quick 視窗）；結算中的題（resolution_choice）、規則題與自動能力的題一律為 false。
UI 可共用 actions、confirm、options、select、order、partition、distribute、number 題型。結算的卡名宣告沿用 options 的選取與答題契約。
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
| C.GuardAct／C.Discard | end_guard／hand_limit_discard | select 守護子集／確切超額張數；都進 actor 自己墓場時依 selected 順序放入；提交後可再出棄牌題 |
| D.OptionalCost／D.ResolutionPayment／D.May | optional_cost／resolution_payment／may_effect | confirm：明列合法布林；宣告付款、結算付款及可選效果不混義 |
| D.BeforeDraw | draw_continue | choose continue／stop，映回 Stepwise |
| D.CostMaterials／D.Discard | cost_material／cost_discard／effect_discard | select 完整集合；按 purpose 回原付款或棄牌答案 |
| D.ExchangeRoutes | exchange_route | choose 真實路線，保留索引缺口及成本摘要 |
| D.Pending／D.TriggerQuota | pending_order／trigger_quota | choose 一個 occurrence；後者附 earlier 歷史，不能回答 earlier |
| D.Integer（原生 X 費用宣言） | declare_x | integer：有效界限／單位／來源，依 native Wait 的來源識別 X，不由形狀推斷 |
| D.Integer（原生結算數值宣言） | declare_number | integer：結算時宣告數值，保留有效界限／單位／來源；對應 NonnegativeInteger／BoundedInteger，依 native Wait 的來源識別 |
| Declare.CardNames | declare_name | choose：恰選一項，選項值為 card_name 與 rules_name |
| D.DiceReroll | dice_reroll | keep／reroll；公開 value、remaining 及 allowed，映回 Integer 0／1 |
| D.Order／D.DamageReplacements | order_cards／damage_replacement_order | order：完整排列、方向、目的地；替代項逐個 TextRef |
| D.DriveTrigger | drive_trigger | accept＋必要 target；公開 icon，拒絕時 target=null |
| D.Search | search | select：完整選取與所有群組分派，含空群 |
| D.SearchPartition | search_partition | partition：本題完整凍結集合、目的地、界限、排序與未選者去向 |
| D.MovementCapacity | movement_capacity | select 恰 available 張到達候選，帶目的玩家／區域 |
| D.Distribution | damage_distribution | distribute：每目標份額、上下限、總量及空集合語義 |
| D.SearchReplacement | search_replacement | choose 原路／替代目的地；producer 補完整目的地資料 |
| D.Targets | targets／resolution_selection／stack_recipient | select：完整集合、群組及角色；正式指定與結算選牌分開。stack_recipient 是 rule_choice 的スタック增加對象，只有一個候選也要問 |
| D.TokenCapacity | token_capacity | choose 完整 token 名稱組合；未建立 token 沒有物件代號 |
| D.Modes／D.ResolutionAlternative | modes／resolution_alternative | choose 模式集合；有效 count、界限與原印刷序 |

choose 的每個選項需有具體 CardRef／TextRef／席位／有型別值及成本摘要，不能只用英文 label 或 array index。
主要意圖保留 Play、Activate、Evolve、Attack、EndPhase，來源不限手牌；選到意圖仍由原程序逐題宣告、驗答及付款。
Attack 的 target 是該 intent 題內選項，不能借另一攻擊者的候選；超進化仍在進化付款題選。
進化可帶 nullable 的超進化／EP 可能性提示，未知為 null，不用提示跳過付款題。

### 4.2 選取與複合限制

`hand_limit_discard` 的棄牌都進 actor 自己墓場時，`selected` 同時指定完整棄牌集合及玩家選定的墓場放入順序，解碼不得依候選順序重排。
依[日文綜合規則 1.29.0](../../authored/rules/comprehensive/jp/1.29.0/ch04.json)與[英文綜合規則 1.28.0](../../authored/rules/comprehensive/en/1.28.0/ch04.json) §4.1.5，同時放入同一區域的順序由該區域所屬玩家決定；§4.9.2 則明定墓場不管理卡片順序。
若有跨擁有者的手牌進其他玩家墓場，不能由 actor 代替其他區域所屬玩家指定順序；該 producer 需另行處理各區域玩家的選擇，不能直接套用本題的單一 actor 順序。
因此這裡指定的是這次放入的順序，不是持續追蹤的墓場遊戲順序；盤面的陣列位置也不代表墓場有頂／底或不能重排。這些牌仍然同時棄置，不因 `selected` 的先後而拆成逐張結算。
此順序語義只適用本 use，不將所有 `select` 或 `roles` 解讀為排序題。

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
| game_started／mulligan、choice_declared／targets_declared、trigger_pending／pending_cancelled | 公開先攻／換牌張數、已公開宣告（效果要求宣告的數值是 number／free，卡名是 name／rules_name）、待機來源與有限取消原因；須有 producer，不能用盤面 diff 補造 |

當步 after 值由 producer 完成動作時凍結，不用最後盤面回填。晉升不得新增當時未授權的資訊。
歷史卡面不連回新 handle；中途曾公開又失知者，若缺獨立觀看證據不得提前披露未提交歷史。
事件的卡面、object 與來源 holder 都是發生時凍結的值，不查目前的物件表：object 與當時盤面物件表的鍵相同，交付時可能已不在目前的表中。
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
私下 Look 選中的單張牌公開揭露後，若緊接著將同一選中牌置頂，`deck_top` 通知在操作完成後交付。
其中的牌庫位置 0 記錄該次置頂的結果。

```typescript
type Notice = {
  id: Opaque;
  kind: "look" | "temporary_reveal" | "deck_top";
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
| 打出、進化（含超進化付法）、起動中途撤回 | 狀態摘要回到宣告前、再出行動視窗；等待席在宣告中與撤回後的盤面、事件、輸入都與宣告前相同 |
| 外層可回復而內層成立、nested 取消 | 不提早外發可規則回復內容；不將 native Cancelled 當 cancel |
| 無 origin 懸置、短暫公開後失知 | 前者不發暫態；後者需獨立觀看證據，沒有新 handle 連回秘密 |
| 故障回 origin／上一停點 | 停動畫、對齊真實故障快照，只 cancel 實際丟棄者；不假勝負、不等待永遠不提交的呈現 |
| 投降／終局截斷 | 提交 native 前綴、promote、result 各一次，未執行步驟不造事件 |
| committed 與 provisional 同份更新 | 先 committed 再 provisional；略過／缺素材最後也必須對齊快照 |
| Look 無題、立即代答、熱座切席 | 通知留在本席 UI、不丟失或給另一席；骰子無重複通知 |
| 事件缺口、必要投影缺口 | 前者停事件但保留安全盤面，後者停互動；不猜事件或答案 |
| 完整本機對局 | strict 合法內容可從開局至終局，全部啟用題型能回答；未實作依存拒絕准入，不用原型假引擎 fallback |
