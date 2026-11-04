#![expect(
    clippy::panic,
    reason = "Only printed pending, target and Storm/Drain choices belong to these complete-card probes."
)]
use super::event_cemetery::finish_checks;
use super::*;
use sve_engine::game::internal::DecisionView;

fn game(initial: &Value) -> Game {
    Game::from_selected_effects(
        selection_with_rules().clone(),
        initial,
        &Value::Null,
        &Value::Null,
        "real-played-traits",
    )
    .unwrap()
}
fn position() -> Value {
    let mut initial = draw_position();
    initial["players"]["P1"]["zones"]["deck"] = json!([
        {"id":"draw0","card":"BP01-171"},{"id":"draw1","card":"BP01-171"},
        {"id":"draw2","card":"BP01-171"}
    ]);
    initial["players"]["P2"]["zones"]["field"] =
        json!([{"id":"enemy","card":"BP01-171","state":{"hp":10_u8,"max_hp":10_u8}}]);
    initial
}
fn pending(board: &mut Game, number: &str) -> CardPlay {
    finish_checks(board);
    let q = board
        .internal_pending_choice()
        .unwrap()
        .unwrap_or_else(|| panic!("missing printed pending ability for {number}"));
    let InputDomain::Pending(options) = q.domain else {
        panic!("real pending");
    };
    let selected = options.iter().find(|a| a.card == number).unwrap();
    board
        .answer_internal_card(q.actor, q.token, Answer::Pending(selected.handle))
        .unwrap()
}
fn awaiting(play: CardPlay) -> DecisionView {
    let CardPlay::Awaiting(q) = play else {
        panic!("printed target or keyword choice");
    };
    q
}
fn target(board: &mut Game, q: DecisionView, number: &str) -> CardPlay {
    let InputDomain::Targets(domain) = q.domain else {
        panic!("printed target");
    };
    let member = domain
        .members
        .iter()
        .find(|m| m.card.as_deref() == Some(number))
        .unwrap();
    board
        .answer_internal_card(q.actor, q.token, Answer::Targets(vec![member.handle]))
        .unwrap()
}
fn enter(board: &mut Game, id: &str) {
    assert!(
        matches!(
            board.play_internal_card(id, PlayerId { seat: 1 }).unwrap(),
            CardPlay::Entered(_)
        ),
        "the complete printed follower enters before its Fanfare"
    );
    finish_checks(board);
}
fn keyword(view: &Value, id: &str, keyword: &str) -> bool {
    view["objects"][id]["keywords"]
        .as_array()
        .unwrap()
        .contains(&json!(keyword))
}

#[test]
fn mage_prints_exclude_their_own_play_then_offer_either_storm_or_drain_for_another_mage() {
    for number in ["BP14-043", "BP14-P10"] {
        for mode in [1, 2] {
            let mut initial = position();
            initial["players"]["P1"]["zones"]["hand"] = json!([
                {"id":"first","card":number},{"id":"second","card":number}
            ]);
            let mut board = game(&initial);
            enter(&mut board, "first");
            resolved(pending(&mut board, number));
            finish_checks(&mut board);
            let first_view = board.projection(View::Referee).unwrap();
            assert!(
                !keyword(&first_view, "first", "疾走")
                    && !keyword(&first_view, "first", "ドレイン"),
                "the first Mage cannot count itself as another card"
            );
            enter(&mut board, "second");
            let q = awaiting(pending(&mut board, number));
            assert!(matches!(q.domain, InputDomain::Modes { .. }));
            resolved(
                board
                    .answer_internal_card(q.actor, q.token, Answer::Modes(vec![ModeId(mode)]))
                    .unwrap(),
            );
            finish_checks(&mut board);
            let view = board.projection(View::Referee).unwrap();
            assert_eq!(keyword(&view, "second", "疾走"), mode == 1);
            assert_eq!(keyword(&view, "second", "ドレイン"), mode == 2);
            assert_eq!(
                view["P1"]["pp"]["current"], 4_u8,
                "both complete normal cards pay their printed three"
            );
        }
    }
}

#[test]
fn evolved_mage_prints_count_two_dual_trait_real_plays_once_each_and_keep_normal_evolution_cost() {
    for number in ["BP14-044", "BP14-P11"] {
        let mut initial = position();
        initial["players"]["P1"]["zones"]["hand"] = json!([
            {"id":"source","card":"BP14-043"},{"id":"second","card":"BP14-043"}
        ]);
        initial["players"]["P1"]["zones"]["evolve_deck"] = json!([{"id":"form","card":number}]);
        let mut board = game(&initial);
        enter(&mut board, "source");
        resolved(pending(&mut board, "BP14-043"));
        finish_checks(&mut board);
        enter(&mut board, "second");
        let keyword_choice = awaiting(pending(&mut board, "BP14-043"));
        resolved(
            board
                .answer_internal_card(
                    keyword_choice.actor,
                    keyword_choice.token,
                    Answer::Modes(vec![ModeId(1)]),
                )
                .unwrap(),
        );
        finish_checks(&mut board);
        board
            .internal_rule_action(
                &json!({"do":"evolve","source":"source","evolve_card":"form","pay":{"pp":1_u8}}),
            )
            .unwrap();
        let q = awaiting(pending(&mut board, number));
        resolved(target(&mut board, q, "BP01-171"));
        finish_checks(&mut board);
        let view = board.projection(View::Referee).unwrap();
        assert_eq!(
            view["objects"]["enemy"]["hp"], 6_u8,
            "two actual dual-trait cards cause four, never eight"
        );
        assert_eq!(
            view["P1"]["pp"]["current"], 3_u8,
            "the real normal face supplies its PP1 evolution"
        );
    }
}

#[test]
fn kyoka_prints_keep_full_normal_recovery_and_damage_the_dead_targets_leader_only_after_a_real_spell_play()
 {
    for number in ["CP04-046", "CP04-P34"] {
        for spell in [false, true] {
            let mut initial = position();
            initial["players"]["P1"]["construction"] = json!("title");
            initial["players"]["P1"]["title_code"] = json!("princess_connect_re_dive");
            initial["players"]["P1"]["zones"]["hand"] = json!([
                {"id":"prior","card":if spell {"CP04-054"}else{"BP01-171"}},
                {"id":"source","card":"CP04-045"}
            ]);
            initial["players"]["P1"]["zones"]["cemetery"] =
                json!([{"id":"recover","card":"CP04-118"}]);
            initial["players"]["P1"]["zones"]["evolve_deck"] = json!([{"id":"form","card":number}]);
            initial["players"]["P2"]["zones"]["field"] = json!([{"id":"enemy","card":"BP01-171"}]);
            let mut board = game(&initial);
            if !spell {
                enter(&mut board, "prior");
            }
            enter(&mut board, "source");
            let recovery = awaiting(pending(&mut board, "CP04-045"));
            resolved(target(&mut board, recovery, "CP04-118"));
            finish_checks(&mut board);
            assert_eq!(
                board.projection(View::Referee).unwrap()["P1"]["ex"],
                json!(["recover"]),
                "the unchanged normal face executes its whole mandatory recovery"
            );
            if spell {
                let prior_spell = awaiting(
                    board
                        .play_internal_card("prior", PlayerId { seat: 1 })
                        .unwrap(),
                );
                resolved(target(&mut board, prior_spell, "CP04-045"));
                finish_checks(&mut board);
            }
            board
                .internal_rule_action(
                    &json!({"do":"evolve","source":"source","evolve_card":"form","pay":{"pp":1_u8}}),
                )
                .unwrap();
            let q = awaiting(pending(&mut board, number));
            let result = resolved(target(&mut board, q, "BP01-171"));
            assert_eq!(
                result
                    .progress
                    .facts
                    .iter()
                    .filter(|fact| matches!(
                        fact,
                        Fact::AbilityPlayed {
                            qualification: AbilityQualification::UnionBurst,
                            ..
                        }
                    ))
                    .count(),
                1
            );
            finish_checks(&mut board);
            let view = board.projection(View::Referee).unwrap();
            assert_eq!(view["P2"]["cemetery"], json!(["enemy"]));
            assert_eq!(
                view["P2"]["leader"]["life"],
                if spell { 18_u8 } else { 20_u8 },
                "the selected controller remains available after lethal follower damage"
            );
        }
    }
}
