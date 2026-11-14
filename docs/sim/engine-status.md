# 規則引擎的執行路徑與邊界

`sim/engine`（crate `sve-engine`）是開發中的引擎原型。
本頁記錄執行路徑與持續成立的邊界；進度、排程及待確認項目由
[#1971 引擎重構](https://github.com/gbaian10/sve-kit/issues/1971) 與
[#478 引擎範圍與進度](https://github.com/gbaian10/sve-kit/issues/478) 追蹤。
文件未列的功能不能推定已支援；規格要求也不能當成已通過測試的保證。

- 規則斷言與證據責任：[DSL 驗收規格](../dsl/validation.md)
- 效果 DSL 正式規格：[docs/dsl/](../dsl/README.md)
- 玩家資訊與執行要求：[引擎執行與資訊邊界](../dsl/engine-boundaries.md)
- 公開程式與恢復定版條件：[版本與權威恢復契約](authority-recovery.md)

## 兩條執行路徑

兩條路徑共用 `Game`／權威狀態及部分規則基本動作，不能按新舊檔案目錄整批刪除。
卡片事實均來自有版號快照；兩條路徑的差別是效果輸入與執行方式，不是真卡與測試卡的差別。

| 路徑 | 作者資料與載入 | 執行與使用邊界 |
| --- | --- | --- |
| typed | `authored/rules/author-effects/` 與 `effect-bodies/`；Schema 由 `dsl/author-nodes.schema.json` 等定義；Python current reader 展開完整作者選擇，Rust `ExpandedLowerer` 產生受驗 IR | `Catalog::from_current_selection`／`Game::from_selected_effects` 建立已准入整卡的局面，`game/internal/` 執行 typed IR；有 Rust authority API，宿主接通狀態以 #1971 為準 |
| 舊原型 | `authored/rules/effects/*.yaml`，`dsl/effects.schema.json` 的 `format: 1, kind: effect_set` 選定 `astra/1` 文法；keywords 為 format 1 的 `keyword_registry` | 舊 `decide`／`legal`／`projection` 與 JSON 效果解譯器，只剩引擎測試驅動；共同規則題本已改由 typed 玩家入口執行，原型回放、輔助與 AI 已移除 |

DSL 1.0 已有 Schema、Python typed boundary、作者載入器、Rust lowering、IR 良構與 runtime 准入。
作者儲存格式與展開入口依[承載契約](../dsl/author-contract-1.0.md)及[作者工具](../dsl/authoring-tools.md)，
不能再以舊原型的單一封套概括；語法通過、良構及實際可執行是不同保證。

## 已定的開發邊界

依 [#1971 的維護者決定](https://github.com/gbaian10/sve-kit/issues/1971)，
舊 façade（`decide`／`legal`）、JSON 效果解譯器與舊玩家封包凍結不修，只供引擎測試驅動，並收在 `runner` feature 後；
對局入口建在 typed 路徑。原型 AI、回放與輔助已移除，typed 後繼由 #2206、#2207、#2208 追蹤。typed 仍呼叫的抽牌、移區、攻擊、回合、終局等共用規則基本動作不在凍結範圍。

重構期間不新增窄積木或准入卡片以繞過共同地基；同一合法範圍先等價合併，
組合放寬另列驗收。來源、raw IR、付款、物件世代與揭露權限的檢查仍保留。
讀值與持續效果的共同路徑不得抹掉規則上刻意的差異。
A、B、C 組的交付清單與 D 組後續排程留在 #1971，不在本頁複製工作進度。

AI 第一版使用基礎 policy，僅靠與玩家相同的觀察及決定介面完成對局；搜尋另行處理。
這是介面邊界，不表示完整 typed 對局已驗收；原型 AI 已移除，typed 搜尋與 AI 契約檢查由 #2208 追蹤。
`docs/ai/` 的 v2 profile 仍是未實作設計，目前沒有載入它的實作。

## 載入與執行驗證

typed 的 current selection 保存永久身分、整卡及來源閉包；Rust lowering、良構及 backend
執行准入各有責任，不能以 Schema 通過代替。未知或未支援的構造須拒絕，
不得回退到原型卡片程式。整卡可執行仍須逐卡對局測試及語義審查。

舊 `Catalog` 載入原型 YAML 時，除 Schema 外，還以 `catalog/semantics.rs` 比對
opcode、誘發事件、讀值路徑、選擇器及期間；載入期拒絕結果不代表 typed backend 的接受集合。

## 驗證方式

測試數量與結果由當次輸入與測試輸出決定，不在文字文件保存固定總數。

- 共同題本位於 `tests/rules-scenarios/`，在 typed 玩家入口實跑（`sim/engine/tests/typed_exam.rs`，可信 runner `TypedAdapter`）：
  雙方只用玩家介面出手，裁判保留標籤解析、建局、狀態查詢與原始事件紀錄。
  `tests/engine/typed-known-results.yaml` 逐條列出尚未通過的局面、原因類別（未准入卡、舊語意狀態、typed 缺口、介面差異）
  與確切原因 `reason`（引擎回報的原因，或每個不符的 checkpoint、視角與比對項）；
  清單外失敗、清單內轉綠、判定種類或 `reason` 改變都會讓測試失敗，轉綠時刪掉該條。
  typed 對宣告參數逐題提問，所以 awaiting 以 `AwaitingMode::FirstStage` 比對：題本的複合選項只比第一題決定的欄位。
- typed 的 lowering、raw IR、准入及執行測試位於 `sim/engine/src/ir/tests/`、
  `sim/engine/tests/internal_ir/` 與 `sim/engine/tests/current_repository/` 等；
  共同題本通過不表示這些路徑已被驗收。
- 執行方式與公開測試輸入依 [sim/engine/README.md](../../sim/engine/README.md)。

題本通過只表示受測局面正確，不能推論任意卡片或組合都正確。
規則族正反例與推定強度依 [DSL 驗收規格](../dsl/validation.md)。

## 回放、輔助與資訊邊界

玩家與 AI 僅取得獲授權的觀察與決定題；未公開隱藏物件不得有可跨區追蹤的固定 ID。
裁判狀態、來源證據、續體及含秘密的存檔不可當一般玩家封包。
這是共同資訊契約，不以舊投影已省略部分欄位推論完整符合。

已載入的 typed CardRuntime 目前拒絕序列化與摘要；原型回放與輔助已移除，
typed 重播與輔助模式由 #2206、#2207 追蹤。恢復與對外入口的交付由 #1971 追蹤；
不得據此推論線上恢復已完成驗收。

## 執行與建置邊界

原型解算燃料與列舉上限是實作防護，不是 CR 15.2 的循環判定。
場上容量依規則為五；牌組構築驗證屬建牌器，對戰宿主不能把載入效果等同驗牌完成。

函式庫可用 `--no-default-features` 建置至 `wasm32-unknown-unknown`；
這不表示瀏覽器宿主、JS 綁定、計時或完整線上對局已完成驗收。

執行失敗的內部分類、玩家安全結果及工具遷移規則見[引擎執行失敗](engine-failures.md)。
