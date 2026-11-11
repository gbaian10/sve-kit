#![expect(
    clippy::panic,
    reason = "Unexpected decision domains invalidate the printed card scenarios."
)]

use super::damage_prevent::pass_quick;
use super::event_cemetery::finish_checks;
use super::event_combat_life::attack;
use super::*;

const PLAYER: PlayerId = PlayerId { seat: 1 };

fn evolve_position(base: &str, form: &str, class: &str) -> Value {
    let mut initial = draw_position();
    initial["players"]["P1"]["leader"]["class"] = json!(class);
    initial["players"]["P1"]["zones"]["field"] = json!([{"id":"source","card":base}]);
    initial["players"]["P1"]["zones"]["evolve_deck"] = json!([{"id":"form","card":form}]);
    initial
}

fn evolve(game: &mut Game) {
    assert_eq!(
        game.internal_rule_action(
            &json!({"do":"evolve","source":"source","evolve_card":"form","pay":{"pp":1_u8}}),
        )
        .unwrap()
        .outcome,
        "resolved",
        "the printed PP cost pays for the evolution"
    );
}

fn printed_pending(game: &mut Game, number: &str, line: u64) -> CardPlay {
    finish_checks(game);
    let question = game.internal_pending_choice().unwrap().unwrap();
    let InputDomain::Pending(options) = &question.domain else {
        panic!("expected a printed ability");
    };
    let ability = options
        .iter()
        .find(|option| option.card == number && option.line == line)
        .unwrap();
    game.answer_internal_card(
        question.actor,
        question.token,
        Answer::Pending(ability.handle),
    )
    .unwrap()
}

fn target(game: &mut Game, play: CardPlay, number: &str) -> CardPlay {
    let CardPlay::Awaiting(question) = play else {
        panic!("expected a formal target");
    };
    let InputDomain::Targets(domain) = &question.domain else {
        panic!("expected a formal target domain");
    };
    assert_eq!(domain.members.len(), 1, "only one target is legal");
    assert_eq!(
        domain.members[0].card.as_deref(),
        Some(number),
        "the printed target filter selects the expected card"
    );
    game.answer_internal_card(
        question.actor,
        question.token,
        Answer::Targets(vec![domain.members[0].handle]),
    )
    .unwrap()
}

fn pay_material(game: &mut Game, play: CardPlay, number: &str) -> CardPlay {
    let CardPlay::Awaiting(question) = play else {
        panic!("expected a printed material cost");
    };
    let InputDomain::CostMaterials(domain) = &question.domain else {
        panic!("expected a material domain");
    };
    assert_eq!(
        domain.members.len(),
        1,
        "only one payment material is legal"
    );
    assert_eq!(
        domain.members[0].card.as_deref(),
        Some(number),
        "the printed material filter selects the expected card"
    );
    game.answer_internal_card(
        question.actor,
        question.token,
        Answer::CostMaterials(vec![domain.members[0].handle]),
    )
    .unwrap()
}

#[test]
fn tartarus_evolution_returns_only_an_own_dead_follower() {
    let mut initial = evolve_position("BP08-072", "BP08-P18", "ナイトメア");
    initial["players"]["P1"]["zones"]["cemetery"] = json!([
        {"id":"dead","card":"BP08-081"},{"id":"other","card":"BP01-037"}
    ]);
    initial["players"]["P2"]["zones"]["cemetery"] = json!([{"id":"opposing","card":"BP08-081"}]);
    let mut board = game(&initial);
    evolve(&mut board);
    let play = printed_pending(&mut board, "BP08-P18", 1);
    resolved(target(&mut board, play, "BP08-081"));
    let state = board.projection(View::Referee).unwrap();
    assert_eq!(state["P1"]["field"], json!(["source", "dead"]));
    assert_eq!(state["P1"]["cemetery"], json!(["other"]));
    assert_eq!(state["P2"]["cemetery"], json!(["opposing"]));
}

#[test]
fn belphomet_pays_each_printed_cost_to_create_a_tentacle() {
    let mut initial = evolve_position("BP12-035", "BP12-SL10", "ウィッチ");
    initial["players"]["P1"]["zones"]["hand"] = json!([
        {"id":"machine","card":"BP12-035"},{"id":"other","card":"BP01-037"}
    ]);
    initial["players"]["P1"]["zones"]["ex"] = json!([
        {"id":"token","card":"BP12-T03"},{"id":"not-token","card":"BP12-035"}
    ]);
    let mut board = game(&initial);
    evolve(&mut board);
    let CardPlay::Awaiting(payment) = printed_pending(&mut board, "BP12-SL10", 1) else {
        panic!("expected a choice to pay the evolution ability");
    };
    assert_eq!(payment.domain, InputDomain::OptionalCost);
    let discard = board
        .answer_internal_card(payment.actor, payment.token, Answer::OptionalCost(true))
        .unwrap();
    resolved(pay_material(&mut board, discard, "BP12-035"));
    finish_checks(&mut board);
    let activation = selection().current["BP12-SL10"]
        .program
        .card()
        .abilities
        .iter()
        .find(|ability| ability.node.kind == AbilityKind::Activated)
        .unwrap()
        .node
        .id;
    let banish = board
        .activate_internal_card("source", activation, PLAYER)
        .unwrap();
    resolved(pay_material(&mut board, banish, "BP12-T03"));
    let state = board.projection(View::Referee).unwrap();
    assert_eq!(state["P1"]["hand"], json!(["other"]));
    assert_eq!(state["P1"]["cemetery"], json!(["machine"]));
    assert_eq!(state["P1"]["ex"], json!(["not-token"]));
    assert_eq!(state["P1"]["pp"]["current"], 7_u8);
    let field = state["P1"]["field"].as_array().unwrap();
    assert_eq!(field.len(), 3);
    for id in &field[1..] {
        let token = &state["objects"][id.as_str().unwrap()];
        assert_eq!(token["name"], "蹂躙の触手");
        assert_eq!(token["keywords"], json!(["疾走"]));
    }
}

#[test]
fn poem_of_silence_uses_the_cemetery_count_before_returning_lulunai() {
    let mut initial = draw_position();
    initial["players"]["P1"]["leader"]["class"] = json!("ナイトメア");
    initial["players"]["P1"]["zones"]["hand"] = json!([{"id":"spell","card":"BP08-P19"}]);
    let mut cemetery = vec![json!({"id":"lulunai","card":"BP05-070"})];
    cemetery.extend((0_u8..9).map(|n| json!({"id":format!("other{n}"),"card":"BP01-037"})));
    initial["players"]["P1"]["zones"]["cemetery"] = json!(cemetery);
    initial["players"]["P2"]["zones"]["hand"] = json!([{"id":"discarded","card":"BP01-037"}]);
    let mut board = game(&initial);
    let play = board.play_internal_card("spell", PLAYER).unwrap();
    resolved(target(&mut board, play, "BP05-070"));
    let state = board.projection(View::Referee).unwrap();
    assert_eq!(state["P1"]["field"], json!(["lulunai"]));
    assert_eq!(state["P2"]["hand"], json!([]));
    assert_eq!(state["P2"]["cemetery"], json!(["discarded"]));
}

#[test]
fn argus_heals_on_the_third_aqua_force_attack_and_evolves_into_navelgazer() {
    let mut initial = evolve_position("CP03-P01", "CP03-006", "エルフ");
    initial["players"]["P1"]["leader"]["life"] = json!(15_u8);
    initial["players"]["P1"]["zones"]["field"] = json!([
        {"id":"source","card":"CP03-P01"},
        {"id":"first","card":"CP03-009"},
        {"id":"second","card":"CP03-009"},
        {"id":"third","card":"CP03-009"},
        {"id":"fourth","card":"CP03-009"}
    ]);
    initial["players"]["P1"]["zones"]["deck"] = json!([{"id":"drawn","card":"BP01-037"}]);
    let mut board = game(&initial);
    for (attacker, life, hand) in [
        ("first", 15_u8, json!([])),
        ("second", 15_u8, json!([])),
        ("third", 17_u8, json!(["drawn"])),
        ("fourth", 17_u8, json!(["drawn"])),
    ] {
        attack(&mut board, attacker, "P2.leader");
        let play = printed_pending(&mut board, "CP03-P01", 2);
        resolved(play);
        finish_checks(&mut board);
        pass_quick(&mut board);
        let state = board.projection(View::Referee).unwrap();
        assert_eq!(state["P1"]["leader"]["life"], life);
        assert_eq!(state["P1"]["hand"], hand);
    }
    evolve(&mut board);
    assert_eq!(
        board.projection(View::Referee).unwrap()["objects"]["source"]["name"],
        "ネイブルゲイザー・ドラゴン"
    );
}

#[test]
fn snow_awake_sets_both_stats_and_at_spellchain_ten_removes_ward() {
    for spells in [9_u8, 10] {
        let mut initial = draw_position();
        initial["players"]["P1"]["leader"]["class"] = json!("ウィッチ");
        initial["players"]["P1"]["zones"]["hand"] = json!([{"id":"spell","card":"BP16-049"}]);
        initial["players"]["P1"]["zones"]["cemetery"] = json!(
            (0..spells)
                .map(|n| json!({"id":format!("spent{n}"),"card":"BP01-170"}))
                .collect::<Vec<_>>()
        );
        initial["players"]["P2"]["zones"]["field"] = json!([{"id":"enemy","card":"BP01-002"}]);
        let mut board = game(&initial);
        let play = board.play_internal_card("spell", PLAYER).unwrap();
        resolved(target(&mut board, play, "BP01-002"));
        let state = board.projection(View::Referee).unwrap();
        let enemy = &state["objects"]["enemy"];
        assert_eq!(enemy["power"], 1_u8);
        assert_eq!(enemy["hp"], 1_u8);
        assert_eq!(
            enemy["keywords"],
            if spells == 10 {
                json!([])
            } else {
                json!(["守護"])
            }
        );
    }
}
