# 作者資料與引擎的驗證責任

語法權威是 `dsl/author-nodes.schema.json`。通過作者語法、來源准入、IR 良構與執行能力檢查是不同保證；
任何一項通過都不能代替其餘項目。本文件列出各邊界的責任及可合併的重複，
配合[作者承載契約](author-contract-1.0.md)、[作者工具](authoring-tools.md)與[引擎邊界](engine-boundaries.md)使用。

## 責任表

| 邊界 | 負責的保證與現行入口 | 不保證的事項 |
| --- | --- | --- |
| 作者 Schema | 封閉物件、欄位型別、互斥鍵、局部組合與數域；`author-nodes.schema.json`、`author-storage.schema.json` | 名稱的 producer 是否存在、scope／付款分支可見性、來源閉包、runtime 是否實作 |
| Python typed boundary | `contracts/author_*.py` 把外部值解碼為封閉型別；局部驗證須與 Schema 對齊，`test_author_schema_drift.py` 比對兩者 | 語法可解碼不表示可執行；不得以未驗證 dict 取代解碼結果 |
| 具名本體與參數展開 | `domains/effects/author_direct.py` 的 `materialize`／`_body`、`author_expansion.py`；確認本體選擇、參數名稱與型別，保留來源位置；`author_validation.py` 排除殘留參數 | 宣稱 import 型別不構成其來源或語義證明 |
| 非局部 binding 與 ports | `author_bindings.py` 的 `analyze_bindings`／`validate_author_ports`：依 producer 順序、種類、分支、execution、alias 使用點與匯入匯出 scope 驗證 | Schema 只驗單一片段，不能承接；此分析本身亦不證明 source port 的語義 |
| 來源與整卡准入 | `author_direct.py` 的 `check_coverage`、`author_set.py`：永久身分、所有印刷、面／版本／區段來源與作者選擇閉包；已辨識能力及明示 reminder／token 區段完整對應 | 行數相同不等於來源閉包；來源分類 pending 不等於禁止輕量准入；此層不證明卡文與效果語義等價 |
| Rust 作者解碼與 lowering | `ir/lowering/ast.rs` 封閉解碼；`ExpandedLowerer` 把支援的作者結構轉成帶來源的 IR，建立 binding 與數域轉換 | 作者語法全集不等於 backend 支援集合；不能為繞過 lowering 改用 raw IR |
| Rust IR 良構 | `ir/wellformed.rs` 及其子模組：raw IR 和 lowering 產物均檢查型別、scope、producer、來源、付款 dominance、觀察與讀取權限，產生私有建構的 `WellFormedProgram`／`WellFormedCardProgram` | 良構不等於 executor 已支援所有動作與讀值組合 |
| Rust 執行能力准入 | `game/internal/admission.rs`、`card.rs` 與各 consumer 的檢查：確認已良構程式在現有 backend 可執行，拒絕未實作形狀 | 目前的形狀辨識不是通用求值保證；未來共用計畫亦須保留 raw IR 邊界 |
| 執行時動態重驗 | `game/internal/` 的付款、選擇、receipt、移區及揭露處理：本次付款成功、物件世代／位置、實際成功成員與 reveal 授權 | 靜態准入不能預先保證局面仍相同，亦不能把選中成員當作實際成功成員 |

`author_bindings.py` 已先經 `AbilityHead` typed boundary，再按 Schema sites 走訪。
其中 dict 是已驗證型別的承載方式，不是允許任意輸入直接進分析器。
Rust 的作者路徑、raw IR 路徑與最終動態使用點均有自己的信任邊界，不能因前一層檢查過就移除。

## 接受集合對照

以下是現行實作的對照基準，並非要求所有層接受相同集合。語法可用與整卡可執行須分開驗收。
Python／Schema 的最小合成案例見 `carddb/tests/contracts/test_author_validation_layers.py`；
Rust 欄位及控制分派的說明是程式盤點，不宣稱這些合成案例已通過 runtime 對局。

| 寫法 | Schema／Python | Rust 作者 AST／lowering | raw IR 良構／runtime | 真正需改的責任 |
| --- | --- | --- | --- | --- |
| `create.n: {per: {of: {count: {who: self, zone: cemetery}}, unit: 5}}` | 接受；`A.act.create` → `A.count-amount` → `A.quantity` → numeric operation → `A.expr.per`，Python 對應 `CountAmount`／`ExprPer` | `ast::TokenCount` 只有 Fixed／Receipt／Query，無 per；`tokens.rs::token_count` 只降低這三種 | `wellformed/producers.rs::tokens` 已交給一般 natural checker；runtime `tokens.rs::specification` 仍只收 literal、receipt projection 或 zone count | token AST／lowering 與執行讀值計畫；不擴 Schema／Python，不預設須放寬 wellformed |
| `per.unit: 0` 或不存在的 `div` | 拒絕：單位須正整數、未知運算鍵不接受 | 不因 token 試點增加別名 | raw IR 仍須符合自己的數域與讀值契約 | 無可刪的防護 |
| static 的整能力 `while` 加無條件 Grant | 接受 | `static_keywords.rs` 把 head 條件放入每條 declaration | runtime `card/static_keywords.rs` 逐條判定已支援的 declaration | 不應把既有整能力條件視為逐條條件已可作者化 |
| static `effects` 中 `if.then` 包 Grant | 接受 `A.static-grant-if`／`StaticGrantIf`；then 恰一條 Grant，無 else 或巢狀條件 | `StaticConditional` 只解碼單一 Grant 或既有 prevention 分支；`static_keywords.rs` 每條保留分支條件，與 head while 取交集，維持同一來源能力 | IR Continuous declaration 的 condition 沿既有逐宣告求值與讀權限；普通場上 Grant 以外的條件式仍受原邊界限制 | Schema／Python 局部形狀對齊；跨家族載入／分派仍另行驗收，不藉拆成多個能力繞過 |
| static prevention If 特例 | then 恰一條 prevent，`kind: ability`，`on` 可省略、只能是 `this`，無 else | `prevention.rs` 額外要求同能力含 removal 禁止，且不能同時有 head while | 只保證既有 prevention 路徑的範圍 | 逐條條件擴充仍須保留此既有語義；語法接受不是任意 prevention 可執行 |

原費／有效值、當前／凍結／LKI、選中／成功、公開數量／私有身分均非可互換的值；
後續共用讀值不得消除這些區分，也不得將缺少快照或未獲權限的值統一補零。

## 合併或刪除檢查的條件

本責任表不刪除任何驗證，也不將 Python 縮成 Schema 加行數檢查。
後續每項實際刪除都須在同一變更列出原檢查、承接入口、等價負例及兩種入口的結果。

| 候選重複／防護 | 承接責任與同等負例 | 可刪範圍 |
| --- | --- | --- |
| 手寫 Schema／Python 局部形狀鏡像 | 保留 Schema 權威與 typed 解碼；`test_author_schema_drift.py::test_schema_alignment_detects_an_open_object` 驗證多餘欄位不會被放過；本文件的 consumer 案例驗證實際接受集合 | 只有等價局部形狀可改由同一封閉型別生成／共用；本次不刪 |
| consumer 重複辨識同一靜態合法形狀 | 由唯一 backend admission/compiler 承接，計畫綁定受驗 immutable IR 與定義；須補 raw IR bypass、未知讀值／能力、未授權來源的負例，再替換 consumer 的重猜 | 在承接計畫實際落地前不可刪；`WellFormedProgram` 不充當執行許可 |
| binding、Pay、跨 execution、alias | `test_author_bindings.py` 的 `test_local_failures`、`test_branch_local_and_unsupported_join`、`test_new_execution_cannot_capture_local`、`test_unused_alias_cycles_are_rejected`；Rust `ir/tests/wellformed/decisions/payments.rs` 的 `zero_payment_and_every_alternative_are_checked` | 不交給局部 Schema；新 walker 只承接結構走訪，scope／dominance 政策保留 |
| port 與來源閉包 | `validate_author_ports` 保留 export 種類、唯一性與 scope；`test_author_direct.py::test_direct_rejects_incomplete_or_invalid_cards`、`test_author_set.py::test_two_way_record_closure` 保護整卡／登錄閉包 | 不以行數或來源欄位存在取代；本次不刪 |
| raw IR producer 與動態授權 | Rust `ir/tests/wellformed/decisions/producers.rs` 的 `token_inputs_are_checked_before_output_and_siblings_share_no_outputs`；runtime 的付款／世代／揭露負例仍由所屬 consumer 維護 | 靜態計畫不得取代動態重驗；本責任表不授權刪除 |

表中既有測試提供責任的反例入口，並不代表它們足以證明任意未來刪除等價。
新增語法時同步修改 Schema、Python 對應型別及接受集合測試；
僅接通既有語法時，保留前兩層並在 Rust 依所缺責任補實作與測試。
