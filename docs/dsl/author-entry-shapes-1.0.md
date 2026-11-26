# DSL 1.0 入口容器

主檔與具名本體共用 `author-nodes.schema.json` 的能力內容。存放與索引依[承載契約](author-contract-1.0.md)。

## 容器矩陣

| 入口         | 必填                                        | 選填與配置                                                                     |
| ------------ | ------------------------------------------- | ------------------------------------------------------------------------------ |
| 主檔         | format: 2、kind: author_effect_set、cards   | scope 默認 shared，dsl 默認 1.0                                                |
| 卡片         | abilities                                   | meta、admission、note、deck_rule、rule_sections、ignored、token、token_sources |
| inline 能力  | line、恰一個能力頭                          | face、section、scope、covers、admission                                        |
| 具名本體用途 | line、use                                   | params、edition、face、section、scope、covers、admission                       |
| 本體分片     | format: 2、kind: effect_body_shard、records | scope 默認 shared，dsl 默認 1.0                                                |
| 具名本體     | id、body、written_by                        | admission、edition、params、ports、note                                        |

具名本體的作者及逐卡審查紀錄使用實際參與者和日期。具名本體的 id 與版次在全庫唯一。作者索引雙向覆蓋主檔，卡號對應永久 CardId。
同一永久卡片的印刷別名共享同一份材料化程式。

## 診斷與驗證範圍

讀取器驗證封閉語法、唯一鍵與必要欄位。診斷保存 stage、reason、卡號、DSL 路徑與可得的 YAML 位置。
rule lines 與具名本體的查詢可使用[作者工具](authoring-tools.md)。

## 展開與 D0 消費邊界

具名本體依 id 與 edition 選取。實值與宣告必須完整對應，具名本體的 `param` 葉依用途替換。
`expanded` profile 以 dynamic anchors 排除殘留的 `param` 與 `generated_by`。

材料化的 `author_effect_set` 包含永久 FaceId 分組的 abilities，每項能力有且只有一筆 `DirectSourceUse`。
座標包含永久 face、revision owner、field、ordinal、line、scope 與 covers。可忽略行與衍生物段落也占用明示的座標。
D0 對照完整行清單，核對卡片身分、面與版次，並降低引擎支援的能力。引擎在建立對局時再驗整卡支援。
