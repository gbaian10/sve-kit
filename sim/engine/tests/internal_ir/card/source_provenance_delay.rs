use super::*;
use sve_engine::game::internal::InputDomain;
use sve_engine::game::player::{Viewer, observation as native};

fn delayed(number: &str, evolves: bool) -> WellFormedCardProgram {
    let mut document = equipment_played::tree(
        &json!({"line":1_u8,"activate":{"cost":{"pp":0},"do":[{"delayed":{
            "on":{"life_changed":{"who":"self","direction":"down"}},
            "until":"turn","next_only":true,
            "do":[{"grant":{"on":{"who":"self","zone":"field","kind":"follower"},
                "until":"game","abilities":[{"attack_time":{"do":[{"life":{"who":"self","delta":1}}]}}]
            }}]
        }}]}}),
        number,
    );
    let mut steps = Vec::new();
    if evolves {
        steps.push(json!({"evolve":{"ref":"this"}}));
    }
    steps.push(json!({"life":{"who":"self","delta":-1}}));
    let trigger = equipment_played::tree(
        &json!({"line":2_u8,"activate":{"cost":{"pp":0},"do":steps}}),
        number,
    );
    document["cards"][number]["abilities"]
        .as_array_mut()
        .unwrap()
        .push(trigger["cards"][number]["abilities"][0].clone());
    lowerer().lower(&document).unwrap().remove(number).unwrap()
}

fn position(seat: u32) -> Game {
    let (mut snapshot, registry, document) = catalog_documents(&json!("1"));
    snapshot.push('\n');
    snapshot.push_str(
        &json!({"number":"unit-form","faces":[{
            "name":"unit-follower","card_class":"ニュートラル","card_type":"フォロワー・エボルヴ",
            "cost":"1","power":"3","hp":"8","traits":[],"text":null,"sections":[]
        }]})
        .to_string(),
    );
    let form = json!({"format":1,"kind":"effect_set","cards":{
        "unit-form":{"status":"complete","abilities":[]}
    }})
    .to_string();
    let catalog = Arc::new(
        Catalog::from_documents(
            &snapshot,
            &registry,
            &[("units.yaml".into(), document), ("form.yaml".into(), form)],
        )
        .unwrap(),
    );
    let own = format!("P{seat}");
    let mut initial = setup(seat, json!([]));
    initial["turn"]["active"] = json!(own);
    for side in ["P1", "P2"] {
        initial["players"][side]["zones"]["hand"] = json!([]);
        initial["players"][side]["zones"]["field"] = json!([]);
    }
    initial["players"][&own]["zones"]["field"] = json!([
        {"id":"giver","card":"unit-follower"},{"id":"recipient","card":"unit-follower"}
    ]);
    initial["players"][&own]["zones"]["evolve_deck"] = json!([
        {"id":"form","card":"unit-form"}
    ]);
    let mut game = Game::new(
        catalog,
        &initial,
        &Value::Null,
        &Value::Null,
        "delayed-giver-face",
    )
    .unwrap();
    game.load_internal_card("unit-follower", &delayed("unit-follower", true))
        .unwrap();
    game.load_internal_card("unit-form", &delayed("unit-form", false))
        .unwrap();
    for number in ["unit-amulet", "unit-spell"] {
        game.load_internal_card(number, &card(vec![])).unwrap();
    }
    game
}

#[expect(
    clippy::panic,
    reason = "The real evolution action offers its matching form."
)]
fn evolve(game: &mut Game, seat: u32) {
    let CardPlay::Awaiting(question) = start(game, seat, 1) else {
        panic!("evolution form")
    };
    let InputDomain::Targets(domain) = question.domain else {
        panic!("form choice")
    };
    assert_eq!(
        domain.members.len(),
        1,
        "only the matching evolution target is offered"
    );
    resolved(
        game.answer_internal_card(
            question.actor,
            question.token,
            Answer::Targets(vec![domain.members[0].handle]),
        )
        .unwrap(),
    );
    game.resume_internal_card().unwrap();
}

#[test]
fn source_provenance_delay_freezes_the_giver_at_creation() {
    for seat in [1, 2] {
        for evolved_at_creation in [false, true] {
            let mut game = position(seat);
            if evolved_at_creation {
                evolve(&mut game, seat);
            }
            resolved(start(&mut game, seat, 0));
            game.resume_internal_card().unwrap();
            if evolved_at_creation {
                resolved(start(&mut game, seat, 1));
                game.resume_internal_card().unwrap();
            } else {
                evolve(&mut game, seat);
            }
            let question = game.internal_pending_choice().unwrap().unwrap();
            let InputDomain::Pending(options) = question.domain else {
                panic!("life callback")
            };
            assert_eq!(options.len(), 1);
            resolved(
                game.answer_internal_card(
                    question.actor,
                    question.token,
                    Answer::Pending(options[0].handle),
                )
                .unwrap(),
            );
            game.resume_internal_card().unwrap();
            assert!(game.internal_pending_choice().unwrap().is_none());
            let other = if seat == 1 { "P2" } else { "P1" };
            game.internal_rule_action(
                &json!({"do":"attack","attacker":"recipient","target":format!("{other}.leader")}),
            )
            .unwrap();
            assert!(
                game.internal_pending_choice().unwrap().is_some(),
                "combat produces the granted Drain occurrence"
            );
            let before = exact_authority(&game);
            let expected = if evolved_at_creation {
                "unit-form"
            } else {
                "unit-follower"
            };
            for viewer in [Viewer::P1, Viewer::P2] {
                let observed = game.observe(viewer).unwrap();
                assert_eq!(observed.pending.len(), 1);
                assert_eq!(observed.pending[0].source.card, expected);
                assert_eq!(
                    observed.pending[0].source.provenance,
                    native::TextProvenance::Granted {
                        by: native::CardFace {
                            card: expected.into(),
                            face: 0
                        }
                    }
                );
            }
            assert_eq!(exact_authority(&game), before);
        }
    }
}

fn start(game: &mut Game, seat: u32, index: usize) -> CardPlay {
    let source = game.internal_activated_sources("giver").unwrap()[index];
    game.activate_internal_card_instance("giver", source.instance, PlayerId { seat })
        .unwrap()
}
