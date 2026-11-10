#![expect(
    clippy::panic,
    clippy::wildcard_enum_match_arm,
    reason = "Unexpected choices invalidate the complete printed Tokyo light scenarios."
)]
use super::*;

fn initial() -> Value {
    let mut value = draw_position();
    value["turn"]["elapsed_turns"] = json!({"P1":7_u8,"P2":6_u8});
    value["players"]["P1"]["zones"]["hand"] = json!([]);
    value["players"]["P1"]["zones"]["deck"] = json!([
        {"id":"drawn","card":"BP01-171"},{"id":"tail","card":"BP01-171"}
    ]);
    value["players"]["P2"]["zones"]["field"] = json!([
        {"id":"enemy","card":"BP01-171","state":{"hp":20_u8,"max_hp":20_u8}}
    ]);
    value
}

fn board(initial: &Value) -> Game {
    Game::from_selected_effects(
        selection_with_rules().clone(),
        initial,
        &Value::Null,
        &Value::Null,
        "unlimited-light",
    )
    .unwrap()
}

fn checks(board: &mut Game) {
    while board.internal_card_stage() == Some(ResumeStage::CheckTiming) {
        board.resume_internal_card().unwrap();
    }
}

fn finish(board: &mut Game, mut play: CardPlay, leader: bool) {
    while let CardPlay::Awaiting(q) = play {
        let answer = match &q.domain {
            InputDomain::Targets(domain) => {
                assert!(
                    domain.members.iter().all(|member| {
                        member.player.seat != 1 || member.card.as_deref() != Some("BP01-171")
                    }),
                    "the own card without Tokyo is outside every printed target domain"
                );
                let choice = domain
                    .members
                    .iter()
                    .find(|member| member.player.seat == 2 && member.card.is_none() == leader)
                    .or_else(|| {
                        domain.members.iter().find(|member| {
                            matches!(
                                member.card.as_deref(),
                                Some("BP18-009" | "BP18-P05" | "BP18-010" | "BP18-P06")
                            )
                        })
                    });
                let handles = choice.map_or_else(
                    || {
                        let limit = domain
                            .cardinality
                            .maximum
                            .map_or(domain.members.len(), |n| usize::try_from(n.0).unwrap());
                        domain
                            .members
                            .iter()
                            .take(limit)
                            .map(|member| member.handle)
                            .collect()
                    },
                    |choice| vec![choice.handle],
                );
                Answer::Targets(handles)
            }
            InputDomain::WardEntry { .. } => Answer::WardEntry(false),
            other => panic!("unexpected Tokyo question {other:?}"),
        };
        play = board
            .answer_internal_card(q.actor, q.token, answer)
            .unwrap();
    }
    checks(board);
}

fn settle(board: &mut Game, leader: bool) {
    loop {
        checks(board);
        if let Some(q) = board.internal_pending_choice().unwrap() {
            let InputDomain::Pending(options) = &q.domain else {
                panic!("printed pending observer");
            };
            let play = board
                .answer_internal_card(q.actor, q.token, Answer::Pending(options[0].handle))
                .unwrap();
            finish(board, play, leader);
        } else if let Some(q) = board.internal_quick_choice().unwrap() {
            let play = board
                .answer_internal_card(q.actor, q.token, Answer::Quick(None))
                .unwrap();
            finish(board, play, leader);
        } else {
            return;
        }
    }
}

fn evolve(board: &mut Game, source: &str, form: &str, super_evolution: bool, leader: bool) {
    let pay = if super_evolution {
        json!({"pp":1_u8,"sep":1_u8})
    } else {
        json!({"pp":1_u8})
    };
    assert_eq!(
        board
            .internal_rule_action(
                &json!({"do":"evolve","source":source,"evolve_card":form,"pay":pay})
            )
            .unwrap()
            .outcome,
        "resolved",
        "the real intrinsic evolution pays its own PP"
    );
    settle(board, leader);
}

#[test]
fn both_rolo_bases_recover_only_an_own_tokyo_card_and_discount_its_actual_ex_play() {
    for number in ["BP18-001", "BP18-SL01"] {
        let mut value = initial();
        value["players"]["P1"]["zones"]["hand"] = json!([{"id":"source","card":number}]);
        value["players"]["P1"]["zones"]["cemetery"] = json!([
            {"id":"recovered","card":"BP18-005"},{"id":"wrong","card":"BP01-171"}
        ]);
        value["players"]["P2"]["zones"]["cemetery"] = json!([{"id":"foreign","card":"BP18-005"}]);
        let mut board = board(&value);
        let play = board
            .play_internal_card("source", PlayerId { seat: 1 })
            .unwrap();
        finish(&mut board, play, false);
        settle(&mut board, false);
        let recovered = board.projection(View::Referee).unwrap();
        assert_eq!(
            recovered["P1"]["ex"],
            json!(["recovered"]),
            "the exact formal cemetery target enters EX"
        );
        assert_eq!(
            recovered["P1"]["cemetery"],
            json!(["wrong"]),
            "wrong traits remain in the cemetery"
        );
        assert_eq!(
            recovered["P2"]["cemetery"],
            json!(["foreign"]),
            "the opponent's Tokyo card is not selected"
        );
        let recovered_play = board
            .play_internal_card("recovered", PlayerId { seat: 1 })
            .unwrap();
        finish(&mut board, recovered_play, false);
        settle(&mut board, false);
        let after = board.projection(View::Referee).unwrap();
        assert_eq!(
            after["P1"]["pp"]["current"], 6_u8,
            "the four-PP source pays four, then the actual two-PP recipient costs zero"
        );
        assert_eq!(
            after["objects"]["recovered"]["generation"], 3_u8,
            "the modifier follows the new EX generation into its actual play"
        );
    }
}

#[test]
fn all_rolo_forms_observe_both_evolutions_and_create_the_complete_sowing_spell() {
    for (base, form) in [
        ("BP18-001", "BP18-002"),
        ("BP18-SL01", "BP18-SL02"),
        ("BP18-001", "BP18-U01"),
    ] {
        let mut value = initial();
        value["players"]["P1"]["sep"] = json!(1_u8);
        value["players"]["P1"]["zones"]["field"] = json!([
            {"id":"source","card":base},{"id":"ally","card":"BP16-038"}
        ]);
        value["players"]["P1"]["zones"]["evolve_deck"] = json!([
            {"id":"form","card":form},{"id":"ally-form","card":"BP16-039"},
            {"id":"spent","card":"BP18-006","face_up":true}
        ]);
        value["players"]["P2"]["zones"]["evolve_deck"] =
            json!([{"id":"enemy-spent","card":"BP18-006","face_up":true}]);
        let mut board = board(&value);
        evolve(&mut board, "source", "form", true, false);
        evolve(&mut board, "ally", "ally-form", false, false);
        let evolved = board.projection(View::Referee).unwrap();
        assert_eq!(
            evolved["objects"]["enemy"]["hp"], 12_u8,
            "the sole Rolo form deals four for its own and the later evolution"
        );
        let ex = evolved["P1"]["ex"].as_array().unwrap();
        assert_eq!(
            ex.len(),
            1,
            "super evolution creates exactly one printed spell"
        );
        let spell = ex[0].as_str().unwrap();
        assert_eq!(evolved["objects"][spell]["name"], "済度播種");
        let play = board
            .play_internal_card(spell, PlayerId { seat: 1 })
            .unwrap();
        finish(&mut board, play, false);
        settle(&mut board, false);
        let after = board.projection(View::Referee).unwrap();
        assert_eq!(
            after["P1"]["pp"]["current"], 7_u8,
            "both evolutions and the actual generated spell each cost one PP"
        );
        assert_eq!(
            after["P1"]["hand"],
            json!(["drawn"]),
            "the complete generated spell draws one"
        );
        assert_eq!(after["P1"]["ep"], 1_u8, "the generated spell grants one EP");
        assert_eq!(
            after["objects"]["spent"]["face_up"], false,
            "the actual chosen own resource flips down"
        );
        assert_eq!(
            after["objects"]["enemy-spent"]["face_up"], true,
            "opposing resources are unchanged"
        );
    }
}

#[test]
fn every_fist_pair_buffs_the_own_tokyo_recipient_before_and_after_evolving_and_hits_either_enemy() {
    for (base, form) in [("BP18-009", "BP18-010"), ("BP18-P05", "BP18-P06")] {
        for leader in [false, true] {
            let mut value = initial();
            value["players"]["P1"]["zones"]["field"] = json!([
                {"id":"source","card":base},{"id":"ally","card":"BP16-038"},
                {"id":"wrong","card":"BP01-171"}
            ]);
            value["players"]["P1"]["zones"]["evolve_deck"] = json!([
                {"id":"form","card":form},{"id":"ally-form","card":"BP16-039"}
            ]);
            let mut board = board(&value);
            evolve(&mut board, "ally", "ally-form", false, leader);
            evolve(&mut board, "source", "form", false, leader);
            let after = board.projection(View::Referee).unwrap();
            assert_eq!(
                after["objects"]["source"]["power"], 4_u8,
                "base and evolved observers each buff the same actual Tokyo follower"
            );
            assert_eq!(
                after["objects"]["source"]["hp"], 4_u8,
                "the first fixed buff survives evolution and the second is added"
            );
            assert_eq!(
                after["P2"]["leader"]["life"],
                if leader { 19_u8 } else { 20_u8 },
                "the formal choice can be the opposing leader"
            );
            assert_eq!(
                after["objects"]["enemy"]["hp"],
                if leader { 20_u8 } else { 19_u8 },
                "the same printed choice can instead be the opposing follower"
            );
            assert_eq!(
                after["P1"]["pp"]["current"], 8_u8,
                "both normal one-PP evolutions pay separately"
            );
        }
    }
}
