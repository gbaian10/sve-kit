# 以適正打出的單一事實記錄 UB

Q2391 要求內層 UB 計入尚在結算的外層 UB，Q2390 又只排除目前執行，按完成日誌或卡名計數都會出錯。UB 作為適正打出 E.play 的 facet 記錄，共用 OriginFactId 並以 execution 排除本次，不另建立 InFlight／Completed 生命週期；依相鄰且無檢查時點的打出與結算程序作此類推，可避免聯合執行堆疊查詢及完成後重計。這是 generalized 的模型解讀，詳細事件、拒付與巢狀驗收見 [IR §13 GAP-S-015](../dsl/ir-vocabulary-1.0.md#gap-s-015)。
