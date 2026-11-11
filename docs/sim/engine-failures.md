# 引擎執行失敗

`EngineFailure` 是權威端的診斷，訊息可包含內部或私有狀態，不是玩家回應。
分類依失敗的契約判斷；不能因訊息含有 private、hidden、stale 等字詞，就刪除檢查、
放寬授權、改用目前物件或回傳零。

## 內部分類

| 種類 | 責任 | 已分類的例子 |
| --- | --- | --- |
| `Unsupported` | 引擎尚無所需的語法或規則能力 | 本文 Resource 非 literal；未知的 OptionalCost 形狀；缺少離場 counter 或 Look 成員的快照 |
| `Invalid` | 資料、存檔或請求無效 | 既有不合法的 counter 資料；回答格式錯誤 |
| `IllegalPlay` | 正式 play 無法完成必要目標或完整付款 | counter 不足以完成全部付款 |
| `AuthorizationIntegrity` | 執行時所讀資訊或引用未取得所需授權 | 他人的 Look；未在同一次結算揭露就作公開讀值 |
| `Invariant` | 權威內部不變量或引擎數值表示範圍失效 | EP 增加超過整數表示範圍 |

分類是逐呼叫點確認的，既有 `Unsupported` 尚有未遷移的動態拒絕，不能把它們全部當成
可放寬的能力缺口。`Invalid` 與 `IllegalPlay` 的既有處理語義保留；例如 counter 溢位
目前仍為 `Invalid`，不得為了分類一致而改變拒答或整次回滾的邊界。

最後已知資訊尚不可讀時，保留 `Unsupported` 與原來的失敗行為；這不授權讀取新世代。
事件主體與公開 Look 的世代檢查也不能因分類而省略。

## 玩家結果

[玩家輸入](player-input.md)只提供不帶原因的 `SubmissionKind` 與 `Input`。
內部分類不直接傳給玩家，也不新增可區分私有狀態的錯誤碼。

| 提交階段 | 內部失敗 | 安全結果 |
| --- | --- | --- |
| Answer 驗證前 | `Invalid`、`IllegalPlay` | `Rejected`，保留題目 |
| Answer 已驗證 | `IllegalPlay` | `Reverted`，沿既有整次 play 回滾並撤銷舊題 token |
| Answer 已驗證 | 其他種類 | `EngineFault`，沿既有回滾範圍停止推進 |
| 主行動、開局、收尾的提交驗證 | `Invalid` | `Rejected` |
| 主行動、開局、收尾的提交驗證 | 其他種類（含 `IllegalPlay`） | `EngineFault` |

Answer 驗證前的 `Unsupported`、`AuthorizationIntegrity` 與 `Invariant` 也走故障。
玩家不能用內部原因決定重試、降級讀取或略過付款；引擎故障不算規則敗北。

## 工具與遷移

可信工具直接比對 `EngineFailure` 的種類，不解析 `Display` 字串。現有 scenario-runner
adapter 只把 `Unsupported` 送成工具的 unsupported 結果；完整性與不變量失敗送成
adapter error，避免被統計成尚未實作的能力。工具診斷不作玩家封包。
目前沒有正式的 typed 執行重掃器；此分類不代表已完成逐卡重掃或擴大卡片准入。

遷移一個 guard 前要核對整條呼叫鏈：

- 保留原來的動態重驗、回滾範圍與所有負例；不能放寬既有斷言。
- 檢查呼叫者是否依 `Unsupported` 做替代處理，尤其 `card_names` 的無名稱退回及舊
  projection 的錯誤遮蔽；換種類不能意外改成繼續讀取或失去原先的退回。
- 分清缺少合法規則所需的快照、未授權的讀取與權威內部故障；不以錯誤訊息決定數域政策。
- 以代表情境驗證錯答、整次回滾、故障停止及私有原因不外洩，不為每句訊息複製測試。
