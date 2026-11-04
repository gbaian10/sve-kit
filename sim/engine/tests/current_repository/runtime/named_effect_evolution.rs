#![expect(
    clippy::panic,
    reason = "Unexpected decisions invalidate these complete printed evolution scenarios."
)]
use super::*;
use sve_engine::game::internal::DecisionView;

fn game(initial: &Value) -> Game {
    Game::from_selected_effects(
        selection_with_rules().clone(),
        initial,
        &Value::Null,
        &Value::Null,
        "named-effect-evolution",
    )
    .unwrap()
}

fn question(play: CardPlay) -> DecisionView {
    let CardPlay::Awaiting(q) = play else {
        panic!("printed private choice")
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
    let option = options
        .iter()
        .find(|o| o.card == card && o.line == line)
        .unwrap();
    board
        .answer_internal_card(q.actor, q.token, Answer::Pending(option.handle))
        .unwrap()
}

fn finish(board: &mut Game, mut play: CardPlay) {
    while let CardPlay::Awaiting(q) = play {
        let answer = match &q.domain {
            InputDomain::Targets(d) => {
                let count = d.cardinality.maximum.map_or(d.members.len(), |n| {
                    usize::try_from(n.0).unwrap().min(d.members.len())
                });
                Answer::Targets(d.members.iter().take(count).map(|m| m.handle).collect())
            }
            InputDomain::Order(cards) => {
                Answer::Order(cards.iter().rev().map(|c| c.handle).collect())
            }
            InputDomain::WardEntry { .. } => Answer::WardEntry(false),
            InputDomain::ResourceEntryPayment { .. }
            | InputDomain::Quick { .. }
            | InputDomain::BeforeDraw
            | InputDomain::Pending(_)
            | InputDomain::TriggerQuota(_)
            | InputDomain::Integer(_)
            | InputDomain::DamageReplacements(_)
            | InputDomain::DriveTrigger { .. }
            | InputDomain::Search(_)
            | InputDomain::SearchPartition(_)
            | InputDomain::Distribution(_)
            | InputDomain::SearchReplacement
            | InputDomain::TokenCapacity { .. }
            | InputDomain::Modes { .. }
            | InputDomain::OptionalCost
            | InputDomain::Abilities(_)
            | InputDomain::CostMaterials(_) => panic!("unexpected printed choice: {:?}", q.domain),
        };
        play = board
            .answer_internal_card(q.actor, q.token, answer)
            .unwrap();
    }
}

fn settle(board: &mut Game) {
    loop {
        if board.internal_card_stage() == Some(ResumeStage::CheckTiming) {
            board.resume_internal_card().unwrap();
        }
        let Some(q) = board.internal_pending_choice().unwrap() else {
            break;
        };
        let InputDomain::Pending(options) = &q.domain else {
            panic!("printed pending ability")
        };
        let play = board
            .answer_internal_card(q.actor, q.token, Answer::Pending(options[0].handle))
            .unwrap();
        finish(board, play);
    }
}

fn enter(board: &mut Game) {
    let play = board
        .play_internal_card("source", PlayerId { seat: 1 })
        .unwrap();
    assert!(matches!(play, CardPlay::Entered(_)), "the real base enters");
}

fn arif_position(base: &str, form: &str) -> Value {
    let mut initial = draw_position();
    initial["players"]["P1"]["ep"] = json!(1_u8);
    initial["players"]["P1"]["sep"] = json!(1_u8);
    initial["players"]["P1"]["zones"]["hand"] = json!([
        {"id":"source","card":base},{"id":"cost-spare","card":"CSD03b-006"}
    ]);
    initial["players"]["P2"]["zones"]["field"] = json!([{"id":"enemy","card":"BP01-037"}]);
    initial["players"]["P1"]["zones"]["field"] = json!([{"id":"ordinary","card":"CSD03b-001"}]);
    initial["players"]["P1"]["zones"]["cemetery"] = json!([
        {"id":"bar","card":"CSD03b-009"},{"id":"tar","card":"CSD03b-012"}
    ]);
    initial["players"]["P1"]["zones"]["evolve_deck"] = json!([
        {"id":"wrong-form","card":"CP03-002"},{"id":"form","card":form},
        {"id":"ordinary-form","card":"CSD03b-002"}
    ]);
    initial["players"]["P1"]["zones"]["deck"] = json!([
        {"id":"excluded","card":base},{"id":"picked","card":"CSD03b-006"},
        {"id":"wrong-trait","card":"BP01-037"},{"id":"tail","card":"BP01-037"}
    ]);
    initial
}

fn arif_evolve(board: &mut Game, base: &str, form: &str) {
    enter(board);
    let q = question(trigger(board, base, 2));
    let InputDomain::Targets(d) = &q.domain else {
        panic!("named evolution resource")
    };
    assert_eq!(d.members.len(), 1, "the other named form is excluded");
    assert_eq!(
        d.members[0].card.as_deref(),
        Some(form),
        "the true Victory form is offered"
    );
    assert!(
        board.internal_decision(View::P2).is_none(),
        "the deck is private"
    );
    let result = resolved(
        board
            .answer_internal_card(q.actor, q.token, Answer::Targets(vec![d.members[0].handle]))
            .unwrap(),
    );
    assert_eq!(
        result
            .progress
            .facts
            .iter()
            .filter(|f| matches!(f, Fact::Evolved(_)))
            .count(),
        1,
        "the real fanfare produces one evolution event"
    );
    let view = board.projection(View::Referee).unwrap();
    assert_eq!(
        view["objects"]["source"]["evolved_with"], "form",
        "the named form is associated"
    );
    assert_eq!(
        view["P1"]["pp"]["current"], 7_u8,
        "only the base play costs PP"
    );
    assert_eq!(view["P1"]["ep"], 1_u8, "effect evolution spends no EP");
    assert_eq!(view["P1"]["sep"], 1_u8, "effect evolution spends no SEP");
}

fn arif_look(board: &mut Game, form: &str, take: bool) {
    let q = question(trigger(board, form, 2));
    let InputDomain::Targets(d) = &q.domain else {
        panic!("real OnEvolve private Look")
    };
    assert_eq!(
        d.members.len(),
        1,
        "literal name and Kagero trait both filter the real top three"
    );
    assert_eq!(
        d.members[0].card.as_deref(),
        Some("CSD03b-006"),
        "Berserk Dragon alone qualifies"
    );
    assert!(
        board.internal_decision(View::P2).is_none(),
        "Look candidates remain private"
    );
    let play = board
        .answer_internal_card(
            q.actor,
            q.token,
            Answer::Targets(if take {
                vec![d.members[0].handle]
            } else {
                vec![]
            }),
        )
        .unwrap();
    finish(board, play);
    settle(board);
    let view = board.projection(View::Referee).unwrap();
    assert_eq!(
        view["P1"]["ex"],
        if take { json!(["picked"]) } else { json!([]) },
        "the optional choice controls actual EX movement"
    );
    assert_eq!(
        view["P1"]["deck"],
        if take {
            json!(["tail", "wrong-trait", "excluded"])
        } else {
            json!(["tail", "wrong-trait", "picked", "excluded"])
        },
        "the chooser orders every unselected observed card below the unseen tail"
    );
}

fn play_picked(board: &mut Game, cost: u64) {
    let before = board.projection(View::Referee).unwrap();
    assert!(
        before["P1"]["hand"]
            .as_array()
            .unwrap()
            .contains(&json!("cost-spare")),
        "a real Kagero card is available for the optional discard"
    );
    let play = board
        .play_internal_card("picked", PlayerId { seat: 1 })
        .unwrap();
    assert!(
        matches!(play, CardPlay::Entered(_)),
        "the actual selected EX follower enters"
    );
    let q = question(trigger(board, "CSD03b-006", 3));
    assert_eq!(
        q.domain,
        InputDomain::OptionalCost,
        "Berserk Dragon offers its printed optional discard"
    );
    assert_eq!(
        board
            .answer_internal_card(q.actor, q.token, Answer::OptionalCost(false))
            .unwrap(),
        CardPlay::Cancelled,
        "declining the real fanfare cancels its cost and damage"
    );
    settle(board);
    let after = board.projection(View::Referee).unwrap();
    assert_eq!(
        after["P1"]["hand"], before["P1"]["hand"],
        "declining retains every hand card despite an eligible discard"
    );
    assert_eq!(
        after["objects"]["enemy"], before["objects"]["enemy"],
        "the declined fanfare deals no damage to the opposing follower"
    );
    assert_eq!(
        after["P1"]["pp"]["current"],
        before["P1"]["pp"]["current"]
            .as_u64()
            .unwrap()
            .checked_sub(cost)
            .unwrap(),
        "only the current play price is paid"
    );
    assert!(
        after["P1"]["field"]
            .as_array()
            .unwrap()
            .contains(&json!("picked")),
        "the actual moved EX follower stays on the field"
    );
}

#[test]
fn arif_all_printings_evolve_by_fanfare_then_the_real_victory_form_moves_and_discounts() {
    for (base, form) in [("CSD03b-004", "CSD03b-005"), ("CSD03b-P01", "CSD03b-P02")] {
        let mut board = game(&arif_position(base, form));
        arif_evolve(&mut board, base, form);
        arif_look(&mut board, form, true);
        play_picked(&mut board, 0);
        assert_eq!(board.internal_rule_action(&json!({"do":"evolve","source":"ordinary","evolve_card":"ordinary-form","pay":{"pp":1_u8}})).unwrap().outcome,"resolved","effect evolution leaves the normal evolution quota unused");
        settle(&mut board);
        assert_eq!(
            board.projection(View::Referee).unwrap()["P1"]["pp"]["current"],
            6_u8,
            "ordinary evolution still pays its printed one PP"
        );
    }
}

#[test]
fn arif_needs_both_grave_names_and_an_available_explicit_destination() {
    for missing in ["bar", "tar", "form"] {
        let mut initial = arif_position("CSD03b-004", "CSD03b-005");
        let zone = if missing == "form" {
            "evolve_deck"
        } else {
            "cemetery"
        };
        initial["players"]["P1"]["zones"][zone]
            .as_array_mut()
            .unwrap()
            .retain(|o| o["id"] != missing);
        let mut board = game(&initial);
        enter(&mut board);
        let play = trigger(&mut board, "CSD03b-004", 2);
        if missing == "form" {
            let q = question(play);
            let InputDomain::Targets(d) = &q.domain else {
                panic!("empty private form choice")
            };
            assert!(
                d.members.is_empty(),
                "other form names never serve as fallback"
            );
            resolved(
                board
                    .answer_internal_card(q.actor, q.token, Answer::Targets(vec![]))
                    .unwrap(),
            );
        } else {
            resolved(play);
        }
        settle(&mut board);
        let view = board.projection(View::Referee).unwrap();
        assert_ne!(
            view["objects"]["source"]["evolved"], true,
            "each missing requirement prevents effect evolution"
        );
        assert_eq!(
            view["P1"]["pp"]["current"], 7_u8,
            "a failed fanfare pays no evolution cost"
        );
        assert_eq!(
            view["P1"]["ex"],
            json!([]),
            "the true form's OnEvolve never fires"
        );
    }
}

fn next_turn(board: &mut Game) {
    board
        .internal_rule_action(&json!({"do":"end-phase"}))
        .unwrap();
    let q = board.internal_quick_choice().unwrap().unwrap();
    board
        .answer_internal_card(q.actor, q.token, Answer::Quick(None))
        .unwrap();
    settle(board);
}

#[test]
fn victory_arif_optional_look_and_discount_expiry_preserve_normal_evolution() {
    for (base, form) in [("CSD03b-004", "CSD03b-005"), ("CSD03b-P01", "CSD03b-P02")] {
        for take in [false, true] {
            let mut initial = arif_position(base, form);
            initial["players"]["P1"]["zones"]["field"] = json!([{"id":"source","card":base}]);
            initial["players"]["P1"]["zones"]["hand"] =
                json!([{"id":"cost-spare","card":"CSD03b-006"}]);
            initial["players"]["P2"]["zones"]["deck"] =
                json!([{"id":"other-top","card":"BP01-037"}]);
            let mut board = game(&initial);
            assert_eq!(board.internal_rule_action(&json!({"do":"evolve","source":"source","evolve_card":"form","pay":{"pp":1_u8}})).unwrap().outcome,"resolved","the base's ordinary named evolution remains available");
            assert_eq!(
                board.projection(View::Referee).unwrap()["P1"]["pp"]["current"],
                9_u8,
                "ordinary evolution pays one PP"
            );
            arif_look(&mut board, form, take);
            if !take {
                attack(&mut board, "source");
                combat(&mut board);
                let view = board.projection(View::Referee).unwrap();
                assert_eq!(
                    view["P1"]["deck"],
                    json!(["picked", "excluded", "tail", "wrong-trait"]),
                    "the real Victory form's TwinDrive checks and rotates exactly two cards"
                );
                assert_eq!(
                    view["P2"]["leader"]["life"], 16_u8,
                    "the evolved follower attacks with its true four power"
                );
            }
            if take {
                next_turn(&mut board);
                next_turn(&mut board);
                play_picked(&mut board, 3);
            }
        }
    }
}

fn maelstrom_position(base: &str, earlier: usize, wrong_trait: bool) -> Value {
    let mut initial = draw_position();
    initial["players"]["P2"]["leader"]["life"] = json!(100_u8);
    let mut field = vec![json!({"id":"source","card":base})];
    field.extend((0..earlier).map(|n|json!({"id":format!("prior-{n}"),"card":if wrong_trait && n==0 {"BP01-037"} else {"CP03-013"}})));
    initial["players"]["P1"]["zones"]["field"] = json!(field);
    initial["players"]["P1"]["zones"]["evolve_deck"] =
        json!([{"id":"form","card":"CP03-002"},{"id":"wrong-form","card":"CSD03b-005"}]);
    initial["players"]["P1"]["zones"]["deck"] = json!([
        {"id":"first","card":"BP01-037"},{"id":"second","card":"BP01-037"},
        {"id":"third","card":"BP01-037"},{"id":"fourth","card":"BP01-037"}
    ]);
    initial["players"]["P2"]["zones"]["field"] = json!([{"id":"enemy","card":"BP01-037"}]);
    initial
}

fn attack(board: &mut Game, id: &str) {
    assert_eq!(
        board
            .internal_rule_action(&json!({"do":"attack","attacker":id,"target":"P2.leader"}))
            .unwrap()
            .outcome,
        "resolved",
        "the real attack declaration creates history"
    );
}

fn combat(board: &mut Game) {
    settle(board);
    if let Some(q) = board.internal_quick_choice().unwrap() {
        let play = board
            .answer_internal_card(q.actor, q.token, Answer::Quick(None))
            .unwrap();
        finish(board, play);
        settle(board);
    }
}

#[test]
fn maelstrom_all_printings_fourth_aqua_attack_evolves_in_either_twin_drive_order() {
    for base in ["CP03-001", "CP03-SL01", "CP03-U01"] {
        for first in [2_u64, 3] {
            let mut board = game(&maelstrom_position(base, 3, false));
            for n in 0_usize..3 {
                attack(&mut board, &format!("prior-{n}"));
                combat(&mut board);
            }
            let before = board.projection(View::Referee).unwrap()["P2"]["leader"]["life"]
                .as_u64()
                .unwrap();
            attack(&mut board, "source");
            let play = trigger(&mut board, base, first);
            finish(&mut board, play);
            combat(&mut board);
            let view = board.projection(View::Referee).unwrap();
            assert_eq!(
                view["objects"]["source"]["evolved_with"], "form",
                "exactly the fourth Aqua attack selects real Glory Maelstrom"
            );
            assert_eq!(
                view["objects"]["source"]["power"], 4_u8,
                "the true form supplies its printed stats"
            );
            assert_eq!(
                view["P2"]["leader"]["life"],
                before - 4,
                "combat uses evolved power after either pending order"
            );
            assert_eq!(
                view["P2"]["cemetery"],
                json!(["enemy"]),
                "the real Glory dealt-damage trigger destroys an opposing follower"
            );
            assert_eq!(
                view["P1"]["hand"],
                json!(["third", "fourth"]),
                "Glory draws two after the original TwinDrive rotates the first two"
            );
            assert_eq!(
                view["P1"]["pp"]["current"], 10_u8,
                "effect evolution spends no ordinary four PP"
            );
        }
    }
}

#[test]
fn maelstrom_rejects_third_fifth_wrong_trait_and_missing_named_form() {
    for (earlier, wrong_trait, missing) in [
        (2_usize, false, false),
        (4, false, false),
        (3, true, false),
        (3, false, true),
    ] {
        let mut initial = maelstrom_position("CP03-001", earlier, wrong_trait);
        if missing {
            initial["players"]["P1"]["zones"]["evolve_deck"]
                .as_array_mut()
                .unwrap()
                .retain(|o| o["id"] != "form");
        }
        let mut board = game(&initial);
        for n in 0..earlier {
            attack(&mut board, &format!("prior-{n}"));
            combat(&mut board);
        }
        attack(&mut board, "source");
        combat(&mut board);
        let view = board.projection(View::Referee).unwrap();
        assert_ne!(
            view["objects"]["source"]["evolved"], true,
            "only the fourth own Aqua attack with the explicit form can evolve"
        );
        assert_eq!(
            view["objects"]["source"]["power"], 3_u8,
            "failed effect evolution retains the base stats"
        );
        assert_eq!(
            view["P1"]["hand"],
            json!([]),
            "the absent true form cannot supply a damage trigger"
        );
    }
}

#[test]
fn maelstrom_ordinary_named_evolution_pays_four_pp_for_each_printing() {
    for base in ["CP03-001", "CP03-SL01", "CP03-U01"] {
        let mut board = game(&maelstrom_position(base, 0, false));
        assert_eq!(
            board
                .internal_rule_action(
                    &json!({"do":"evolve","source":"source","evolve_card":"form","pay":{"pp":4_u8}})
                )
                .unwrap()
                .outcome,
            "resolved",
            "the complete base retains ordinary named evolution"
        );
        assert_eq!(
            board.projection(View::Referee).unwrap()["P1"]["pp"]["current"],
            6_u8,
            "ordinary evolution pays four PP"
        );
        assert_eq!(
            board.projection(View::Referee).unwrap()["objects"]["source"]["evolved_with"],
            "form",
            "ordinary evolution uses the same real named destination"
        );
    }
}
