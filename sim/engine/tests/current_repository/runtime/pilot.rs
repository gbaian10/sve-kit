#![expect(
    clippy::panic,
    reason = "Unexpected questions invalidate the printed pilot scenarios."
)]
use super::event_cemetery::finish_checks;
use super::*;
use sve_engine::game::internal::SearchAnswer;

const fn actor() -> PlayerId {
    PlayerId { seat: 1 }
}

fn board(initial: &Value) -> Game {
    Game::from_selected_effects(
        selection_with_rules().clone(),
        initial,
        &Value::Null,
        &Value::Null,
        "pilot-real",
    )
    .unwrap()
}

fn instance(board: &mut Game, vehicle: &str, line: u64) -> AbilityInstanceId {
    let state = board.projection(View::Referee).unwrap();
    let printing = state["objects"][vehicle]["card"].as_str().unwrap();
    let definition = selection().current[printing].program.card().abilities.iter().find(|ability| {
        matches!(&ability.node.source.origin, AbilityOrigin::Printed { line: printed, .. } if printed.0 == line)
            && ability.node.kind == AbilityKind::Activated
    }).unwrap().node.id;
    board
        .internal_activated_sources(vehicle)
        .unwrap()
        .into_iter()
        .find(|source| source.definition == definition)
        .unwrap()
        .instance
}

fn activate(board: &mut Game, vehicle: &str, line: u64, count: usize) {
    finish_checks(board);
    let ability = instance(board, vehicle, line);
    let CardPlay::Awaiting(question) = board
        .activate_internal_card_instance(vehicle, ability, actor())
        .unwrap()
    else {
        panic!("printed act materials");
    };
    let InputDomain::CostMaterials(domain) = &question.domain else {
        panic!("act material domain");
    };
    assert_eq!(
        domain.members.len(),
        count,
        "CR10.4.3 excludes all opposing materials"
    );
    assert!(
        domain.members.iter().all(|member| member.player == actor()),
        "only the controller pays act materials"
    );
    resolved(
        board
            .answer_internal_card(
                question.actor,
                question.token,
                Answer::CostMaterials(domain.members.iter().map(|member| member.handle).collect()),
            )
            .unwrap(),
    );
    finish_checks(board);
}

#[test]
fn all_vehicle_printings_pay_both_own_material_costs_and_become_printed_token_followers() {
    for printing in ["BP11-T01", "BP11-P29"] {
        for line in [1_u64, 2] {
            let mut initial = draw_position();
            initial["players"]["P1"]["zones"]["field"] = json!([
                {"id":"vehicle","card":printing}, {"id":"bunny","card":"BP11-019"},
                {"id":"ally","card":"BP01-037"}]);
            initial["players"]["P2"]["zones"]["field"] = json!([
                {"id":"opponent-bunny","card":"BP11-019"}, {"id":"enemy","card":"BP01-037"}]);
            let mut board = board(&initial);
            activate(&mut board, "vehicle", line, if line == 1 { 1 } else { 2 });
            let state = board.projection(View::Referee).unwrap();
            assert_eq!(
                state["objects"]["vehicle"]["card_type"],
                "フォロワー・トークン"
            );
            assert_eq!(state["objects"]["vehicle"]["power"], 3_u8);
            assert_eq!(state["objects"]["vehicle"]["hp"], 3_u8);
            assert_eq!(state["objects"]["bunny"]["acted"], true);
            assert_eq!(state["objects"]["ally"]["acted"], line == 2);
            assert_eq!(state["objects"]["opponent-bunny"]["acted"], false);
            assert_eq!(
                state["P1"]["pp"]["current"],
                if line == 1 { 10_u8 } else { 9 }
            );
        }
    }
}

#[test]
fn insufficient_pilot_materials_preserve_pp_and_board_for_all_printings() {
    for printing in ["BP11-T01", "BP11-P29"] {
        let mut initial = draw_position();
        initial["players"]["P1"]["zones"]["field"] = json!([{ "id":"vehicle", "card":printing }]);
        initial["players"]["P2"]["zones"]["field"] = json!([
            {"id":"enemy-bunny","card":"BP11-019"}, {"id":"enemy","card":"BP01-037"}]);
        let mut board = board(&initial);
        for line in [1_u64, 2] {
            let ability = instance(&mut board, "vehicle", line);
            let before = board.projection(View::Referee).unwrap();
            board
                .activate_internal_card_instance("vehicle", ability, actor())
                .unwrap_err();
            assert_eq!(board.projection(View::Referee).unwrap(), before);
        }
    }
}

#[test]
fn both_bunny_evolved_printings_resolve_each_printed_mode_and_created_vehicle_attacks_with_storm() {
    for (base, form) in [("BP11-019", "BP11-020"), ("BP11-SL05", "BP11-SL06")] {
        for mode in [0_usize, 1] {
            let mut initial = draw_position();
            initial["players"]["P1"]["zones"]["hand"] = json!([{"id":"bunny","card":base}]);
            initial["players"]["P1"]["zones"]["evolve_deck"] = json!([{"id":"form","card":form}]);
            initial["players"]["P1"]["zones"]["deck"] = json!([
                {"id":"shot","card":"BP11-028"}, {"id":"spare","card":"BP01-037"}]);
            initial["players"]["P2"]["zones"]["field"] =
                json!([{"id":"opponent-bunny","card":base}]);
            let mut board = board(&initial);
            assert!(matches!(
                board.play_internal_card("bunny", actor()).unwrap(),
                CardPlay::Entered(_)
            ));
            finish_checks(&mut board);
            evolve_mode(&mut board, form, mode);
            let state = board.projection(View::Referee).unwrap();
            assert_eq!(
                state["P1"]["pp"]["current"], 6_u8,
                "printed three PP plus one PP evolution"
            );
            if mode == 1 {
                assert_eq!(state["P1"]["hand"], json!(["shot"]));
                assert_eq!(state["P1"]["field"], json!(["bunny"]));
                continue;
            }
            let vehicle = state["P1"]["field"]
                .as_array()
                .unwrap()
                .iter()
                .filter_map(Value::as_str)
                .find(|id| *id != "bunny")
                .unwrap()
                .to_owned();
            assert!(
                state["objects"][&vehicle]["card_type"].is_null(),
                "created amulet still uses its printed kind"
            );
            assert_eq!(state["objects"][&vehicle]["entered_this_turn"], true);
            activate(&mut board, &vehicle, 1, 1);
            board
                .internal_rule_action(
                    &json!({"do":"attack","attacker":vehicle,"target":"P2.leader"}),
                )
                .unwrap();
            let question = board.internal_pending_choice().unwrap().unwrap();
            let InputDomain::Pending(options) = &question.domain else {
                panic!("printed vehicle attack trigger");
            };
            assert_eq!(options.len(), 1);
            resolved(
                board
                    .answer_internal_card(
                        question.actor,
                        question.token,
                        Answer::Pending(options[0].handle),
                    )
                    .unwrap(),
            );
            finish_checks(&mut board);
            assert_eq!(
                board.projection(View::Referee).unwrap()["P1"]["hand"],
                json!(["shot"]),
                "other own Wasteland follower enables the actual attack draw"
            );
        }
    }
}

#[test]
fn mirror_summon_targets_a_piloted_vehicle_but_cannot_create_a_follower_from_its_amulet_name() {
    for spell in ["BP22-055", "BP22-P31"] {
        let mut initial = draw_position();
        initial["players"]["P1"]["zones"]["field"] = json!([
            {"id":"vehicle","card":"BP11-T01"}, {"id":"bunny","card":"BP11-019"}]);
        initial["players"]["P1"]["zones"]["hand"] = json!([{"id":"spell","card":spell}]);
        let mut board = board(&initial);
        activate(&mut board, "vehicle", 1, 1);
        let before = board.projection(View::Referee).unwrap();
        let CardPlay::Awaiting(question) = board.play_internal_card("spell", actor()).unwrap()
        else {
            panic!("the piloted vehicle is a legal printed target");
        };
        let InputDomain::Targets(domain) = &question.domain else {
            panic!("printed token follower target");
        };
        assert_eq!(domain.members.len(), 1);
        resolved(
            board
                .answer_internal_card(
                    question.actor,
                    question.token,
                    Answer::Targets(vec![domain.members[0].handle]),
                )
                .unwrap(),
        );
        finish_checks(&mut board);
        let after = board.projection(View::Referee).unwrap();
        assert_eq!(
            after["P1"]["field"], before["P1"]["field"],
            "CR9.1.2.3 / 1.3.2: no follower token has this printed name"
        );
        assert_eq!(
            after["P1"]["cemetery"],
            json!(["spell"]),
            "a legal spell still resolves normally"
        );
        assert_eq!(
            after["objects"]["vehicle"]["card_type"],
            "フォロワー・トークン"
        );
        assert_eq!(after["objects"]["vehicle"]["power"], 3_u8);
        assert_eq!(after["objects"]["vehicle"]["hp"], 3_u8);
    }
}

fn evolve_mode(board: &mut Game, form: &str, mode: usize) {
    let evolved = board
        .internal_rule_action(
            &json!({"do":"evolve","source":"bunny","evolve_card":"form","pay":{"pp":1_u8}}),
        )
        .unwrap();
    assert_eq!(
        evolved.outcome, "resolved",
        "the printed one PP evolution completes"
    );
    finish_checks(board);
    let question = board.internal_pending_choice().unwrap().unwrap();
    let InputDomain::Pending(options) = &question.domain else {
        panic!("printed evolution trigger");
    };
    assert_eq!(
        options.len(),
        1,
        "one evolved source creates one pending trigger"
    );
    assert_eq!(
        options[0].card, form,
        "the real evolved printing supplies the trigger"
    );
    let CardPlay::Awaiting(choice) = board
        .answer_internal_card(
            question.actor,
            question.token,
            Answer::Pending(options[0].handle),
        )
        .unwrap()
    else {
        panic!("printed either");
    };
    let InputDomain::Modes { modes, .. } = &choice.domain else {
        panic!("mode choices");
    };
    assert_eq!(modes.len(), 2, "both printed alternatives are offered");
    let mut result = board
        .answer_internal_card(choice.actor, choice.token, Answer::Modes(vec![modes[mode]]))
        .unwrap();
    if mode == 1 {
        let CardPlay::Awaiting(search) = result else {
            panic!("real deck search");
        };
        let InputDomain::Search(domain) = &search.domain else {
            panic!("deck search domain");
        };
        assert_eq!(
            domain.cards.len(),
            1,
            "only Desperado Shot matches the printed name"
        );
        result = board
            .answer_internal_card(
                search.actor,
                search.token,
                Answer::Search(SearchAnswer::Ungrouped(vec![domain.cards[0].handle])),
            )
            .unwrap();
    }
    resolved(result);
    finish_checks(board);
}
