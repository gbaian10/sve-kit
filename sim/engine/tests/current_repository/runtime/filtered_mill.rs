use super::*;

fn prepared() -> &'static SelectedEffects {
    static PREPARED: OnceLock<SelectedEffects> = OnceLock::new();
    PREPARED.get_or_init(|| {
        let (mut snapshot, keywords, mut input) = inputs().clone();
        input["authors"].as_array_mut().unwrap().retain(|author| matches!(author["input"]["card_no"].as_str(), Some("ECP01-045" | "CP02-T01" | "BP03-T02")));
        for (number, traits) in [("mill-horse", json!(["ウマ娘"])), ("mill-neutral", json!([]))] {
            snapshot.push_str(&json!({"number":number,"faces":[{"name":number,"card_class":"ニュートラル","card_type":"フォロワー","traits":traits,"cost":"1","power":"1","hp":"30","text":null,"sections":[]}]}).to_string());
            snapshot.push('\n');
            input["documents"].as_array_mut().unwrap().push(json!([format!("{number}.yaml"),json!({"format":1_u8,"kind":"effect_set","cards":{number:{"status":"complete","abilities":[]}}}).to_string()]));
        }
        repository::selection_with_rules_from(&snapshot, &keywords, &input)
    })
}

fn game(initial: &Value) -> Game {
    let mut board = Game::from_selected_effects(
        prepared().clone(),
        initial,
        &Value::Null,
        &Value::Null,
        "filtered-mill",
    )
    .unwrap();
    let blank = WellFormedCardProgram::check(CardProgram {
        abilities: vec![],
        card_play_costs: vec![],
    })
    .unwrap();
    for number in ["mill-horse", "mill-neutral"] {
        board.load_internal_card(number, &blank).unwrap();
    }
    board
}

#[test]
fn ignition_both_printings_damage_every_follower_by_the_actual_milled_horses() {
    for printing in ["ECP01-045", "ECP01-P25"] {
        for library in [
            vec!["mill-neutral"; 4],
            vec!["mill-horse", "mill-neutral", "mill-horse", "mill-neutral"],
            vec!["mill-horse"; 4],
            vec!["mill-horse", "mill-horse"],
            vec![],
        ] {
            let horses = library.iter().filter(|card| **card == "mill-horse").count();
            let mut initial = draw_position();
            initial["players"]["P1"]["zones"]["cemetery"] =
                json!([{ "id":"old", "card":"mill-horse" }]);
            initial["players"]["P1"]["zones"]["hand"] = json!([{"id":"source","card":printing}]);
            initial["players"]["P1"]["zones"]["deck"] = Value::Array(
                library
                    .iter()
                    .enumerate()
                    .map(|(index, card)| json!({"id":format!("d{index}"),"card":card}))
                    .collect(),
            );
            initial["players"]["P1"]["zones"]["field"] =
                json!([{"id":"own","card":"mill-neutral"}]);
            initial["players"]["P2"]["zones"]["field"] =
                json!([{"id":"opponent","card":"mill-horse"}]);
            let mut board = game(&initial);
            resolved(
                board
                    .play_internal_card("source", PlayerId { seat: 1 })
                    .unwrap(),
            );
            let state = board.projection(View::Referee).unwrap();
            assert_eq!(
                state["objects"]["own"]["damage"], horses,
                "all own followers take the actual horse count"
            );
            assert_eq!(
                state["objects"]["opponent"]["damage"], horses,
                "opposing followers take the same uniform amount"
            );
            assert_eq!(state["P1"]["leader"]["life"], 20_u8);
            assert_eq!(state["P2"]["leader"]["life"], 20_u8);
            assert_eq!(
                state["P1"]["pp"]["current"], 5_u8,
                "the real Spell pays its printed five PP"
            );
            assert!(
                state["P1"]["deck"].as_array().unwrap().is_empty(),
                "a short deck moves only the available cards"
            );
            let cemetery = state["P1"]["cemetery"].as_array().unwrap();
            assert!(
                cemetery.contains(&json!("old")),
                "preexisting cemetery cards remain but do not contribute to this movement receipt"
            );
            for (index, _) in library.iter().enumerate() {
                assert!(
                    cemetery.contains(&json!(format!("d{index}"))),
                    "every counted or uncounted top card really reaches the cemetery"
                );
            }
        }
    }
}
