#![expect(
    clippy::panic,
    reason = "Unexpected questions invalidate the printed field-or-EX material scenarios."
)]
use super::event_cemetery::finish_checks;
use super::*;
use sve_engine::game::internal::DecisionView;

fn trigger(board: &mut Game, number: &str, line: u64) -> CardPlay {
    finish_checks(board);
    let pending = board.internal_pending_choice().unwrap().unwrap();
    let InputDomain::Pending(options) = &pending.domain else {
        panic!("printed pending ability")
    };
    let option = options
        .iter()
        .find(|option| option.card == number && option.line == line)
        .unwrap();
    board
        .answer_internal_card(pending.actor, pending.token, Answer::Pending(option.handle))
        .unwrap()
}

fn question(play: CardPlay) -> DecisionView {
    let CardPlay::Awaiting(question) = play else {
        panic!("formal target or material selection")
    };
    question
}

fn prepared(field_material: bool) -> (Game, String) {
    let mut initial = draw_position();
    initial["players"]["P1"]["leader"]["life"] = json!(10_u8);
    initial["players"]["P1"]["zones"]["hand"] = json!([{"id":"source","card":"BP12-074"}]);
    initial["players"]["P2"]["zones"]["field"] = json!([
        {"id":"enemy","card":"BP14-008"},{"id":"survivor","card":"BP01-037"}
    ]);
    let mut board = game(&initial);
    assert!(
        matches!(
            board
                .play_internal_card("source", PlayerId { seat: 1 })
                .unwrap(),
            CardPlay::Entered(_)
        ),
        "the real Medusa enters before Fanfare"
    );
    resolved(trigger(&mut board, "BP12-074", 1));
    finish_checks(&mut board);
    let created = board.projection(View::Referee).unwrap();
    assert_eq!(
        created["P1"]["ex"].as_array().unwrap().len(),
        1,
        "Fanfare creates exactly one Medusiana in EX"
    );
    let token = created["P1"]["ex"][0].as_str().unwrap().to_owned();
    assert_eq!(
        created["objects"][&token]["card"], "BP12-P28",
        "the admitted printed Medusiana template is used"
    );
    if field_material {
        assert!(
            matches!(
                board
                    .play_internal_card(&token, PlayerId { seat: 1 })
                    .unwrap(),
                CardPlay::Entered(_)
            ),
            "the same created token can be played from EX"
        );
        resolved(trigger(&mut board, "BP12-P28", 2));
        finish_checks(&mut board);
        assert_eq!(
            board.projection(View::Referee).unwrap()["P1"]["leader"]["life"],
            12_u8,
            "the token's real Fanfare counts both own Makai followers"
        );
    }
    (board, token)
}

fn pay_and_destroy(board: &mut Game) {
    let ability = selection().current["BP12-074"]
        .program
        .card()
        .abilities
        .iter()
        .find(|ability| ability.node.kind == AbilityKind::Activated)
        .unwrap()
        .node
        .id;
    let target = question(
        board
            .activate_internal_card("source", ability, PlayerId { seat: 1 })
            .unwrap(),
    );
    let InputDomain::Targets(targets) = &target.domain else {
        panic!("formal enemy target")
    };
    let victim = targets
        .members
        .iter()
        .find(|member| member.card.as_deref() == Some("BP14-008"))
        .unwrap()
        .handle;
    let material = question(
        board
            .answer_internal_card(target.actor, target.token, Answer::Targets(vec![victim]))
            .unwrap(),
    );
    let InputDomain::CostMaterials(materials) = &material.domain else {
        panic!("public field-or-EX material")
    };
    assert_eq!(
        materials.members.len(),
        1,
        "only the created Makai token can pay"
    );
    resolved(
        board
            .answer_internal_card(
                material.actor,
                material.token,
                Answer::CostMaterials(vec![materials.members[0].handle]),
            )
            .unwrap(),
    );
    finish_checks(board);
}

#[test]
fn r7a_field_ex_material_medusa_pays_either_zone_and_only_field_payment_triggers_lastword() {
    for field_material in [false, true] {
        let (mut board, token) = prepared(field_material);
        pay_and_destroy(&mut board);
        let after = board.projection(View::Referee).unwrap();
        assert!(
            after["objects"].get(&token).is_none(),
            "the paid token is erased after its cemetery move"
        );
        assert_eq!(
            after["objects"]["source"]["acted"], true,
            "the source act cost is paid with the material"
        );
        assert_eq!(
            after["P2"]["cemetery"],
            json!(["enemy"]),
            "the activated body destroys the declared follower"
        );
        if field_material {
            let lastword = question(trigger(&mut board, "BP12-P28", 3));
            assert_eq!(
                lastword.actor.seat, 2,
                "the opposing player chooses its own follower for Medusiana's Lastword"
            );
            let InputDomain::Targets(targets) = &lastword.domain else {
                panic!("Lastword follower selection")
            };
            assert_eq!(
                targets.members.len(),
                1,
                "only the surviving opposing follower remains"
            );
            resolved(
                board
                    .answer_internal_card(
                        lastword.actor,
                        lastword.token,
                        Answer::Targets(vec![targets.members[0].handle]),
                    )
                    .unwrap(),
            );
            finish_checks(&mut board);
            assert_eq!(
                board.projection(View::Referee).unwrap()["P2"]["cemetery"],
                json!(["enemy", "survivor"]),
                "field-to-cemetery payment really triggers the token's Lastword"
            );
        } else {
            assert!(
                board.internal_pending_choice().unwrap().is_none(),
                "Q1684 forbids Lastword from EX-to-cemetery payment"
            );
            assert_eq!(
                after["P2"]["field"],
                json!(["survivor"]),
                "EX payment does not sacrifice another enemy"
            );
        }
    }
}
