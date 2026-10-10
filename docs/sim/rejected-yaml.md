# 載入期拒絕的 YAML

`Catalog` 載入 `authored/` 時，除了 JSON Schema，還會用 `sim/engine/src/catalog/semantics.rs`
檢查每張卡。不通過的卡記在 `Catalog::rejections()`（`檔案:行: 訊息`），**永不執行**：
任何用到它的操作都回 `Unsupported("card program rejected at load: …")`。

```bash
cargo run --locked -p sve-engine --bin sve-prototype -- "$SVE_TEST_SNAPSHOT" . validate target/validate
```

機器可讀清單在 [rejected-yaml.yaml](../../tests/engine/rejected-yaml.yaml)；共用測試
`authored_yaml_loads_or_is_a_listed_rejection` 要求實際結果與清單完全相同（多一張、少一張都失敗）。

## 檢查項目

| 項目 | 拒絕的情況 |
| --- | --- |
| opcode | 該位置沒有實作（本文、靜態能力、費用、構築各有清單），含 `unsupported`、`declare_name`、`swap` |
| 誘發事件 | 引擎不會發出的事件名；階段事件帶 `subject`、非階段事件帶 `side`；非誘發能力帶 `event`／`trigger_if` 等；`trigger_if` 讀事件限定（`event.*`、`is_active`）以外的值（R-0009：長寫條件寫在本文） |
| read 路徑 | 未知的回合計數、物件欄位、玩家屬性、參照；`event.*` 讀了該事件沒有的欄位或在誘發以外讀；`event.subject.*` 不是事件快照實際有的鍵；任何 `event.target.*`；`x` 沒有 `variables.x`；`paid.*` 在本卡沒有對應的追加費用；`choice.*` 在 `replace_choice` 以外；`damage.amount` 在 `replace_damage` 以外；`item.*` 在選擇器 `where` 以外 |
| 選擇器與參照 | 未知的 `type`、`keyword`、`ability_event`；參照既不是內建也不是本卡綁定的名稱；`target.<key>`／`cost.<key>` 的 key 不是本卡任何選擇的 key；`different_from` 指向不存在的選擇；`create.name`、`pilot.by` 內的讀值與參照 |
| 期間 | `until`、`during` 沒有到期處理的值 |
| 函式參數 | `fn` 的參數個數不符（add 等二元 2 個、not 等 1 個、min／max 1 或 2 個、and／or 1 個以上；引擎原本會靜默丟掉多的參數） |
| 其他參數 | `restrict` 沒有程序檢查的動作、未實作的抑制事件、非計數器的 `counter` 名稱、`draw`／`look` 的 `up_to`、本文中的 `modify.type／traits／cost／set_cost`、未知的 `distinct_by`、`damage.split` 沒有對應帶 `distribute` 的選擇、`replace_move` 的 `from`／`replacement` 區域、`move … to: banish`（要寫 `op: banish`）、從者／護符的 `name_alias` 不是 `while_zone: field`（CR 10.3.5、Q424） |
| 指稱 | 同一行、同類（誘發／宣告）且內容不同的能力沒有各自的 keyword（契約 3.1） |
| 行號 | 每個拒絕都要能定位到卡內的行（共用測試要求行號大於 0） |

尚待確認的限制與修復進度由 [#478](https://github.com/gbaian10/sve-kit/issues/478) 追蹤；載入通過不等於執行正確。
