# 效果 DSL 1.0：承載與執行邊界

語法唯一權威為 `dsl/`；操作見[作者工具](authoring-tools.md)，作者准入依 [ADR-0021](../adr/0021-direct-effect-admission.md)。輕量與共用本體都可直接依卡文撰寫，經相同 lowering 與 runtime 執行，不要求先完成來源分類器。

## 1. 版本承諾與入口

| format／kind | Schema |
| --- | --- |
| `2 / author_effect_set` | `dsl/author-storage.schema.json` |
| `2 / effect_body_shard` | `dsl/effect-body-storage.schema.json` |
| `1 / effects_index` | `dsl/effects-index.schema.json` |

主檔與本體格式為 2；記憶體內材料化的完整能力樹仍用 format 1 author／expanded 資源。astra/1 原型與 DSL 1.0 分開驗證，不能混用 Schema。公開格式 4.1.0 可打包已准入且展開的 DSL 1.0 AST（含 light），其語法由 `dsl/program.schema.json` 定義；內部作者資料可同步修訂。

## 2. 卡片、本體與用途

### 2.1 封套與分片

主檔 `{format:2,kind:author_effect_set,scope?,dsl?,cards}`，預設 shared／1.0。每卡含 `meta, abilities`，可含 `admission, ignored, rule_sections, note, deck_rule, token, token_sources`。meta.written_by 必填，其餘 status 預設 draft，reviews／rulings 預設空。

作者主檔位於 `authored/rules/author-effects/<owner>/`，可用 `<卡號>.yaml` 或至少三位數的 `<sequence>.yaml`。同一個卡包目錄可混用兩種檔名，新增卡片預設以卡號命名。具名本體位於 `effect-bodies/<owner>/<sequence>.yaml`，使用至少三位數的序號。每檔小於 1 MiB。身分由既有登錄及 printing 解出，不由卡號或文字 hash 推測。英文 override 的來源契約仍未提供。

### 2.2 材料化 companion record

具名本體保存 `id, body, written_by`，可有 `admission, edition, params, ports, note`；edition 預設 1。`admission: shared` 表示集中審查的共用本體。等價句型可引用同一 id。

### 2.3 材料化逐用途選版與 inline

用途寫 `line` 加能力頭，或 `use` 加裸參數值；能力頭不能同時與 use 出現。edition 只有唯一版次才可省略。位置使用 face／section／scope，跨行用 covers；提醒／排版以 ignored 明示。輕量與共用本體可在同卡混用。

直接來源座標為 `DirectSourceUse`：admission、永久 face_id、face_revision owner、field／ordinal、line、scope、covers。這是位置及覆蓋責任，不宣稱來源等價證明。展開後不保留 param／generated_by。

#### 獨立 keywords 的非本體來源

獨立 keywords 與能力本體分開降低；其來源座標仍保留永久面與行。支援的印刷關鍵字仍經具名 runtime 宣告及有效區域檢查，未支援的宣告整卡拒絕。

### 2.4 effects-index

索引位於 `authored/rules/effects-index/<卡包>.yaml`，卡包是 `record_key` 第一個 `-` 前的前綴（如 BP06、CP02、PR）。每檔沿用 format 1／effects_index 封套，只收該卡包且依 `record_key`、`scope` 排序；讀取整個目錄合併後仍驗證跨檔 CardId／scope 唯一。新增卡片只更新對應卡包索引，不使用舊單檔路徑。

索引每個永久 CardId／scope 唯一，檔案與紀錄雙向閉合。同卡原型與 current 互斥；切換時主檔、索引與原型移除在同次寫入完成，拒絕卡不回退原型。

### 2.5 每包機器錨點

此承載已由 ADR-0021 取代：主檔直接讀取，不再產生或提交 effect-anchors，也不以修改 Schema bytes 要求全庫刷新。

## 3. 封閉葉參數

直接本體宣告 `params: {name: Nat | Int | CardName | Concept | CardKind | Player | Zone | Phase}`，也接受既有 `{type,source?}` 宣告；用途只提供整數或字串，不接受布林冒整數或整棵程式。宣告名稱與實值必須完整對應且均有使用。替換後由 DSL Schema 驗目的位置型別及封閉值域，scope／binding／receipt 良構由 lowering 檢查。

## 4. 本體版本與新鮮度

作者以具名本體及 edition 選用內容。改本體時跑受影響行為測試，無需 SHA pin、逐用途 bytes 證明或全庫 normalizer 重鍵。

作者入口從建置 DB 讀永久身分與各欄非空行，並用原始座標報告診斷。

## 5. 審核、測試證據與 meta

同語義本體用於 1～2 張不同遊戲卡時可用 light；達三張提示整理共用本體，重印／異圖不累計，也不追溯阻擋已合併用途。共用本體集中審一輪，新增用途核對參數、頭及上下文；長尾相關測試通過即可合併，後續抽查。

不建立三卡兩模型資格、固定五類邊界、runner／case hash、verification JSON 或結果 DB。review 只保留真實紀錄，過期 metadata 不構成機械門檻；測試通過不冒稱來源等價、完整 verified 或官方裁定。真正 disputed／revise 只阻擋受影響卡，來源分類 pending 本身不阻擋直接入口。

## 6. C0 與 D0 的交接

C0 解析嚴格 YAML、展開具名本體、驗 Schema 與整卡行覆蓋，`CardAccepted.lowering_input()` 交給 Rust。直接交接包含 `admission: direct`、expanded tree、永久卡號／CardId、完整面序／revision／費用與各欄非空行 inventory；不含證明摘要。

D0 核對所有能力／ignored 座標與獨立行清單、面與 owner、完整閉包及 DSL 型別，再降低同一套 typed IR。Catalog 依既有面序對快照，直接入口不用卡文字 hash 配面；Game 仍執行整卡能力准入。任一不支援動作、錯型別或錯身分拒絕整張卡及該次選用，不回傳半份結果。

token 定義須接至衍生物本體。D0 拒絕尚未支援的 token 接線與對局機制。

### 守護與光紋的具名宣告准入

印刷 keywords 經完整清單降低、逐元素來源定位及 runtime 有效印刷能力視圖消費。持有者、controller、有效區域、silence 與物件世代責任維持；宣告名稱不代表任意關鍵字均可執行。

## 7. 公開 typed IR 與引擎能力

傳輸 4.0.0 的 dsl-programs.entries 維持空陣列。4.1.0 以必要能力 `dsl-programs-v1` 打包已准入的 expanded AST，移除作者與來源稽核資訊，見[快照傳輸](../schema/export/snapshot-transport.md#34-dsl-程式包與版本准入)。這不是 typed IR 的公開 wire；傳輸准入不授予指定引擎的執行資格，沒有 exact target 實跑紀錄時仍須手動處理。

IR 所需語義能力仍由節點與參數形狀推導，runtime 必須實際支援；light 或 shared 標記不授予缺失的付款、事件、取代、token 或私有資訊機制。actor、合法回答與玩家／AI 觀察隔離保持不變。

程式公開版本與對局權威恢復的保存責任不同；typed 入口對外定版的條件及恢復驗收見
[版本與權威恢復契約](../sim/authority-recovery.md)。目前的內部 IR／計畫型別不構成公開 wire 或 save 格式。

## 8. 驗證時點與拒絕診斷

write 先驗完整候選再寫入；check 與建置只讀作者資料，不改 reviewed 或重算證據檔。Rust 選用驗 lowering，建立對局時再驗實際引擎能力。測試只跑相關範圍，完整回歸交 CI。

診斷保留 stage／reason、卡號、面、欄、行及 DSL 路徑；不列整行卡文、私人路徑或秘密。原型 baseline 的 AST／exact bytes 不再是切換門檻，有用的對局回歸斷言繼續保留。
