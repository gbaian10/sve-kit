#![expect(
    clippy::panic,
    reason = "Unexpected decisions invalidate the scenario."
)]
use super::*;

const PLAYER: PlayerId = PlayerId { seat: 1 };

fn definition(number: &str, line: u32) -> AbilityId {
    selection().current[number]
        .program
        .card()
        .abilities
        .iter()
        .find(|ability| ability.source.line == line)
        .unwrap()
        .node
        .id
}

fn selected(game: &mut Game, play: CardPlay, expected: &[&str], choose: &str) {
    let CardPlay::Awaiting(question) = play else {
        panic!("target choice");
    };
    let InputDomain::Targets(domain) = &question.domain else {
        panic!("targets");
    };
    let ids: Vec<_> = domain
        .members
        .iter()
        .map(|member| member.card.as_deref().unwrap())
        .collect();
    assert_eq!(ids, expected, "only legal holders are offered");
    let handle = domain
        .members
        .iter()
        .find(|member| member.card.as_deref() == Some(choose))
        .unwrap()
        .handle;
    assert!(
        matches!(
            game.answer_internal_card(
                question.actor,
                question.token,
                Answer::Targets(vec![handle])
            )
            .unwrap(),
            CardPlay::Resolved(_)
        ),
        "the selected counter effect resolves"
    );
}

#[test]
fn feria_and_all_printings_add_only_to_another_selected_fairy_tale_follower() {
    for number in ["BP03-114", "BP03-P32", "SP01-053", "SP01-SL53"] {
        let mut initial = draw_position();
        initial["players"]["P1"]["zones"]["field"] = json!([
            {"id":"source","card":number},
            {"id":"ally","card":"BP03-004"},
            {"id":"wrong-trait","card":"BP01-037"}
        ]);
        initial["players"]["P1"]["zones"]["ex"] = json!([{"id":"ex","card":"BP03-004"}]);
        initial["players"]["P2"]["zones"]["field"] = json!([{"id":"opponent","card":"BP03-004"}]);
        let mut game = game(&initial);
        let play = game
            .activate_internal_card("source", definition(number, 1), PLAYER)
            .unwrap();
        selected(&mut game, play, &["BP03-004"], "BP03-004");
        let view = game.projection(View::Referee).unwrap();
        assert_eq!(view["objects"]["ally"]["counters"]["童話カウンター"], 1_u8);
        assert_eq!(view["objects"]["source"]["acted"], true);
        for id in ["source", "wrong-trait", "ex", "opponent"] {
            assert!(
                view["objects"][id]["counters"]["童話カウンター"].is_null(),
                "unselected objects keep their counters"
            );
        }
    }
}

#[test]
fn feria_cannot_select_itself_or_pay_without_another_legal_holder() {
    let mut initial = draw_position();
    initial["players"]["P1"]["zones"]["field"] = json!([{"id":"source","card":"BP03-114"}]);
    let mut game = game(&initial);
    let before = game.projection(View::Referee).unwrap();
    game.activate_internal_card("source", definition("BP03-114", 1), PLAYER)
        .expect_err("no legal other holder exists");
    assert!(
        game.projection(View::Referee).unwrap() == before,
        "an illegal activation preserves state"
    );
}

// A synthetic named amulet isolates this spell without admitting Gigablade's unrelated triggers.
fn blade_game(initial: &Value) -> Game {
    static SELECTED: OnceLock<SelectedEffects> = OnceLock::new();
    let selected = SELECTED.get_or_init(|| {
        let (mut snapshot, keywords, mut input) = inputs().clone();
        if !snapshot.ends_with('\n') {
            snapshot.push('\n');
        }
        snapshot.push_str(
            &json!({"number":"counter-blade","faces":[{
                "name":"ギガブレード","card_class":"ロイヤル","card_type":"アミュレット",
                "traits":[],"cost":"1","power":"-","hp":"-","text":null,"sections":[]
            }]})
            .to_string(),
        );
        input["documents"].as_array_mut().unwrap().push(json!([
            "counter-blade.yaml",
            json!({"format":1_u8,"kind":"effect_set","cards":{
                "counter-blade":{"status":"complete","abilities":[]}
            }})
            .to_string()
        ]));
        repository::select(&snapshot, &keywords, &input).unwrap()
    });
    let mut game = Game::from_selected_effects(
        selected.clone(),
        initial,
        &Value::Null,
        &Value::Null,
        "counter-holders",
    )
    .unwrap();
    game.load_internal_card(
        "counter-blade",
        &WellFormedCardProgram::check(CardProgram {
            abilities: vec![],
            card_play_costs: vec![],
        })
        .unwrap(),
    )
    .unwrap();
    game
}

#[test]
fn unlimited_search_and_its_printing_add_to_every_own_field_gigablade_then_draw_two() {
    for number in ["BP18-P51", "BP18-T02"] {
        for copies in [0_u8, 2] {
            let mut initial = draw_position();
            initial["players"]["P1"]["zones"]["hand"] = json!([{"id":"source","card":number}]);
            let mut field = vec![json!({"id":"other","card":"BP01-037"})];
            for index in 0_u8..copies {
                field.push(json!({"id":format!("blade-{index}"),"card":"counter-blade","state":{"counters":{"ギガカウンター":2_u8}}}));
            }
            initial["players"]["P1"]["zones"]["field"] = json!(field);
            initial["players"]["P1"]["zones"]["ex"] = json!([{"id":"ex","card":"counter-blade"}]);
            initial["players"]["P2"]["zones"]["field"] =
                json!([{"id":"opponent","card":"counter-blade"}]);
            initial["players"]["P1"]["zones"]["deck"] =
                json!([{"id":"draw-1","card":"BP01-037"},{"id":"draw-2","card":"BP01-037"}]);
            let mut game = blade_game(&initial);
            assert!(
                matches!(
                    game.play_internal_card("source", PLAYER).unwrap(),
                    CardPlay::Resolved(_)
                ),
                "the spell resolves with zero or multiple holders"
            );
            let view = game.projection(View::Referee).unwrap();
            for index in 0_u8..copies {
                assert_eq!(
                    view["objects"][format!("blade-{index}")]["counters"]["ギガカウンター"],
                    5_u8
                );
            }
            for id in ["other", "ex", "opponent"] {
                assert!(
                    view["objects"][id]["counters"]["ギガカウンター"].is_null(),
                    "wrong name, zone and controller stay unchanged"
                );
            }
            assert_eq!(view["P1"]["hand"], json!(["draw-1", "draw-2"]));
        }
    }
}
