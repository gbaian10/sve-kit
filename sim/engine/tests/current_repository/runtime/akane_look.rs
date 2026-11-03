#![expect(
    clippy::panic,
    reason = "Unexpected decisions invalidate the complete printed Akane scenarios."
)]
use super::*;
use sve_engine::game::internal::DecisionView;

fn game(initial: &Value) -> Game {
    Game::from_selected_effects(
        selection_with_rules().clone(),
        initial,
        &Value::Null,
        &Value::Null,
        "akane-look",
    )
    .unwrap()
}

fn position(card: &str, existing: usize) -> Value {
    let mut initial = draw_position();
    initial["players"]["P1"]["zones"]["hand"] = json!([{"id":"source","card":card}]);
    initial["players"]["P1"]["zones"]["field"] = json!(
        (0..existing)
            .map(|n| json!({"id":format!("old-{n}"),"card":"CP03-037"}))
            .collect::<Vec<_>>()
    );
    initial["players"]["P1"]["zones"]["deck"] = json!([
        {"id":"a","card":"CP03-037","state":{"cost":9_u8}},
        {"id":"b","card":"CP03-037","state":{"cost":8_u8}},
        {"id":"expensive","card":"CP03-032","state":{"cost":1_u8}},
        {"id":"wrong-trait","card":"CP03-013"},
        {"id":"spell","card":"CSD03a-007"},
        {"id":"tail","card":"CP03-013"}
    ]);
    initial["players"]["P2"]["zones"]["field"] = json!([
        {"id":"enemy","card":"CP03-013","state":{"acted":true,"power":0_u8,"hp":20_u8,"max_hp":20_u8}}
    ]);
    initial
}

fn awaiting(play: CardPlay) -> DecisionView {
    let CardPlay::Awaiting(q) = play else {
        panic!("printed choice")
    };
    q
}

fn trigger(board: &mut Game, card: &str, line: u64) -> CardPlay {
    if board.internal_card_stage() == Some(ResumeStage::CheckTiming) {
        board.resume_internal_card().unwrap();
    }
    let q = board.internal_pending_choice().unwrap().unwrap();
    let InputDomain::Pending(options) = &q.domain else {
        panic!("printed pending ability")
    };
    let handle = options
        .iter()
        .find(|o| o.card == card && o.line == line)
        .unwrap()
        .handle;
    board
        .answer_internal_card(q.actor, q.token, Answer::Pending(handle))
        .unwrap()
}

fn checks(board: &mut Game) {
    if board.internal_card_stage() == Some(ResumeStage::CheckTiming) {
        board.resume_internal_card().unwrap();
    }
    assert!(
        board.internal_pending_choice().unwrap().is_none(),
        "the actual cards have no additional immediate triggers"
    );
}

fn fanfare(board: &mut Game, card: &str) -> DecisionView {
    assert!(
        matches!(
            board
                .play_internal_card("source", PlayerId { seat: 1 })
                .unwrap(),
            CardPlay::Entered(_)
        ),
        "the real five-PP base enters before its fanfare"
    );
    awaiting(trigger(board, card, 2))
}

fn choose(board: &mut Game, q: &DecisionView, count: usize, maximum: u64) {
    let InputDomain::Targets(d) = &q.domain else {
        panic!("private top-five choice")
    };
    assert_eq!(
        d.members.len(),
        2,
        "only the two original-cost-two Royal Paladin followers qualify"
    );
    assert!(
        d.members
            .iter()
            .all(|m| m.card.as_deref() == Some("CP03-037")),
        "neither the cheapened cost-three follower, wrong trait nor spell is eligible"
    );
    assert_eq!(
        d.cardinality.minimum,
        Nat(0),
        "putting followers is optional"
    );
    assert_eq!(
        d.cardinality.maximum,
        Some(Nat(maximum)),
        "the maximum respects current free field slots"
    );
    assert!(
        board.internal_decision(View::P2).is_none(),
        "the opponent cannot inspect Look candidates"
    );
    let before = board.projection(View::Referee).unwrap();
    board
        .answer_internal_card(PlayerId { seat: 2 }, q.token, Answer::Targets(vec![]))
        .unwrap_err();
    board
        .answer_internal_card(q.actor, q.token, Answer::Targets(vec![u64::MAX]))
        .unwrap_err();
    if maximum < 2 {
        board
            .answer_internal_card(
                q.actor,
                q.token,
                Answer::Targets(d.members.iter().map(|m| m.handle).collect()),
            )
            .unwrap_err();
    }
    assert_eq!(
        board.projection(View::Referee).unwrap(),
        before,
        "foreign actors, unknown handles and over-capacity answers preserve state"
    );
    let play = board
        .answer_internal_card(
            q.actor,
            q.token,
            Answer::Targets(d.members.iter().take(count).map(|m| m.handle).collect()),
        )
        .unwrap();
    let order = awaiting(play);
    let InputDomain::Order(cards) = &order.domain else {
        panic!("the actual unselected top cards are ordered at the bottom")
    };
    assert_eq!(
        cards.len(),
        5_usize.checked_sub(count).unwrap(),
        "only successfully moved cards leave the observation remainder"
    );
    resolved(
        board
            .answer_internal_card(
                order.actor,
                order.token,
                Answer::Order(cards.iter().rev().map(|c| c.handle).collect()),
            )
            .unwrap(),
    );
    checks(board);
}

fn rush(view: &Value, id: &str) -> bool {
    view["objects"][id]["keywords"]
        .as_array()
        .unwrap()
        .contains(&json!("突進"))
}

fn combat(board: &mut Game) {
    checks(board);
    let q = board.internal_quick_choice().unwrap().unwrap();
    board
        .answer_internal_card(q.actor, q.token, Answer::Quick(None))
        .unwrap();
    checks(board);
}

#[test]
fn both_akane_printings_put_zero_one_or_two_real_followers_and_grant_only_them_rush() {
    for card in ["CP03-025", "CP03-P15"] {
        for count in 0_usize..=2 {
            let mut board = game(&position(card, 1));
            let q = fanfare(&mut board, card);
            choose(&mut board, &q, count, 2);
            let view = board.projection(View::Referee).unwrap();
            assert_eq!(
                view["P1"]["pp"]["current"], 5_u8,
                "only Akane's printed five PP is paid, despite the picked followers' current costs"
            );
            assert!(
                !rush(&view, "source") && !rush(&view, "old-0"),
                "the source and pre-existing follower do not receive Rush"
            );
            assert_eq!(
                view["P1"]["field"].as_array().unwrap().len(),
                2 + count,
                "each selected real card enters alongside the source and old follower"
            );
            let mut remainder = vec!["a", "b", "expensive", "wrong-trait", "spell"]
                .into_iter()
                .skip(count)
                .rev()
                .collect::<Vec<_>>();
            remainder.insert(0, "tail");
            assert_eq!(
                view["P1"]["deck"],
                json!(remainder),
                "the unseen tail stays above the chooser's reversed remainder"
            );
            for (index, id) in ["a", "b"].into_iter().enumerate() {
                assert_eq!(
                    rush(&view, id),
                    index < count,
                    "Rush follows only actual entries and never a card left in the deck"
                );
                if index < count {
                    assert_eq!(
                        board
                            .internal_rule_action(
                                &json!({"do":"attack","attacker":id,"target":"P2.leader"})
                            )
                            .unwrap()
                            .outcome,
                        "cannot-attack",
                        "Rush does not permit an entry-turn leader attack"
                    );
                    assert_eq!(
                        board
                            .internal_rule_action(
                                &json!({"do":"attack","attacker":id,"target":"enemy"})
                            )
                            .unwrap()
                            .outcome,
                        "resolved",
                        "each granted follower can actually attack the acted enemy on its entry turn"
                    );
                    combat(&mut board);
                }
            }
            assert_eq!(
                board.projection(View::Referee).unwrap()["objects"]["enemy"]["damage"],
                u64::try_from(count).unwrap() * 2,
                "every actual granted follower deals its printed two damage"
            );
        }
    }
}

#[test]
fn both_akane_printings_limit_selection_to_free_slots_and_leave_unmoved_cards_unmodified() {
    for card in ["CP03-025", "CP03-P15"] {
        for existing in [3_usize, 4] {
            let mut board = game(&position(card, existing));
            let q = fanfare(&mut board, card);
            let count = 4 - existing;
            choose(&mut board, &q, count, u64::try_from(count).unwrap());
            let view = board.projection(View::Referee).unwrap();
            assert_eq!(
                view["P1"]["field"].as_array().unwrap().len(),
                5,
                "the field is filled without overflowing"
            );
            assert_eq!(
                rush(&view, "a"),
                count == 1,
                "only a successful entry receives Rush"
            );
            assert!(
                !rush(&view, "b"),
                "the second candidate remains in the deck without Rush"
            );
            let mut expected = vec!["tail", "spell", "wrong-trait", "expensive", "b"];
            if count == 0 {
                expected.push("a");
            }
            assert_eq!(
                view["P1"]["deck"],
                json!(expected),
                "unmoved cards remain part of the ordered observation"
            );
            assert_eq!(
                view["P1"]["pp"]["current"], 5_u8,
                "capacity does not change the base's payment"
            );
        }
    }
}

fn evolution_position(base: &str, form: &str) -> Value {
    let mut initial = position(base, 0);
    initial["players"]["P1"]["zones"]["hand"] = json!([]);
    initial["players"]["P1"]["zones"]["field"] = json!([
        {"id":"source","card":base},{"id":"ally-a","card":"CP03-037"},{"id":"ally-b","card":"CP03-037"}
    ]);
    initial["players"]["P1"]["zones"]["evolve_deck"] = json!([
        {"id":"form","card":form},{"id":"wrong-form","card":"CP03-067"}
    ]);
    initial["players"]["P1"]["zones"]["deck"] = json!([
        {"id":"first","card":"CP03-013"},{"id":"second","card":"CP03-013"},{"id":"third","card":"CP03-013"}
    ]);
    initial
}

#[test]
fn both_akane_printings_pay_two_pp_to_reach_the_real_soul_saver_and_its_full_abilities() {
    for (base, form) in [("CP03-025", "CP03-026"), ("CP03-P15", "CP03-P16")] {
        let initial = evolution_position(base, form);
        let mut board = game(&initial);
        let before = board.projection(View::Referee).unwrap();
        assert_eq!(board.internal_rule_action(&json!({"do":"evolve","source":"source","evolve_card":"wrong-form","pay":{"pp":2_u8}})).unwrap().outcome,"cannot-evolve","the intrinsic named destination rejects another real evolve form");
        assert_eq!(
            board.projection(View::Referee).unwrap(),
            before,
            "rejected names spend and reveal nothing"
        );
        assert_eq!(
            board
                .internal_rule_action(
                    &json!({"do":"evolve","source":"source","evolve_card":"form","pay":{"pp":2_u8}})
                )
                .unwrap()
                .outcome,
            "resolved",
            "the selected real named form evolves normally"
        );
        let q = awaiting(trigger(&mut board, form, 2));
        let InputDomain::Targets(d) = &q.domain else {
            panic!("Soul Saver's real OnEvolve targets")
        };
        assert_eq!(
            d.members.len(),
            2,
            "the true form can select both other Royal Paladins"
        );
        resolved(
            board
                .answer_internal_card(
                    q.actor,
                    q.token,
                    Answer::Targets(d.members.iter().map(|m| m.handle).collect()),
                )
                .unwrap(),
        );
        checks(&mut board);
        let view = board.projection(View::Referee).unwrap();
        assert_eq!(
            view["objects"]["source"]["evolved_with"], "form",
            "the actual named resource is associated"
        );
        assert_eq!(
            view["objects"]["source"]["power"], 5_u8,
            "the real form supplies printed stats"
        );
        for id in ["ally-a", "ally-b"] {
            assert_eq!(
                view["objects"][id]["power"], 4_u8,
                "the true OnEvolve grants +2 power"
            );
            assert_eq!(
                view["objects"][id]["hp"], 5_u8,
                "the true OnEvolve also grants +2 life"
            );
        }
        assert_eq!(
            view["P1"]["pp"]["current"], 8_u8,
            "ordinary evolution pays exactly two PP"
        );
        assert_eq!(
            board
                .internal_rule_action(
                    &json!({"do":"attack","attacker":"source","target":"P2.leader"})
                )
                .unwrap()
                .outcome,
            "resolved",
            "the already-present evolved source attacks"
        );
        let result = resolved(trigger(&mut board, form, 1));
        assert_eq!(
            result
                .progress
                .facts
                .iter()
                .filter(|f| matches!(f, Fact::DriveChecked(_)))
                .count(),
            2,
            "the actual form's TwinDrive performs two checks"
        );
        combat(&mut board);
        let after = board.projection(View::Referee).unwrap();
        assert_eq!(
            after["P1"]["deck"],
            json!(["third", "first", "second"]),
            "normal-construction TwinDrive rotates two no-icon cards"
        );
        assert_eq!(
            after["P2"]["leader"]["life"], 15_u8,
            "combat uses the real evolved power"
        );
    }
}
