# 輸入點依規則階段保留

唯一合法答案不代表規則程序可以略過，依私有候選數量自動省掉輸入也可能洩漏資訊。決定依 PlayParameter、ResolutionChoice、RuleChoice、QuickWindow 分型，並用共同判定程序保留已開始指示的選擇，包括唯一答案、空集合或拒絕；代價是部分操作仍需輸入，但能維持付款、觀察與重播的相同邊界。具體例外及判定順序見 [IR §2](../dsl/ir-vocabulary-1.0.md#2-決定precondition-與輸入點)。
