#![expect(
    clippy::panic,
    clippy::wildcard_enum_match_arm,
    reason = "Unexpected questions invalidate the printed exchange scenarios."
)]
use super::*;
use sve_engine::game::internal::{DecisionView, SearchAnswer};

fn question(play: CardPlay) -> DecisionView {
    let CardPlay::Awaiting(question) = play else {
        panic!("printed exchange decision")
    };
    question
}

fn finish(game: &mut Game, mut play: CardPlay, exchange: bool) {
    while let CardPlay::Awaiting(question) = play {
        let answer = match &question.domain {
            InputDomain::OptionalCost => Answer::OptionalCost(exchange),
            InputDomain::CostMaterials(domain) => Answer::CostMaterials(
                domain
                    .members
                    .iter()
                    .take(usize::try_from(domain.cardinality.minimum.0).unwrap())
                    .map(|member| member.handle)
                    .collect(),
            ),
            InputDomain::Targets(domain) => Answer::Targets(vec![domain.members[0].handle]),
            InputDomain::Search(domain) => {
                assert_eq!(domain.cards.len(), 1, "only the printed name matches");
                Answer::Search(SearchAnswer::Ungrouped(vec![domain.cards[0].handle]))
            }
            InputDomain::WardEntry { .. } => Answer::WardEntry(false),
            other => panic!("unexpected printed question {other:?}"),
        };
        play = game
            .answer_internal_card(question.actor, question.token, answer)
            .unwrap();
    }
    if !matches!(play, CardPlay::Entered(_)) {
        resolved(play);
    }
}

fn settle(game: &mut Game) {
    loop {
        event_cemetery::finish_checks(game);
        let Some(question) = game.internal_pending_choice().unwrap() else {
            break;
        };
        let InputDomain::Pending(options) = &question.domain else {
            panic!("pending")
        };
        let play = game
            .answer_internal_card(
                question.actor,
                question.token,
                Answer::Pending(options[0].handle),
            )
            .unwrap();
        finish(game, play, false);
    }
}

fn position(printing: &str, pp: u8) -> Value {
    let mut initial = draw_position();
    initial["players"]["P1"]["pp"] = json!({"current":pp,"max":10_u8});
    initial["players"]["P1"]["zones"]["hand"] = json!([{"id":"source","card":printing}]);
    initial["players"]["P2"]["zones"]["field"] =
        json!([{"id":"enemy","card":"BP01-171","state":{"hp":10_u8}}]);
    initial
}

#[test]
fn enforcer_all_printings_pay_normally_search_the_member_and_trigger_both_at_own_end() {
    for printing in ["BP19-098", "BP19-P26"] {
        let mut initial = position(printing, 5);
        initial["players"]["P1"]["zones"]["deck"] = json!([
            {"id":"found","card":"BP19-102"},{"id":"wrong","card":"BP01-171"}
        ]);
        let mut game = game(&initial);
        let play = game
            .play_internal_card("source", PlayerId { seat: 1 })
            .unwrap();
        finish(&mut game, play, false);
        settle(&mut game);
        let state = game.projection(View::Referee).unwrap();
        assert_eq!(state["P1"]["pp"]["current"], 0_u8);
        assert!(
            state["P1"]["field"]
                .as_array()
                .unwrap()
                .contains(&json!("found"))
        );
        assert_eq!(state["P1"]["deck"], json!(["wrong"]));
        game.internal_rule_action(&json!({"do":"end-phase"}))
            .unwrap();
        settle(&mut game);
        assert_eq!(
            game.projection(View::Referee).unwrap()["objects"]["enemy"]["hp"],
            7_u8
        );
    }
}

#[test]
fn member_all_printings_pay_normally_and_deal_one_at_own_end() {
    for printing in ["BP19-102", "BP19-P29"] {
        let mut game = game(&position(printing, 2));
        let play = game
            .play_internal_card("source", PlayerId { seat: 1 })
            .unwrap();
        finish(&mut game, play, false);
        settle(&mut game);
        assert_eq!(
            game.projection(View::Referee).unwrap()["P1"]["pp"]["current"],
            0_u8
        );
        game.internal_rule_action(&json!({"do":"end-phase"}))
            .unwrap();
        settle(&mut game);
        assert_eq!(
            game.projection(View::Referee).unwrap()["objects"]["enemy"]["hp"],
            9_u8
        );
    }
}

fn named_probe(initial: &Value) -> Game {
    static SELECTED: OnceLock<SelectedEffects> = OnceLock::new();
    let selected = SELECTED.get_or_init(|| {
        let (mut snapshot, keywords, mut input) = inputs().clone();
        // A synthetic name probe does not replace the unadmitted BP19-092 program.
        snapshot.push_str(
            &json!({"number":"exchange-light-elraade","faces":[{
            "name":"結盟の咎人・エルラーデ","card_class":"ニュートラル",
            "card_type":"フォロワー","traits":[],"cost":"6","power":"6","hp":"6",
            "text":null,"sections":[]}]})
            .to_string(),
        );
        snapshot.push('\n');
        input["documents"].as_array_mut().unwrap().push(json!([
            "exchange-light-elraade.yaml",
            json!({"format":1_u8,"kind":"effect_set",
                "cards":{"exchange-light-elraade":{"status":"complete","abilities":[]}}})
            .to_string()
        ]));
        repository::selection_with_rules_from(&snapshot, &keywords, &input)
    });
    let mut game = Game::from_selected_effects(
        selected.clone(),
        initial,
        &Value::Null,
        &Value::Null,
        "exchange-light-name-probe",
    )
    .unwrap();
    game.load_internal_card(
        "exchange-light-elraade",
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
fn named_zero_cost_exchange_acts_only_an_unspent_matching_probe() {
    let mut initial = position("BP19-102", 0);
    initial["players"]["P1"]["zones"]["field"] = json!([
        {"id":"fee","card":"exchange-light-elraade"},
        {"id":"spent","card":"exchange-light-elraade","state":{"acted":true}},
        {"id":"wrong","card":"BP01-171"}
    ]);
    let mut game = named_probe(&initial);
    let accept = question(
        game.play_internal_card("source", PlayerId { seat: 1 })
            .unwrap(),
    );
    assert!(matches!(accept.domain, InputDomain::OptionalCost));
    let materials = question(
        game.answer_internal_card(accept.actor, accept.token, Answer::OptionalCost(true))
            .unwrap(),
    );
    let InputDomain::CostMaterials(domain) = &materials.domain else {
        panic!("materials")
    };
    assert_eq!(domain.members.len(), 1);
    let play = game
        .answer_internal_card(
            materials.actor,
            materials.token,
            Answer::CostMaterials(vec![domain.members[0].handle]),
        )
        .unwrap();
    finish(&mut game, play, true);
    let state = game.projection(View::Referee).unwrap();
    assert_eq!(state["objects"]["fee"]["acted"], true);
    assert_eq!(state["objects"]["wrong"]["acted"], false);
    assert_eq!(state["P1"]["pp"]["current"], 0_u8);
    for (material, acted) in [("exchange-light-elraade", true), ("BP01-171", false)] {
        initial["players"]["P1"]["zones"]["field"] =
            json!([{"id":"unavailable","card":material,"state":{"acted":acted}}]);
        let mut unavailable = named_probe(&initial);
        let before = unavailable.projection(View::Referee).unwrap();
        let rejected = question(
            unavailable
                .play_internal_card("source", PlayerId { seat: 1 })
                .unwrap(),
        );
        unavailable
            .answer_internal_card(rejected.actor, rejected.token, Answer::OptionalCost(true))
            .unwrap_err();
        assert_eq!(unavailable.projection(View::Referee).unwrap(), before);
    }
}

#[test]
fn zirnitra_all_printings_pay_exactly_one_namesake_then_create_draw_and_grant_storm() {
    for printing in ["BP09-054", "BP09-SL12"] {
        let mut initial = position(printing, 3);
        initial["players"]["P1"]["zones"]["field"] = json!([
            {"id":"fee-a","card":printing},{"id":"fee-b","card":printing},
            {"id":"dragon-a","card":"BP01-T11"},{"id":"dragon-b","card":"BP01-T11"}
        ]);
        initial["players"]["P1"]["zones"]["deck"] = json!(
            (0_u8..4)
                .map(|i| json!({"id":format!("draw{i}"),"card":"BP01-171"}))
                .collect::<Vec<_>>()
        );
        let mut game = game(&initial);
        let accept = question(
            game.play_internal_card("source", PlayerId { seat: 1 })
                .unwrap(),
        );
        let materials = question(
            game.answer_internal_card(accept.actor, accept.token, Answer::OptionalCost(true))
                .unwrap(),
        );
        let InputDomain::CostMaterials(domain) = &materials.domain else {
            panic!("materials")
        };
        assert_eq!(domain.members.len(), 2);
        let before = game.projection(View::Referee).unwrap();
        game.answer_internal_card(
            materials.actor,
            materials.token,
            Answer::CostMaterials(domain.members.iter().map(|member| member.handle).collect()),
        )
        .unwrap_err();
        assert_eq!(
            game.projection(View::Referee).unwrap(),
            before,
            "Q1255 cannot pay twice"
        );
        let play = game
            .answer_internal_card(
                materials.actor,
                materials.token,
                Answer::CostMaterials(vec![domain.members[0].handle]),
            )
            .unwrap();
        finish(&mut game, play, true);
        settle(&mut game);
        let state = game.projection(View::Referee).unwrap();
        assert_eq!(projected_zone_count(&state["P1"]["field"]), 5);
        assert_eq!(projected_zone_count(&state["P1"]["cemetery"]), 1);
        assert_eq!(projected_zone_count(&state["P1"]["hand"]), 3);
        assert_eq!(projected_zone_count(&state["P1"]["deck"]), 1);
        assert_eq!(state["P1"]["pp"]["current"], 0_u8);
        let dragons = state["P1"]["field"]
            .as_array()
            .unwrap()
            .iter()
            .filter_map(Value::as_str)
            .filter(|id| state["objects"][*id]["card"] == "BP01-T11")
            .collect::<Vec<_>>();
        assert_eq!(dragons.len(), 3);
        for dragon in dragons {
            assert!(
                state["objects"][dragon]["keywords"]
                    .as_array()
                    .unwrap()
                    .contains(&json!("疾走"))
            );
        }
    }
}

#[test]
fn crimson_all_printings_combine_two_original_cost_two_materials_with_yuzuki_discount() {
    for printing in ["BP15-087", "PR-413"] {
        let mut initial = position(printing, 0);
        initial["players"]["P1"]["zones"]["field"] = json!([
            {"id":"yuzuki","card":"BP15-080"},{"id":"fee","card":"BP05-046"}
        ]);
        let mut game = game(&initial);
        let play = game
            .play_internal_card("source", PlayerId { seat: 1 })
            .unwrap();
        finish(&mut game, play, true);
        settle(&mut game);
        let state = game.projection(View::Referee).unwrap();
        assert_eq!(state["P1"]["pp"]["current"], 0_u8);
        assert_eq!(state["objects"]["yuzuki"]["acted"], true);
        assert_eq!(state["objects"]["fee"]["acted"], true);
        assert_eq!(state["objects"]["enemy"]["hp"], 6_u8);
    }
}

#[test]
fn advance_all_printings_act_a_soldier_damage_and_create_a_knight_but_require_an_enemy() {
    for printing in ["BP21-035", "BP21-P20"] {
        let mut initial = position(printing, 1);
        initial["players"]["P1"]["zones"]["field"] = json!([{"id":"fee","card":"BP13-028"}]);
        let mut game = game(&initial);
        let play = game
            .play_internal_card("source", PlayerId { seat: 1 })
            .unwrap();
        finish(&mut game, play, true);
        settle(&mut game);
        let state = game.projection(View::Referee).unwrap();
        assert_eq!(state["objects"]["fee"]["acted"], true);
        assert_eq!(state["objects"]["enemy"]["hp"], 6_u8);
        assert_eq!(projected_zone_count(&state["P1"]["field"]), 2);
        assert_eq!(state["P1"]["pp"]["current"], 0_u8);
        initial["players"]["P1"]["pp"]["current"] = json!(2_u8);
        initial["players"]["P2"]["zones"]["field"] = json!([]);
        for accept_exchange in [true, false] {
            let mut no_target = super::game(&initial);
            let before = no_target.projection(View::Referee).unwrap();
            let accept = question(
                no_target
                    .play_internal_card("source", PlayerId { seat: 1 })
                    .unwrap(),
            );
            no_target
                .answer_internal_card(
                    accept.actor,
                    accept.token,
                    Answer::OptionalCost(accept_exchange),
                )
                .unwrap_err();
            assert_eq!(
                no_target.projection(View::Referee).unwrap(),
                before,
                "Q2721 preserves payment and creates no Knight"
            );
        }
    }
}
