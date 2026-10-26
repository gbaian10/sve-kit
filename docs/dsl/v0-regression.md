# v0 分類差異與手寫 DSL 回歸基線

本契約把「來源位置可對齊」「標註可以整理」與「效果可執行」分開。
`authored/rules/effect-reviews/<owner>.yaml` 保存可撤回的判讀草案；
`carddb/tests/fixtures/effect_baseline/<owner>.json` 保存審查時凍結的測試期望。
兩者都不是正式來源語義、裁定、共用 body 或翻譯採納。

## 來源與版本

- 分類來源是官方 SVE 日文卡表的 v0 擷取（2026-09-26 卡表輸入、2026-09-27 分類整理），
  沿用[分類來源契約](../schema/domains/effect-classifications.md)的逐文字版本與分類識別碼。
- 手寫效果基底為 commit `70050d752657b7de5ae98c3b2de3e2fd90b866d7` 的
  `authored/rules/effects/`；422 卡、802 個 abilities、780 個不同位置。
  BP07-P06 後續依來源更正 [#824](https://github.com/gbaian10/sve-kit/pull/824)
  （`77340d8e33c0134f3292b6f2c9bcf8cc61614034`）移動能力 2 的錨點；
  只更新該卡 AST 與位置，保留原 body、目標選取及 additional_costs。
- 正向位置片段的基準已改用 2026-10-25 建置的 `four-layer-jp-v8` 卡表
  （as-of 仍為 2026-10-06，data version `preview-20261025T051811Z-0001`），
  建置 SQLite SHA-256 為
  `ae40461ab23b4c4e41f61c731fe6212c92e305376da33275af782d31694e050f`，
  建置 revision 為 `d8bd3b7f86b3b823d703997a9ce2c54a8cac4c21`。
  JP 解析輸入批次為
  `sha256:2bf20b2ff9ae9e21be688dbf5a91d4cd7803fa0e253206ecbdcb55e334cbe9c4`
  （`official-jp-exact-v1`）。此基準只承載位置證據，不把新建置當作當年的判讀輸入。
  clone 者仍只讀 repo 內 fixture 執行基線測試，不需要原建置 DB。
- AST 與判讀仍保留原凍結證據：原位置片段來自 `four-layer-jp-v6`
  （as-of 2026-10-06，data version `preview-20261018T170627Z-0001`），
  SQLite SHA-256 為
  `1de5adc33023646bbc7ced36bb49bb1850fe54309fed29c246d060a40862bc53`。
  本次沒有重錄 `ast_sha256`、移動 ability 或修改分類版本、結論、理由、
  `candidate` 狀態及審核紀錄；BP07-P06 沿用上述已採納的錨點更正。
- v8 時，判讀草案的 frame／use 引用與測試基線的反向 `frame_ids` 已依正式映射換成
  `frame-v2` 身分（normalizer 為 `four-layer-jp-v8`）；對應凍結建置 SHA-256 為
  `9889d5ce18670019aaecc9f5e1ecb7aab59c1e222992f59a2bc80953933cd6ea`。
  v8 只因 BP01-P25 的完整 Lastword 全主戰者固定傷害語義改變一個 frame；
  其餘 frame 保號，草案依賴依正式 writer 映射重驗，未搬移審核核可。
  草案不保留 frame-v1 引用；分類輸入、結論與 `candidate` 狀態不變，不因此取得新審核核可。
  當時草案的 `basis` 指向此判讀建置，反向 `frame_ids` 與草案的引用維持不變；
  正向 `fragments` 則依上述新位置基準重算，不回填或搬移舊審核核可。
- 重算範圍為全部 422 卡、780 個正向位置及其 801 個非 layout 片段
  （`unclassified_positions` 不在本次正向片段重算範圍，維持不變）。
  JP printing、face、欄位、section、行號、`ability_indices`、來源全文 hash 與 span
  均逐項核對不變；20 個多能力位置中多出的 22 個 abilities，以及
  21 個多來源片段位置完整保留。
  780 個位置的 binding／use／frame 引用更新；其中三個抽牌位置
  （BP10-122、CP02-068、ECP01-002）涉及 [K-B1](https://github.com/gbaian10/sve-kit/pull/962)
  的來源語義變更，BP01-P25 涉及 [K-B2](https://github.com/gbaian10/sve-kit/pull/990)
  的來源語義變更。其餘沿版本與 [FrameId 改版](https://github.com/gbaian10/sve-kit/pull/977)
  映射換號；BP07-P06 的三個片段另反映 #824 清空錯植 section 後的 revision 身分。
  正式映射拒絕延續舊審核的語義變更，只用來核對 occurrence 的新位置證據，
  不作為等價或審核通過的證明。60 個反向位置與其分類、`covered_by` 保持不變。
- v9 採納 BP19-P32 的完整 Advance Fanfare 破壞來源後，只更新該卡正向位置的
  四個 frame／binding／use 引用葉；兩個 sibling binding、原文全文 hash、span、
  `ability_indices` 與全部 `ast_sha256` 保留。其餘正向位置仍以上述 v8 位置基準為證據。
  草案的 frame／use 引用與 `basis` 同步重驗為 `four-layer-jp-v9`，凍結候選 DB SHA-256 為
  `954cec76c925454f30236a0468bd87b71270b4d6e696f16402fac0a94df1dfec`；分類、結論、理由、`candidate` 狀態及既有審核歷史不改，
  不因來源 resolved 取得本體、整卡或執行資格。
- 正向位置連接只比較 JP printing、face、欄位、section 與行號，沒有證明歷史 v0
  卡文與目前來源完全相同。[分類文字版本驗證](../schema/domains/effect-classifications.md#分類文字版本)與本基線的位置證據不得互相取代。

官方文字的權利仍依 [LICENSING.md](../../LICENSING.md)；此基線不含圖片或語音。

## 判讀草案

每個分片固定 `format: 1`、`kind: effect_review_draft`、`status: candidate`，
`basis` 固定分類輸入、normalizer 與凍結建置指紋。分片 owner 只是存放位置，
跨包組不拆散；以該組最小卡號的 owner 存放，反向位置按自身卡包存放。

`groups` 中每筆包含：

| 欄位 | 意義 |
| --- | --- |
| `id` | 研究用 shape key；不是正式框架 ID，也不是語義等價的證明 |
| `priority` | 是否涉及控制、時點或 relation 的優先組 |
| `classifications` | 原 T 識別碼到 resolved Classification 的 SHA-256；包含註記雜湊與成員，來源改變時必須重新判讀 |
| `examples` | 每份舊標註的代表卡號，不代表所有異圖都要重審 |
| `frames`、`uses` | 凍結建置中的完整候選反查範圍，不當作已確認受影響清單 |
| `conclusion`、`rationale` | 下表的一種結論，以及具體差異與擬議處置 |
| `translation_review` | 中文重審的直接用途及條件用途，兩者分開 |

| 結論 | 可以做什麼 |
| --- | --- |
| 參數值差異 | 保留有型別的值；不同 Zone／Player 仍可能要求不同 lowering 行為 |
| 等價寫法 | 只在文中列明的展開義務下視為同一操作；不能把稀疏 IR 當完整指令清單 |
| 舊標註修正 | 保存修正方向，保留原分類與原 DSL；不是立即改寫來源 |
| 需來源變體 | 未明示變體／選取模式前不得合併；仍須由來源語義流程採納 |
| 尚未知 | 證據不足，保留差異，不預設等價或強制拆框架 |

凍結範圍包含 55 組，其中 10 組為優先組。另在 `handwritten_differences`
記錄兩組手寫差異：`card_nos` 與 `frame` 表明範圍，`classifications` 同樣綁 resolved hash，
`conclusion`、`rationale` 與 `translation_review` 說明處置，`scenario` 必須對應引擎邊界測試。
BP18 的兩卡共用同一 v0 分類，差異在手寫 body；其「舊標註修正」是 BP18-059 改用 placed 的手寫修正候選。`cost_timing` 的字串 `None`
沿用 v0 的「無費用時點」分類值，不是 YAML null。

這些結論評估的是標註差異，並未證明未來的共用 body 已保留所有行為。
特別是搜尋巨集必須保留展示、移動、洗牌；不能僅因所有版本都帶 `I.search` 就刪除這些動作。

## 正反向位置

基線依卡包保存每卡 AST 指紋及所有 abilities 所在位置。AST 指紋使用完整卡片的
JSON（UTF-8、鍵排序、無多餘空白、保留 Unicode）SHA-256；不抽掉數值、註記或欄位。
此處的指紋是審查輸入的變更警報，不是 DSL 語義識別碼。

每個正向位置保留 `ability_indices` 及全部 `fragments`，包括 binding ID、frame ID、
use ID、來源 hash 和 span。`face`、`section`、ability index 是零起算，`line` 沿手寫 DSL
從 1 起算。21 個多來源片段位置必須完整保留；後續組裝不能只取第一個 frame。
同位置的多個 abilities 也不能只保留一個。

`reverse_positions` 是沒有同位置 ability 的來源位置；每筆包含分類、原因及框架。
跨行涵蓋另附 `covered_by.ability` 與該 ability 內的 JSON pointer。
定位覆蓋不證明其 body 全面正確，尤其位置錯標仍需單獨修正。

| 分類 | 數量 | 處理 |
| --- | --- | --- |
| token | 29 | 引用 token 的欄位不複製成本卡效果；token 自身仍需獨立覆蓋 |
| 跨行涵蓋 | 16 | 指向 costs、additional_costs 或 body，模式本文須組裝 |
| 通用規則 | 4 | 食事／EP 等由一般規則處理，不建立虛構能力 |
| 說明 | 11 | 關鍵字、建構與裝備的說明文字，不重複執行 |
| 真正缺漏 | 0 | 本次未發現無承接的獨立能力；不代表 422 卡全部正確 |

「通用規則」及「說明」只區分責任，不代表引擎已實作；例如ドライブチェック與タイトル構築開局配置仍須獨立確認支援。

BP07-P06 的基線依來源更正 `x:d529a2a90bd757f2be41c721eb0eaf4b` 不含補充段落的位置。
原 v6 建置仍有 section 0 line 1／2；新位置基準已採納清空錯植 section 的更正，
不再需要刻意排除該兩行。三個 effect 片段改用新 revision 的 binding／use／frame，
其本文全文 hash 與 span 均不變；能力 2 仍位於 effect line 3，
測試以更正後全文核對 hash 及第三行範圍。

CSD01-T01 的兩行歸通用規則。其餘 15 個空 abilities 卡的主 effect
為空；16 個空 abilities 卡都保留在測試基線內，不算作執行成功案例。

## 可執行差異情境

[引擎邊界測試](../../sim/engine/tests/boundaries.rs)的下列情境使用公開 Game／Catalog
介面及合成卡面，無須私有卡表：

| 情境 | 檢查的語義 |
| --- | --- |
| `authored_ride_costs_share_one_payment_and_reject_missing_resources` | 讀 CP03-009、CP03-112、CSD03a-006 的資源費用宣告，顯式與隱式都只支付一次；無可用資源時不扣 PP、不改狀態 |
| `chosen_and_actual_movement_results_diverge_at_ex_capacity` | 選兩張但 EX 可容納零／一／兩張；chosen 計數保留原選取量，placed 只計實際移動量 |
| `original_bp18_bodies_keep_different_remainders_when_ex_is_full` | 直接讀 BP18-059／BP18-SL14 第一個 body；EX 滿時，兩者對剩餘牌的底置集合及最終牌庫順序不同 |

BP18 測試刻意保存目前兩份程式的不同結果，**不是核可兩份都符合卡文**。
修正草案使用 placed 計算剩餘集合及後續減費；替代移動是否屬於該結果仍須按來源與規則判讀。
費用責任則沿現有 ride lowering：一般規則補缺少的資源費用，已有宣告時不得重複支付。
其他現有 ride 邊界測試另涵蓋 EP、重複宣告、存讀檔及觀測重建。

```bash
uv --directory carddb run pytest -n 4 tests/domains/effects/test_regression_baseline.py
cargo test --locked -p sve-engine --test boundaries
```

Python 基線測試驗證 AST、位置、分類版本與跨行 pointer；Rust 情境才驗證上述具體行為。
兩者都不宣稱 780 個位置全部可執行，亦不改變原 `status: complete` 的歷史性質。

### 已遷移到 DSL 1.0 的卡

整卡切換到DSL 1.0 後，原型條目離開 `authored/rules/effects/`，其原 AST 原樣保存在
`carddb/tests/fixtures/effect_baseline/migrated/<卡號>.json`。這 14 個 fixture 目前仍供
Rust 共用情境測試還原已遷移卡的原型 AST。Python 不再斷言遷移前後等價；基線測試以作者索引
與 fixture 中的卡號排除已遷移卡，再檢查仍在原型目錄中的卡片 AST 指紋與位置。

舊 Rust 執行器的共享場景（`shared`、`cards`）只驗**歷史回歸**：測試 catalog 以明確命名的
輔助函式，在已通過 `Catalog::from_current_selection()` 的原型文件上還原這些保存的原 AST，
場景、期望值與已知失敗清單維持原樣。這個 catalog 不是 current 選用結果，也不讓遷移卡取得
runtime 資格。

真正的 current 選用另由 `current_repository` 測試直接驗證：選用結果的原型側不含已遷移卡，
DSL 檔走完整 D0 候選。歷史 fixture 不得填回選用結果，也不得當成 current 的 fallback。

## 採納與中文重審

目前 `required_uses` 為空：草案沒有修改正式來源語義或中文 render。
`conditional_uses` 只列需來源變體及尚未知組的用途，待具體採納有語義差異時再逐項比較；
它不是立即重譯清單。其他組若後來發現來源語義差異，同樣須建立具體用途清單。

只改 DSL、巨集展開或標註時，中文不重派。若修改來源語義、leaf 或 render，
依新舊輸出篩選實際用途；不能用所有 pending 框架、整個候選組的異圖數或完整版本重建量
代替人工重審量。需要變更正式框架時，交由來源語義的批次升版流程；需要提高裁定證據強度時，
交由裁定流程。這份草案不執行任一採納。

採納後更新對應草案及測試期望，保留 Git 中的舊版本與差異原因；不要用自動重錄所有
指紋的方式掩蓋新變化。未採納前可直接撤回此草案，原分類、原 DSL 與翻譯完全不變。

## 更新基線

先審查具體變更，再只更新受影響卡片與判讀；不得為消除失敗而批次重錄全部 hash。

- 只用 repo 可重算 `ast_sha256`（依上述完整卡片 canonical JSON 規則）及
  `ability_indices`（依 face／field／section／line 分組、保留零起算索引）。
  `classifications` 的 hash 則由分類 reader 的 resolved Classification 重算，並重新審查對應理由。
- `fragments` 的 binding／use／frame／source hash／span，以及反向 `frame_ids`，
  必須從契約記錄的凍結建置取得；若改用新建置，須一併更新 `basis`、來源版本及受影響範圍。
  缺少該輸入時保留失敗並取得凍結來源，不能清空片段或移除反向位置來使測試通過。
- ability 改位置時，同時檢查原位置與新位置的來源、正向／反向集合及 `covered_by`。
  原位置若是錯植或重複內容，先由來源更正流程處理；更正採納前不移動 ability，
  也不得從基線刪除原位置。更正採納且新位置片段驗證後，才移動 ability 並移除失效的反向位置。
- BP18-059 修正 body 時保留清楚的行為差異證據，再更新 AST 期望與原 body 情境的預期結果；
  不再把已修正的行為稱為已知缺陷。
