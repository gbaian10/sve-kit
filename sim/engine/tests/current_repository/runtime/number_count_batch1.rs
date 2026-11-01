#![expect(
    clippy::panic,
    clippy::wildcard_enum_match_arm,
    reason = "Unexpected decisions invalidate these printed numeric scenarios."
)]
use super::event_cemetery::finish_checks;
use super::*;

const PLAYER: PlayerId = PlayerId { seat: 1 };

fn printings(number: &str) -> Vec<String> {
    let card = &selection().current[number].card_id;
    selection()
        .current
        .iter()
        .filter(|(_, selected)| &selected.card_id == card)
        .map(|(printing, _)| printing.clone())
        .collect()
}

fn position(number: &str) -> Value {
    let mut initial = draw_position();
    initial["players"]["P1"]["zones"]["hand"] = json!([{"id":"source","card":number}]);
    initial["players"]["P1"]["zones"]["deck"] =
        json!([{"id":"draw1","card":"BP01-037"},{"id":"draw2","card":"BP01-037"}]);
    initial["players"]["P2"]["zones"]["deck"] = json!([{"id":"opponent-draw","card":"BP01-037"}]);
    initial["players"]["P2"]["zones"]["field"] = json!([{"id":"enemy","card":"BP01-037","state":{"hp":15_u8,"max_hp":15_u8,"power":0_u8,"acted":true}}]);
    initial
}

fn new_game(initial: &Value) -> Game {
    Game::from_selected_effects(
        selection_with_rules().clone(),
        initial,
        &Value::Null,
        &Value::Null,
        "number-count-batch1",
    )
    .unwrap()
}

fn finish(game: &mut Game, mut play: CardPlay) -> CardPlay {
    while let CardPlay::Awaiting(q) = play {
        let answer = match &q.domain {
            InputDomain::Targets(domain) => Answer::Targets(vec![
                domain
                    .members
                    .iter()
                    .find(|member| member.card.as_deref() == Some("BP01-037"))
                    .unwrap_or(&domain.members[0])
                    .handle,
            ]),
            InputDomain::CostMaterials(domain) => {
                Answer::CostMaterials(vec![domain.members[0].handle])
            }
            InputDomain::WardEntry { .. } => Answer::WardEntry(false),
            InputDomain::OptionalCost => Answer::OptionalCost(true),
            _ => panic!("unexpected batch decision"),
        };
        play = game.answer_internal_card(q.actor, q.token, answer).unwrap();
    }
    play
}

fn trigger(game: &mut Game) {
    let q = game.internal_pending_choice().unwrap().unwrap();
    let InputDomain::Pending(options) = &q.domain else {
        panic!("printed pending trigger")
    };
    assert_eq!(options.len(), 1, "exactly one printed trigger is pending");
    let play = game
        .answer_internal_card(q.actor, q.token, Answer::Pending(options[0].handle))
        .unwrap();
    resolved(finish(game, play));
    game.resume_internal_card().unwrap();
}

fn activate(game: &mut Game, number: &str, line: u32) {
    let id = selection().current[number]
        .program
        .card()
        .abilities
        .iter()
        .find(|a| a.source.line == line)
        .unwrap()
        .node
        .id;
    let play = game.activate_internal_card("source", id, PLAYER).unwrap();
    resolved(finish(game, play));
}

fn play(game: &mut Game) {
    let result = game.play_internal_card("source", PLAYER).unwrap();
    assert!(
        matches!(finish(game, result), CardPlay::Entered(_)),
        "the printed follower enters the field"
    );
    finish_checks(game);
    trigger(game);
}

#[test]
fn evolved_gigas_dragon_slayer_and_lorena_deal_their_current_attack() {
    for (number, base, power) in [
        ("BP02-015", "BP02-014", 3_u8),
        ("BP09-064", "BP09-063", 2),
        ("BP18-108", "BP18-107", 4),
    ] {
        for printing in printings(number) {
            let mut initial = position(&printing);
            initial["players"]["P1"]["zones"]["hand"] = json!([]);
            initial["players"]["P1"]["zones"]["field"] = json!([{"id":"source","card":base}]);
            initial["players"]["P1"]["zones"]["evolve_deck"] =
                json!([{"id":"form","card":printing}]);
            let mut game = new_game(&initial);
            assert_eq!(
                game.internal_rule_action(
                    &json!({"do":"evolve","source":"source","evolve_card":"form","pay":{"pp":1_u8}})
                )
                .unwrap()
                .outcome,
                "resolved"
            );
            finish_checks(&mut game);
            trigger(&mut game);
            let state = game.projection(View::Referee).unwrap();
            assert_eq!(state["objects"]["enemy"]["damage"], power);
            assert_eq!(state["P1"]["pp"]["current"], 9_u8);
        }
    }
}

#[test]
fn vampy_and_laplus_count_only_allied_field_bats_then_spend_a_real_bat() {
    for number in ["BP02-069", "BP02-070"] {
        for printing in printings(number) {
            let mut initial = position(&printing);
            initial["players"]["P1"]["zones"]["field"] =
                json!([{"id":"bat1","card":"BP01-T15"},{"id":"bat2","card":"BP01-T15"}]);
            initial["players"]["P1"]["zones"]["ex"] = json!([{"id":"ex-bat","card":"BP01-T15"}]);
            initial["players"]["P2"]["zones"]["field"]
                .as_array_mut()
                .unwrap()
                .push(json!({"id":"opposing-bat","card":"BP01-T15"}));
            let mut game = new_game(&initial);
            play(&mut game);
            let before = game.projection(View::Referee).unwrap();
            assert_eq!(before["objects"]["source"]["power"], 4_u8);
            assert_eq!(before["objects"]["source"]["hp"], 4_u8);
            activate(&mut game, &printing, 2);
            let state = game.projection(View::Referee).unwrap();
            assert_eq!(state["objects"]["enemy"]["damage"], 3_u8);
            assert_eq!(state["P2"]["leader"]["life"], 19_u8);
            assert_eq!(state["P1"]["pp"]["current"], 7_u8);
            assert_eq!(state["P1"]["field"].as_array().unwrap().len(), 2);
        }
    }
}

#[test]
fn waltz_uses_live_attack_and_its_other_ability_banishes_only_an_amulet() {
    for printing in printings("BP07-027") {
        for line in [1_u32, 2] {
            let mut initial = position(&printing);
            initial["players"]["P1"]["zones"]["hand"] = json!([]);
            initial["players"]["P1"]["zones"]["field"] =
                json!([{"id":"source","card":printing,"state":{"power":5_u8}}]);
            initial["players"]["P2"]["zones"]["field"]
                .as_array_mut()
                .unwrap()
                .push(json!({"id":"amulet","card":"BP09-102"}));
            let mut game = new_game(&initial);
            activate(&mut game, &printing, line);
            let state = game.projection(View::Referee).unwrap();
            if line == 1 {
                assert_eq!(state["objects"]["enemy"]["damage"], 5_u8);
                assert_eq!(state["P1"]["pp"]["current"], 9_u8);
            } else {
                assert_eq!(state["P2"]["banish"], json!(["amulet"]));
                assert_eq!(state["P1"]["pp"]["current"], 7_u8);
            }
            assert_eq!(state["objects"]["source"]["acted"], true);
        }
    }
}

#[test]
fn master_samurai_fanfare_deals_its_printed_two_attack() {
    for printing in printings("BP09-031") {
        let initial = position(&printing);
        let mut game = new_game(&initial);
        play(&mut game);
        let state = game.projection(View::Referee).unwrap();
        assert_eq!(state["objects"]["enemy"]["damage"], 2_u8);
        assert_eq!(state["objects"]["source"]["power"], 2_u8);
        assert_eq!(state["P1"]["pp"]["current"], 7_u8);
    }
}

#[test]
fn believer_counts_itself_and_other_effective_wards_at_real_end_phase() {
    for printing in printings("BP12-093") {
        let mut initial = position(&printing);
        initial["players"]["P1"]["zones"]["hand"] = json!([]);
        initial["players"]["P1"]["zones"]["field"] = json!([{"id":"source","card":printing},{"id":"ward","card":"BP01-046"},{"id":"ordinary","card":"BP01-037"}]);
        initial["players"]["P2"]["zones"]["field"] = json!([{"id":"enemy-ward","card":"BP01-046"}]);
        let mut game = new_game(&initial);
        game.internal_rule_action(&json!({"do":"end-phase"}))
            .unwrap();
        trigger(&mut game);
        assert_eq!(
            game.projection(View::Referee).unwrap()["objects"]["source"]["power"],
            3_u8
        );
    }
}

#[test]
fn windy_dragon_newt_heals_by_its_current_remaining_life_when_attacking() {
    for printing in printings("BP15-068") {
        let mut initial = position(&printing);
        initial["players"]["P1"]["leader"]["life"] = json!(10_u8);
        initial["players"]["P1"]["zones"]["hand"] = json!([]);
        initial["players"]["P1"]["zones"]["field"] = json!([{"id":"source","card":printing,"state":{"hp":3_u8,"max_hp":4_u8,"damage":1_u8}}]);
        let mut game = new_game(&initial);
        game.internal_rule_action(&json!({"do":"attack","attacker":"source","target":"enemy"}))
            .unwrap();
        trigger(&mut game);
        assert_eq!(
            game.projection(View::Referee).unwrap()["P1"]["leader"]["life"],
            13_u8
        );
    }
}

#[test]
fn hermit_buffs_itself_then_damages_enemies_at_the_opponents_main_phase() {
    for printing in printings("BP15-108") {
        let initial = position(&printing);
        let mut game = new_game(&initial);
        play(&mut game);
        assert_eq!(
            game.projection(View::Referee).unwrap()["objects"]["source"]["power"],
            3_u8
        );
        game.internal_rule_action(&json!({"do":"end-phase"}))
            .unwrap();
        let q = game.internal_quick_choice().unwrap().unwrap();
        game.answer_internal_card(q.actor, q.token, Answer::Quick(None))
            .unwrap();
        trigger(&mut game);
        let state = game.projection(View::Referee).unwrap();
        assert_eq!(state["objects"]["enemy"]["damage"], 3_u8);
        assert_eq!(state["objects"]["source"]["damage"], 0_u8);
        assert_eq!(state["P1"]["pp"]["current"], 5_u8);
    }
}

#[test]
fn evolved_velly_counts_the_bats_it_just_created() {
    for printing in printings("BP18-093") {
        let mut initial = position(&printing);
        initial["players"]["P1"]["zones"]["hand"] = json!([]);
        initial["players"]["P1"]["zones"]["field"] =
            json!([{"id":"source","card":"BP18-092"},{"id":"old-bat","card":"BP01-T15"}]);
        initial["players"]["P1"]["zones"]["ex"] = json!([{"id":"ex-bat","card":"BP01-T15"}]);
        initial["players"]["P1"]["zones"]["evolve_deck"] = json!([{"id":"form","card":printing}]);
        let mut game = new_game(&initial);
        game.internal_rule_action(
            &json!({"do":"evolve","source":"source","evolve_card":"form","pay":{"pp":1_u8}}),
        )
        .unwrap();
        finish_checks(&mut game);
        trigger(&mut game);
        let state = game.projection(View::Referee).unwrap();
        assert_eq!(state["objects"]["enemy"]["damage"], 3_u8);
        assert_eq!(state["P1"]["field"].as_array().unwrap().len(), 4);
        assert_eq!(state["P1"]["ex"], json!(["ex-bat"]));
    }
}

#[test]
fn shiro_revives_a_cheap_ward_then_evolves_and_counts_that_other_ward() {
    for printing in printings("BP15-098") {
        let mut initial = position(&printing);
        initial["players"]["P1"]["zones"]["cemetery"] = json!([
            {"id":"guard","card":"BP01-046"},
            {"id":"ordinary","card":"BP01-037"},
            {"id":"expensive-ward","card":"BP02-105"}
        ]);
        initial["players"]["P1"]["zones"]["ex"] = json!([{"id":"ex-guard","card":"BP01-046"}]);
        initial["players"]["P1"]["zones"]["evolve_deck"] = json!([{"id":"form","card":"BP15-099"}]);
        initial["players"]["P2"]["zones"]["field"]
            .as_array_mut()
            .unwrap()
            .push(
                json!({"id":"enemy-guard","card":"BP01-046","state":{"hp":10_u8,"max_hp":10_u8}}),
            );
        let mut game = new_game(&initial);
        play(&mut game);
        let revived = game.projection(View::Referee).unwrap();
        assert_eq!(revived["P1"]["field"], json!(["source", "guard"]));
        assert_eq!(
            revived["P1"]["cemetery"],
            json!(["ordinary", "expensive-ward"])
        );
        assert_eq!(revived["P1"]["pp"]["current"], 6_u8);
        assert_eq!(
            game.internal_rule_action(
                &json!({"do":"evolve","source":"source","evolve_card":"form","pay":{"pp":1_u8}})
            )
            .unwrap()
            .outcome,
            "resolved"
        );
        finish_checks(&mut game);
        trigger(&mut game);
        let evolved = game.projection(View::Referee).unwrap();
        assert_eq!(evolved["objects"]["enemy"]["damage"], 1_u8);
        assert_eq!(evolved["objects"]["enemy-guard"]["damage"], 1_u8);
        assert_eq!(evolved["P2"]["leader"]["life"], 20_u8);
        assert_eq!(evolved["P1"]["pp"]["current"], 5_u8);
    }
}

#[test]
fn hermit_negative_attack_deals_no_damage_at_the_real_opponents_main_phase() {
    for printing in printings("BP15-108") {
        let mut initial = position(&printing);
        initial["players"]["P1"]["zones"]["hand"] = json!([]);
        initial["players"]["P1"]["zones"]["field"] = json!([{
            "id":"source","card":printing,
            "state":{"power":-2_i8,"hp":2_u8,"max_hp":5_u8,"damage":3_u8}
        }]);
        let mut game = new_game(&initial);
        game.internal_rule_action(&json!({"do":"end-phase"}))
            .unwrap();
        let question = game.internal_quick_choice().unwrap().unwrap();
        game.answer_internal_card(question.actor, question.token, Answer::Quick(None))
            .unwrap();
        trigger(&mut game);
        let state = game.projection(View::Referee).unwrap();
        assert_eq!(state["objects"]["source"]["power"], -2_i8);
        assert_eq!(state["objects"]["enemy"]["damage"], 0_u8);
        assert_eq!(state["objects"]["enemy"]["hp"], 15_u8);
        assert_eq!(state["P2"]["leader"]["life"], 20_u8);
    }
}
