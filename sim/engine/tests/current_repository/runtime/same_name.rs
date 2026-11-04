#![expect(
    clippy::panic,
    reason = "Unexpected choices invalidate the printed same-name scenarios."
)]
use super::event_cemetery::finish_checks;
use super::*;

fn finish(game: &mut Game, mut play: CardPlay) -> CardPlay {
    while let CardPlay::Awaiting(question) = play {
        let answer = match &question.domain {
            InputDomain::OptionalCost => Answer::OptionalCost(true),
            InputDomain::Targets(domain) => {
                assert_eq!(
                    domain.members.len(),
                    1,
                    "the printed target has one legal candidate"
                );
                Answer::Targets(vec![domain.members[0].handle])
            }
            InputDomain::CostMaterials(domain) => {
                assert_eq!(
                    domain.members.len(),
                    1,
                    "CR 10.4.3 limits an unspecified payment to the controller's other card"
                );
                Answer::CostMaterials(vec![domain.members[0].handle])
            }
            InputDomain::ResourceEntryPayment { .. }
            | InputDomain::WardEntry { .. }
            | InputDomain::Quick { .. }
            | InputDomain::BeforeDraw
            | InputDomain::Pending(_)
            | InputDomain::TriggerQuota(_)
            | InputDomain::Integer(_)
            | InputDomain::Order(_)
            | InputDomain::DamageReplacements(_)
            | InputDomain::DriveTrigger { .. }
            | InputDomain::Search(_)
            | InputDomain::SearchPartition(_)
            | InputDomain::Distribution(_)
            | InputDomain::SearchReplacement
            | InputDomain::TokenCapacity { .. }
            | InputDomain::Abilities(_)
            | InputDomain::Modes { .. } => panic!("unexpected question"),
        };
        play = game
            .answer_internal_card(question.actor, question.token, answer)
            .unwrap();
    }
    play
}
fn settle(game: &mut Game) {
    for _ in 0_u8..4 {
        finish_checks(game);
        let Some(question) = game.internal_pending_choice().unwrap() else {
            return;
        };
        let InputDomain::Pending(options) = &question.domain else {
            panic!("pending");
        };
        assert_eq!(
            options.len(),
            1,
            "each printed occurrence resolves before the next one"
        );
        let play = game
            .answer_internal_card(
                question.actor,
                question.token,
                Answer::Pending(options[0].handle),
            )
            .unwrap();
        resolved(finish(game, play));
    }
    panic!("trigger loop");
}
fn boosted_token(card: &str) -> Value {
    json!({"id":"chosen","card":card,"state":{"power":7_u8,"hp":7_u8,"max_hp":7_u8,"cost":9_u8,"counters":{"mark":4_u8}}})
}
fn created<'packet>(view: &'packet Value, zone: &str, old: &[&str]) -> &'packet Value {
    let id = view["P1"][zone]
        .as_array()
        .unwrap()
        .iter()
        .filter_map(Value::as_str)
        .find(|id| !old.contains(id))
        .unwrap();
    &view["objects"][id]
}
#[test]
fn pickaxe_all_printings_sacrifice_a_token_damage_the_target_and_recreate_only_its_name() {
    for number in ["BP11-074", "PR-313"] {
        let mut initial = draw_position();
        initial["players"]["P1"]["zones"]["hand"] = json!([{"id":"host","card":number}]);
        initial["players"]["P1"]["zones"]["field"] = json!([boosted_token("BP01-T03")]);
        initial["players"]["P2"]["zones"]["field"] =
            json!([{"id":"enemy","card":"BP01-037","state":{"hp":10_u8,"max_hp":10_u8}}]);
        let mut game = game(&initial);
        assert!(matches!(
            game.play_internal_card("host", PlayerId { seat: 1 })
                .unwrap(),
            CardPlay::Entered(_)
        ));
        settle(&mut game);
        let view = game.projection(View::Referee).unwrap();
        assert_eq!(view["objects"]["enemy"]["damage"], 4_u8);
        assert!(view["objects"]["chosen"].is_null());
        assert_eq!(view["P1"]["pp"]["current"], 7_u8);
        assert_eq!(view["P1"]["field"].as_array().unwrap().len(), 2);
        let token = created(&view, "field", &["host"]);
        assert_eq!(token["name"], "フェアリー");
        assert_eq!(token["power"], 1_u8);
        assert_eq!(token["max_hp"], 1_u8);
        assert_eq!(token["counters"], json!({}));
    }
}
#[test]
fn hamelin_copies_a_spell_token_in_ex_without_copying_counters() {
    let mut initial = draw_position();
    initial["players"]["P1"]["zones"]["hand"] = json!([{"id":"host","card":"BP05-083"}]);
    initial["players"]["P1"]["zones"]["ex"] =
        json!([{"id":"chosen","card":"BP01-T01","state":{"cost":9_u8,"counters":{"mark":4_u8}}}]);
    initial["players"]["P1"]["zones"]["deck"] = json!([{"id":"drawn","card":"BP01-037"}]);
    let mut game = game(&initial);
    assert!(matches!(
        game.play_internal_card("host", PlayerId { seat: 1 })
            .unwrap(),
        CardPlay::Entered(_)
    ));
    settle(&mut game);
    let view = game.projection(View::Referee).unwrap();
    assert_eq!(view["P1"]["ex"].as_array().unwrap().len(), 2);
    let token = created(&view, "ex", &["chosen"]);
    assert_eq!(token["name"], "薔薇の一撃");
    assert_eq!(token["counters"], json!({}));
    assert_eq!(view["objects"]["chosen"]["counters"]["mark"], 4_u8);
    let copy = token["id"].as_str().unwrap();
    let play = game.play_internal_card(copy, PlayerId { seat: 1 }).unwrap();
    resolved(finish(&mut game, play));
    settle(&mut game);
    let played = game.projection(View::Referee).unwrap();
    assert_eq!(
        played["P1"]["pp"]["current"], 6_u8,
        "the fresh copy pays its printed cost of two"
    );
    assert_eq!(played["P2"]["leader"]["life"], 17_u8);
    assert_eq!(played["P1"]["hand"], json!(["drawn"]));
}
#[test]
fn mirror_all_printings_select_original_cost_and_create_a_fresh_printed_follower() {
    for number in ["BP22-055", "BP22-P31"] {
        let mut initial = draw_position();
        initial["players"]["P1"]["zones"]["hand"] = json!([{"id":"spell","card":number}]);
        initial["players"]["P1"]["zones"]["field"] = json!([boosted_token("BP01-T03")]);
        let mut game = game(&initial);
        let play = game
            .play_internal_card("spell", PlayerId { seat: 1 })
            .unwrap();
        resolved(finish(&mut game, play));
        settle(&mut game);
        let view = game.projection(View::Referee).unwrap();
        let token = created(&view, "field", &["chosen"]);
        assert_eq!(token["name"], "フェアリー");
        assert_eq!(token["power"], 1_u8);
        assert_eq!(token["max_hp"], 1_u8);
        assert_eq!(token["counters"], json!({}));
        assert_eq!(view["objects"]["chosen"]["power"], 7_u8);
        assert_eq!(view["P1"]["cemetery"], json!(["spell"]));
    }
}
