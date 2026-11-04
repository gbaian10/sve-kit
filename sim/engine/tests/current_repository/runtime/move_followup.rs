#![expect(
    clippy::panic,
    reason = "Complete printed movement scenarios require their declared choices."
)]
use super::*;
use sve_engine::game::internal::{DecisionView, SearchAnswer};

fn game(initial: &Value) -> Game {
    Game::from_selected_effects(
        selection_with_rules().clone(),
        initial,
        &Value::Null,
        &Value::Null,
        "printed-move-followup",
    )
    .unwrap()
}

fn position(card: &str, pp: u8) -> Value {
    let mut initial = draw_position();
    initial["players"]["P1"]["pp"] = json!({"current":pp,"max":pp});
    initial["players"]["P1"]["zones"]["hand"] = json!([{"id":"source","card":card}]);
    initial["players"]["P2"]["zones"]["field"] = json!([
        {"id":"enemy","card":"CP03-013","state":{"acted":true,"power":0_u8,"hp":20_u8,"max_hp":20_u8}}
    ]);
    initial
}

fn awaiting(play: CardPlay) -> DecisionView {
    let CardPlay::Awaiting(q) = play else {
        panic!("printed movement choice");
    };
    q
}

fn trigger(board: &mut Game, card: &str, line: u64) -> CardPlay {
    if board.internal_card_stage() == Some(ResumeStage::CheckTiming) {
        board.resume_internal_card().unwrap();
    }
    let q = board.internal_pending_choice().unwrap().unwrap();
    let InputDomain::Pending(options) = &q.domain else {
        panic!("printed pending ability");
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

fn select(board: &mut Game, q: &DecisionView, card: &str) -> CardPlay {
    let InputDomain::Targets(domain) = &q.domain else {
        panic!("printed target or evolution form");
    };
    assert_eq!(
        domain.members.len(),
        1,
        "only the printed eligible card is available"
    );
    assert_eq!(
        domain.members[0].card.as_deref(),
        Some(card),
        "kind, class, trait, original or current cost filters apply"
    );
    let before = board.projection(View::Referee).unwrap();
    board
        .answer_internal_card(PlayerId { seat: 2 }, q.token, Answer::Targets(vec![]))
        .unwrap_err();
    board
        .answer_internal_card(q.actor, q.token, Answer::Targets(vec![u64::MAX]))
        .unwrap_err();
    assert_eq!(
        board.projection(View::Referee).unwrap(),
        before,
        "foreign actors and forged handles preserve state"
    );
    board
        .answer_internal_card(
            q.actor,
            q.token,
            Answer::Targets(vec![domain.members[0].handle]),
        )
        .unwrap()
}

fn checks(board: &mut Game) {
    if board.internal_card_stage() == Some(ResumeStage::CheckTiming) {
        board.resume_internal_card().unwrap();
    }
    assert!(
        board.internal_pending_choice().unwrap().is_none(),
        "all actual immediate printed triggers have resolved"
    );
}

fn keyword(view: &Value, id: &str, name: &str) -> bool {
    view["objects"][id]["keywords"]
        .as_array()
        .unwrap()
        .contains(&json!(name))
}

fn blaster_evolves(board: &mut Game) {
    let form_choice = awaiting(trigger(board, "CSD03a-003", 3));
    assert!(
        board.internal_decision(View::P2).is_none(),
        "the evolution deck stays private"
    );
    resolved(select(board, &form_choice, "CSD03a-004"));
    let damage_targets = awaiting(trigger(board, "CSD03a-004", 2));
    resolved(select(board, &damage_targets, "CP03-013"));
    checks(board);
}

#[test]
fn both_panthers_revive_only_originally_cheap_royal_followers_and_buff_only_actual_entries() {
    for printing in ["BP08-023", "PR-200"] {
        for full in [false, true] {
            let mut initial = position(printing, 4);
            initial["players"]["P1"]["zones"]["field"] = json!(
                (0..if full { 4_u8 } else { 0_u8 })
                    .map(|n| json!({"id":format!("old-{n}"),"card":"CP03-037"}))
                    .collect::<Vec<_>>()
            );
            initial["players"]["P1"]["zones"]["cemetery"] = json!([
                {"id":"found","card":"CP03-037","state":{"cost":9_u8}},
                {"id":"expensive","card":"CP03-032","state":{"cost":1_u8}},
                {"id":"wrong-class","card":"CP03-013"},
                {"id":"spell","card":"CSD03a-007"}
            ]);
            let mut board = game(&initial);
            assert!(
                matches!(
                    board
                        .play_internal_card("source", PlayerId { seat: 1 })
                        .unwrap(),
                    CardPlay::Entered(_)
                ),
                "the real four-PP panther enters first"
            );
            let q = awaiting(trigger(&mut board, printing, 2));
            resolved(select(&mut board, &q, "CP03-037"));
            checks(&mut board);
            let view = board.projection(View::Referee).unwrap();
            assert!(
                keyword(&view, "source", "突進"),
                "the panther retains its printed Rush"
            );
            assert_eq!(
                view["objects"]["found"]["power"],
                if full { 2_u8 } else { 3_u8 },
                "only a successful new field generation gets plus one attack"
            );
            assert_eq!(
                view["objects"]["found"]["hp"], 3_u8,
                "the printed instruction does not add life"
            );
            assert_eq!(
                view["objects"]["found"]["generation"],
                u8::from(!full),
                "a failed entry does not change generation"
            );
            assert_eq!(
                view["P1"]["field"]
                    .as_array()
                    .unwrap()
                    .contains(&json!("found")),
                !full,
                "full fields leave the chosen card in the cemetery"
            );
            assert_eq!(
                view["P1"]["pp"]["current"], 0_u8,
                "only the printed play cost is paid"
            );
        }
    }
}

#[test]
fn both_blessings_use_current_cost_and_the_real_revived_blaster_keeps_ward_through_evolution() {
    for printing in ["BP01-115", "PR-012"] {
        let mut initial = position(printing, 6);
        initial["players"]["P1"]["zones"]["cemetery"] = json!([
            {"id":"found","card":"CSD03a-003","state":{"cost":8_u8}},
            {"id":"too-expensive-now","card":"CP03-037","state":{"cost":9_u8}},
            {"id":"spell","card":"CSD03a-007"}
        ]);
        initial["players"]["P1"]["zones"]["evolve_deck"] =
            json!([{"id":"blade-form","card":"CSD03a-004"}]);
        let mut board = game(&initial);
        let q = awaiting(
            board
                .play_internal_card("source", PlayerId { seat: 1 })
                .unwrap(),
        );
        resolved(select(&mut board, &q, "CSD03a-003"));
        let before = board.projection(View::Referee).unwrap();
        assert!(
            keyword(&before, "found", "守護"),
            "the actual revived generation receives Ward"
        );
        assert!(
            before["P1"]["cemetery"]
                .as_array()
                .unwrap()
                .contains(&json!("source")),
            "the spell finishes in the cemetery before the revived follower's fanfare, Q247"
        );
        blaster_evolves(&mut board);
        let view = board.projection(View::Referee).unwrap();
        assert!(
            keyword(&view, "found", "守護"),
            "evolution retains the granted Ward, Q272"
        );
        assert_eq!(
            view["objects"]["found"]["power"], 4_u8,
            "the real evolved Blaster's printed attack replaces the base"
        );
        assert_eq!(
            view["objects"]["found"]["generation"], 1_u8,
            "evolution does not create another movement generation"
        );
        assert_eq!(
            view["objects"]["enemy"]["hp"], 18_u8,
            "its real evolution ability deals twice the one Royal Paladin follower"
        );
        assert_eq!(
            view["P1"]["pp"]["current"], 0_u8,
            "the spell costs six and the ability evolution has no intrinsic payment"
        );
    }
}

fn alfred_position(base: &str, form: &str, boost: bool, full: bool) -> Value {
    let mut initial = position(base, 2);
    initial["players"]["P1"]["zones"]["hand"] = json!([]);
    initial["players"]["P1"]["zones"]["field"] = json!([{"id":"source","card":base}]);
    if full {
        for n in 0_u8..4 {
            initial["players"]["P1"]["zones"]["field"]
                .as_array_mut()
                .unwrap()
                .push(json!({"id":format!("old-{n}"),"card":"CP03-037"}));
        }
    }
    let mut grave = (0_u8..14)
        .map(|n| json!({"id":format!("grave-{n}"),"card":"CP03-037"}))
        .collect::<Vec<_>>();
    if boost {
        grave.push(json!({"id":"fifteenth-spell","card":"CSD03a-007"}));
    }
    initial["players"]["P1"]["zones"]["cemetery"] = json!(grave);
    initial["players"]["P1"]["zones"]["deck"] = json!([
        {"id":"found","card":"CSD03a-003"},
        {"id":"wrong-trait","card":"CP03-013"},
        {"id":"spell","card":"CSD03a-007"},
        {"id":"expensive","card":"CP03-025","state":{"cost":1_u8}}
    ]);
    initial["players"]["P1"]["zones"]["evolve_deck"] = json!([
        {"id":"alfred-form","card":form},
        {"id":"blade-form","card":"CSD03a-004"},
        {"id":"wrong-form","card":"CP03-026"}
    ]);
    initial
}

fn alfred_source(view: &Value, boost: bool) {
    assert_eq!(
        view["objects"]["source"]["power"],
        if boost { 5_u8 } else { 4_u8 },
        "fourteen followers plus one trait spell meets the fifteen-card condition, even with no entry, Q1456"
    );
    assert_eq!(
        view["objects"]["source"]["hp"],
        if boost { 5_u8 } else { 4_u8 },
        "the source receives both printed stats independently of search success"
    );
    for name in ["疾走", "ツインドライブ"] {
        assert!(
            keyword(view, "source", name),
            "the real Alfred form retains {name}"
        );
    }
}

fn evolve_alfred(board: &mut Game) {
    let before = board.projection(View::Referee).unwrap();
    let denied = board.internal_rule_action(&json!({"do":"evolve","source":"source","evolve_card":"wrong-form","pay":{"pp":2_u8}})).unwrap();
    assert_eq!(denied.outcome, "cannot-evolve", "an unrelated named form is refused through the rule-action outcome");
    assert!(denied.events.is_empty(), "the refused evolution produces no events");
    assert_eq!(board.projection(View::Referee).unwrap(), before, "the differently named resource cannot pay or evolve Alfred");
    let performed = board.internal_rule_action(&json!({"do":"evolve","source":"source","evolve_card":"alfred-form","pay":{"pp":2_u8}})).unwrap();
    assert_eq!(performed.outcome, "resolved", "the real named form evolves normally for the printed two PP");
}

fn alfred_case(base: &str, form: &str, boost: bool, take: bool, full: bool) {
    let mut board = game(&alfred_position(base, form, boost, full));
    evolve_alfred(&mut board);
    let q = awaiting(trigger(&mut board, form, 2));
    let InputDomain::Search(domain) = &q.domain else {
        panic!("the actual Royal Paladin search");
    };
    assert_eq!(
        domain.cards.len(),
        1,
        "only an original-cost-at-most-three Royal Paladin follower is eligible"
    );
    assert_eq!(
        domain.cards[0].card, "CSD03a-003",
        "wrong trait, spell and cheapened printed cost five are excluded"
    );
    assert!(
        board.internal_decision(View::P2).is_none(),
        "the private search does not expose the deck"
    );
    resolved(
        board
            .answer_internal_card(
                q.actor,
                q.token,
                Answer::Search(SearchAnswer::Ungrouped(
                    domain
                        .cards
                        .iter()
                        .take(usize::from(take))
                        .map(|c| c.handle)
                        .collect(),
                )),
            )
            .unwrap(),
    );
    let entered = take && !full;
    let entered_view = board.projection(View::Referee).unwrap();
    alfred_source(&entered_view, boost);
    assert_eq!(
        entered_view["P1"]["field"]
            .as_array()
            .unwrap()
            .contains(&json!("found")),
        entered,
        "the follow collection contains only actual entries"
    );
    if entered {
        assert_eq!(
            entered_view["objects"]["found"]["power"],
            if boost { 4_u8 } else { 3_u8 },
            "the actual new follower receives the conditional buff before its fanfare"
        );
        blaster_evolves(&mut board);
        let evolved_view = board.projection(View::Referee).unwrap();
        assert_eq!(
            evolved_view["objects"]["found"]["power"],
            if boost { 5_u8 } else { 4_u8 },
            "the actual Blaster evolution retains the generation's attack buff"
        );
        assert_eq!(
            evolved_view["objects"]["found"]["hp"],
            if boost { 5_u8 } else { 4_u8 },
            "the generation's life buff also survives evolution"
        );
        assert_eq!(
            evolved_view["objects"]["found"]["generation"], 1_u8,
            "ability evolution preserves the actual field-entry generation"
        );
        assert_eq!(
            evolved_view["objects"]["enemy"]["hp"], 16_u8,
            "two real Royal Paladin followers cause four damage"
        );
    } else {
        checks(&mut board);
    }
    let completed_view = board.projection(View::Referee).unwrap();
    assert_eq!(
        completed_view["P1"]["pp"]["current"], 0_u8,
        "Alfred's ordinary evolution costs exactly two PP, and the later effect evolution costs none"
    );
}

// Every printing pair needs both grave thresholds and search outcomes; the selection fixture is shared.
#[test]
fn all_alfred_bases_and_forms_search_then_buff_the_real_entry_and_the_source_independently() {
    for base in ["CSD03a-001", "CSD03a-SL01"] {
        for form in ["CSD03a-002", "CSD03a-SL02"] {
            for boost in [false, true] {
                for take in [false, true] {
                    alfred_case(base, form, boost, take, false);
                }
            }
        }
    }
    alfred_case("CSD03a-001", "CSD03a-002", true, true, true);
}
