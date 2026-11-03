# 構築索引

構築索引是從一份已驗證的卡表快照投影出來、每地區一份的小檔，只放牌組驗證器需要的事實。
規則與邏輯欄位見[牌組構築檢查規格](../../sim/deck-construction.md)；本文只定傳輸形狀與投影來源。
對戰大廳每請求的 CPU 有限，不能每次解析整份快照，所以另產這份（[#1597](https://github.com/gbaian10/sve-kit/issues/1597) 2026-11-03 留言）。

## 1. 產出

`export-offline` 寫完快照後一併產生；既有匯出根可用下面的指令補產。
快照缺少判斷所需的事實（例如沒跑機制 adapter，見 §3）時，`export-offline` 不寫索引、在輸出報告註明
`construction_index: skipped` 與原因；`construction-index` 指令則直接失敗。沒有索引的快照不能發布（publish 要求每地區的索引都在）。

```bash
uv --directory carddb run sve-carddb snapshot construction-index --export-dir <匯出根>
```

- 讀匯出根的 `snapshots/preview/current.json`，跟 publish 用同一套閉包驗證
- 寫在同一個匯出根的 `construction/<manifest hex>/<region>.json`（manifest hex 是 `manifest_sha256` 去掉 `sha256:`）；
  讀端用已載入快照的 manifest hash 組出路徑，快照清單本身不列這些檔，傳輸格式不變（維護者 2026-11-03 決定）
- 檔案是 canonical-json-v1 bytes；指令印出每個檔的路徑、bytes 與 SHA-256；`construction` 目錄是連結時拒絕寫入
- 綁定：檔內 `snapshot.data_version`、`snapshot.manifest_sha256` 指向來源快照；同一場對局固定快照與索引，兩邊用這兩個值核對
- 本契約的範圍是本機產出

## 2. 形狀（`construction_index_format: 1`）

| 欄位 | 內容 |
| --- | --- |
| `region` | `jp` 或 `en` |
| `snapshot` | `{data_version, manifest_sha256}` |
| `classes`、`titles`、`zones` | 字典：代碼字串的清單，卡片欄位存索引位置 |
| `title_rules` | 作品名代碼 → 規則代號（目前只有 `cardfight_vanguard` → `vanguard`） |
| `names` | 構築名稱的個數；構築名稱只以整數代號 0～`names`−1 出現，不帶名稱文字 |
| `cards` | 欄式：`id`、`name`、`class`、`titles`、`zone` 五個等長陣列，依卡片 ID 排序 |
| `copies` | `[卡片位置, "main" 或 "evolve", 上限]` |
| `singleton_except` | `[卡片位置, [不受影響的構築名稱代號…]]` |
| `start_amulet` | 有【スタートアミュレット】的卡片位置 |
| `formats` | 賽制代碼 → `{coverage: [[from, until 或 null]], restrictions: [...]}` |

- `cards.class`：`classes` 的位置；`-1` 表示沒有職業，只出現在 EP 等可放區 `none` 的卡
- `cards.titles`：位元遮罩，第 i 位是 `titles[i]`
- `cards.zone`：`zones` 的位置（`none`、`leader`、`main`、`evolve`）
- 卡片位置是 `cards` 陣列的索引
- 構築名稱代號是不透明的分組鍵（[規格 §1.2](../../sim/deck-construction.md#12-構築索引邏輯形)）：只比較相同與否，
  代號在不同快照之間不保證相同；違規的 `values.name` 就是這個代號，介面顯示名稱時用 `cards` 第一張卡在快照裡的卡名
- `restrictions[]`：`{id, kind, from, until, max, max_groups, members: [[構築名稱代號, scope, option]]}`；
  `kind` 是 `copy_limit` 或 `choice_group`，日期是 `YYYY-MM-DD`，`until` 不含當天

## 3. 投影來源

| 欄位 | 來源 |
| --- | --- |
| 收錄的卡 | 該地區 `release_state=released` 的卡；其餘卡驗證時是 `unknownCard` |
| `name` | 第 1 面在該地區 `role=primary` 的 `face_rules_name`（CR 6.1.1.4.1）；不是恰好一個就停止 |
| `class`、`titles` | 第 1 面該地區的 current `face_revision` |
| `zone` | 該地區的 `deck_role`：`leader`、`main`、`evolve` 照用，`extra` 與缺值是 `none` |
| `start_amulet` | `card_mechanic_coverage.printed_mechanisms` 有 `starting_amulet`；JP 地區覆寫優先於 shared，EN 只用自己的 `en_override` |
| `copies`、`singleton_except` | 第 1 面 current 效果文與 sections 的固定句型，見 §4 |
| `formats` | 該地區的 `rules_profile`；`coverage` 取完整且不晚於 `manifest.data_as_of` 的日期，扣除 partial 與 announced 限制的區間；`restrictions` 取 `confirmed` 列 |

`announced`、`withdrawn` 的禁限不投影；其他狀態或種類停止匯出，不猜。
`coverage` 的有限終點最多是 `data_as_of` 的隔日（不含當天）；開放終點也不能代表未來完整。
主牌組／進化牌組可放卡的 `starting_amulet` 無正標籤時，必須在 `checked_mechanisms` 中才能判為沒有；
缺少紀錄或尚未檢查就停止匯出，不能把未知當成沒有。不可放入牌組的卡與主戰者不要求這項否定證據。

## 4. 本文另定張數的句型

卡片本文會改構築條件（CR 6.1.2），但快照沒有結構化欄位，所以匯出端比對固定句型：

| 地區 | 句型 | 結果 |
| --- | --- | --- |
| jp | 『（自己的卡名）』は自分のデッキに N 枚まで入れることができる。 | `copies` main N |
| jp | これはエボルヴデッキに N 枚まで入れることができる。 | `copies` evolve N |
| jp | カード名に『X』を含まないカードは、メインデッキやエボルヴデッキにそれぞれ1枚までしか入れられない。 | `singleton_except`：名稱含 X 的構築名稱 |
| en | You can put up to N of this card into your deck. | `copies` main N |
| en | You can put up to N of this card into your evolve deck. | `copies` evolve N |
| en | Your main deck and evolve deck may only contain 1 copy of any card without "X" in its name. | `singleton_except` |

- 一行看起來像張數規則（jp 含「枚まで入れ」「枚までしか入れ」；en 含「up to N of this card」「may only contain N cop」）卻對不上句型時，停止匯出；新卡出現新寫法要先改這裡
- `copies` 的區要和卡的可放區相同；同一構築名稱同一區的上限不一致時停止匯出
- 2026-11-02 開發匯出的結果：jp、en 各 9 張 `copies`（進化牌組 10 張 7 張、ラピッドファイア 6、オニオン軍団 50）、1 張 `singleton_except`（カットスロート）、8 張【スタートアミュレット】

## 5. 大小

2026-11-02 開發匯出（`data_version` preview 29b）：

| 地區 | 卡數 | 構築名稱 | raw | gzip -9 | Brotli 11 |
| --- | ---: | ---: | ---: | ---: | ---: |
| jp | 3,832 | 3,105 | 185,946 B | 89,077 B | 71,203 B |
| en | 3,694 | 2,983 | 179,267 B | 85,757 B | 69,307 B |

大部分是 32 位十六進位的卡片 ID（約 14 萬 bytes raw，難以壓縮）。
