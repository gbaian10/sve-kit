# rules-scenarios：規則考題

模擬器規則引擎（DSL）的驗收題。每題是一個 YAML，描述初始局面、玩家的決定序列與預期結果；
格式與判定規則見 [CONTRACT.md](CONTRACT.md)（`sve-exam/2.1`）。

## 目錄

| 路徑                     | 內容                                                               |
| ------------------------ | ------------------------------------------------------------------ |
| `questions/`             | **定案題庫**：237 題、708 個局面。實作以這裡為準                   |
| `CONTRACT.md`            | 題目格式契約 v2.1（凍結）加第 10 節釐清                            |
| `validate.py`            | 格式檢查器：只查結構與引用，不判斷預期結果對不對                   |
| `card-facts-verified.md` | 用官方卡圖確認過的卡片事實（觸發圖示），題目的 `card_facts` 引用它 |

`questions/` 最初由兩位作者的原稿合併而成，之後再補充題目。兩人都出過的 24 題合併成一份，
局面名稱加上 `[astra]`／`[fable]` 前綴，保留兩邊各自的局面。
重複原稿已從目前目錄移除；執行與驗證都以 `questions/` 的定案題庫為準。

## 作者

每題的 `author` 欄位記錄作者；兩人共同的題目寫成清單 `[astra, fable]`。

| 作者  | 模型              | 題數（`questions/`） |
| ----- | ----------------- | -------------------- |
| astra | Codex gpt-6-astra | 119                  |
| fable | Claude Fable 5.1  | 94                   |
| 共同  | 兩者              | 24                   |

所有題目都經過對方交叉審核，雙方同意後定案。

`verified_by[].note` 裡出現的 `xverify-*/recheck.md`、`last.md`、`split.md` 等檔名是設計期本機審核稿，
不隨 repo 發布。題目的公開依據以題目的 `refs`、`evidence` 與裁定登錄（`authored/rules/rulings/`）為準。

定案題庫中的官方規則、Q&A 與卡文引文用來說明預期結果，保留出處，且不在本專案授權內。
題庫自有部分與裁定登錄的授權依根目錄 [LICENSING.md](../../LICENSING.md) 的路徑表；
引用來源不因此獲得重新授權。

## 必過題

`must_pass: true` 的 223 題（659 個局面）是 DSL 必須通過的門檻；其餘 14 題（49 個局面）是參考題，不列入門檻。

## 檢查格式

需要 carddb 產出的 `cards.jsonl`（位於資料目錄 `$SVE_DATA_DIR/derived/jp/`）：

```bash
uv run tests/rules-scenarios/validate.py "$SVE_DATA_DIR/derived/jp/cards.jsonl" tests/rules-scenarios/questions
```
