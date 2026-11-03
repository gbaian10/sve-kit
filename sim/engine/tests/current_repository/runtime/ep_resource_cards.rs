use super::*;

/// Every JP printing of the admitted EP/SEP cards. Opening EP/SEP, EP substitution and
/// super evolution are native rule procedures, so each card selects with no ability.
fn printings() -> Vec<String> {
    let other = [
        "PR-266",
        "PR-417",
        "PR-445",
        "PR-566",
        "PR-567",
        "PR-568",
        "PR-593",
        "PR-594",
        "PR-404",
        "PR-469",
        "PR-563",
        "SCS01-010",
        "BP12-EP",
        "PCS01-EP",
        "BP18-SEP",
    ];
    (1..=49)
        .map(|n| format!("CP01-T{n:02}"))
        .chain((1..=10).map(|n| format!("CP03-T{n:02}")))
        .chain(other.map(String::from))
        .collect()
}

#[test]
fn ep_and_sep_printings_select_without_card_abilities() {
    let selected = selection();
    for printing in printings() {
        assert!(
            selected.current.contains_key(&printing),
            "{printing} is in the current selection"
        );
        assert!(
            selected.current[&printing]
                .program
                .card()
                .abilities
                .is_empty(),
            "{printing} keeps the EP/SEP rule reminder out of card abilities"
        );
    }
}
