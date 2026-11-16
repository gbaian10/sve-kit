use super::*;
use sve_engine::game::internal::InputDomain;
use sve_engine::game::player::{Viewer, observation as native};

fn program() -> WellFormedCardProgram {
    let mut document = equipment_played::tree(
        &json!({"line":1_u8,"activate":{"cost":{"pp":0},"do":[{"grant":{
            "on":{"who":"self","zone":"field","kind":"follower","name":"middle"},
            "until":"game","abilities":[{"static":{"effects":[{"grant":{
                "on":{"who":"self","zone":"field","kind":"follower","name":"unit-follower"},
                "keywords":["drain"]
            }}]}}]
        }}]}}),
        "unit-amulet",
    );
    let evolution = equipment_played::tree(
        &json!({"line":2_u8,"activate":{
            "cost":{"pp":0},"targets":[{"bind":"middle","from":{
                "who":"self","zone":"field","kind":"follower","name":"middle"
            },"n":1}],"do":[{"evolve":{"ref":"middle"}}]
        }}),
        "unit-amulet",
    );
    document["cards"]["unit-amulet"]["abilities"]
        .as_array_mut()
        .unwrap()
        .push(evolution["cards"]["unit-amulet"]["abilities"][0].clone());
    lowerer()
        .lower(&document)
        .unwrap()
        .remove("unit-amulet")
        .unwrap()
}

fn position(seat: u32) -> Game {
    let (mut snapshot, _, document) = catalog_documents(&json!("1"));
    for (number, kind) in [
        ("unit-middle", "フォロワー"),
        ("middle-form", "フォロワー・エボルヴ"),
    ] {
        snapshot.push('\n');
        snapshot.push_str(
            &json!({"number":number,"faces":[{
                "name":"middle","card_class":"ニュートラル","card_type":kind,
                "cost":"1","power":"3","hp":"8","traits":[],"text":null,"sections":[]
            }]})
            .to_string(),
        );
    }
    let extras = json!({"format":1,"kind":"effect_set","cards":{
        "unit-middle":{"status":"complete","abilities":[]},
        "middle-form":{"status":"complete","abilities":[]}
    }})
    .to_string();
    let catalog = Arc::new(
        Catalog::from_documents(
            &snapshot,
            include_str!(concat!(
                env!("CARGO_MANIFEST_DIR"),
                "/../../authored/rules/keywords.yaml"
            )),
            &[
                ("units.yaml".into(), document),
                ("middle.yaml".into(), extras),
            ],
        )
        .unwrap(),
    );
    let own = format!("P{seat}");
    let mut initial = setup(seat, json!([]));
    initial["turn"]["active"] = json!(own);
    for side in ["P1", "P2"] {
        initial["players"][side]["zones"]["field"] = json!([]);
        initial["players"][side]["zones"]["hand"] = json!([]);
    }
    initial["players"][&own]["zones"]["field"] = json!([
        {"id":"giver","card":"unit-amulet"},{"id":"middle","card":"unit-middle"},
        {"id":"first","card":"unit-follower"},{"id":"second","card":"unit-follower"}
    ]);
    initial["players"][&own]["zones"]["evolve_deck"] = json!([
        {"id":"form","card":"middle-form"}
    ]);
    let mut game = Game::new(
        catalog,
        &initial,
        &Value::Null,
        &Value::Null,
        "continuous-new-face",
    )
    .unwrap();
    game.load_internal_card("unit-amulet", &program()).unwrap();
    for number in ["unit-middle", "middle-form", "unit-follower", "unit-spell"] {
        game.load_internal_card(number, &card(vec![])).unwrap();
    }
    resolved(start(&mut game, seat, 0));
    game.resume_internal_card().unwrap();
    game
}

fn drain(game: &mut Game, seat: u32, attacker: &str) {
    let other = if seat == 1 { "P2" } else { "P1" };
    game.internal_rule_action(
        &json!({"do":"attack","attacker":attacker,"target":format!("{other}.leader")}),
    )
    .unwrap();
    let quick = game.internal_quick_choice().unwrap().unwrap();
    game.answer_internal_card(quick.actor, quick.token, Answer::Quick(None))
        .unwrap();
    assert!(
        game.internal_pending_choice().unwrap().is_some(),
        "combat produces the granted Drain occurrence"
    );
}

#[expect(
    clippy::panic,
    reason = "Effect evolution asks for the middle and its matching form."
)]
fn evolve(game: &mut Game, seat: u32) {
    let CardPlay::Awaiting(question) = start(game, seat, 1) else {
        panic!("middle target")
    };
    let InputDomain::Targets(domain) = question.domain else {
        panic!("middle target domain")
    };
    assert_eq!(
        domain.members.len(),
        1,
        "only the matching evolution target is offered"
    );
    let CardPlay::Awaiting(form) = game
        .answer_internal_card(
            question.actor,
            question.token,
            Answer::Targets(vec![domain.members[0].handle]),
        )
        .unwrap()
    else {
        panic!("middle form")
    };
    let InputDomain::Targets(domain) = form.domain else {
        panic!("middle form domain")
    };
    assert_eq!(
        domain.members.len(),
        1,
        "only the matching evolution target is offered"
    );
    resolved(
        game.answer_internal_card(
            form.actor,
            form.token,
            Answer::Targets(vec![domain.members[0].handle]),
        )
        .unwrap(),
    );
    game.resume_internal_card().unwrap();
}

#[test]
fn source_provenance_continuous_reevaluates_the_giver_without_rewriting_history() {
    for seat in [1, 2] {
        let mut game = position(seat);
        drain(&mut game, seat, "first");
        let frozen = [Viewer::P1, Viewer::P2].map(|viewer| game.observe(viewer).unwrap());
        for observed in &frozen {
            assert_eq!(observed.pending[0].source.card, "unit-amulet");
            assert_eq!(
                observed.pending[0].source.provenance,
                native::TextProvenance::Granted {
                    by: native::CardFace {
                        card: "unit-middle".into(),
                        face: 0
                    }
                }
            );
        }
        let question = game.internal_pending_choice().unwrap().unwrap();
        let InputDomain::Pending(options) = question.domain else {
            panic!("drain")
        };
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
        let history = [Viewer::P1, Viewer::P2].map(|viewer| {
            game.events(viewer, None)
                .unwrap()
                .cloned()
                .collect::<Vec<_>>()
        });
        evolve(&mut game, seat);
        drain(&mut game, seat, "second");
        let before = exact_authority(&game);
        for (index, viewer) in [Viewer::P1, Viewer::P2].into_iter().enumerate() {
            let observed = game.observe(viewer).unwrap();
            assert_eq!(observed.pending.len(), 1);
            assert_eq!(observed.pending[0].source.card, "unit-amulet");
            assert_eq!(
                observed.pending[0].source.provenance,
                native::TextProvenance::Granted {
                    by: native::CardFace {
                        card: "middle-form".into(),
                        face: 0
                    }
                }
            );
            assert_eq!(
                frozen[index].pending[0].source.provenance,
                native::TextProvenance::Granted {
                    by: native::CardFace {
                        card: "unit-middle".into(),
                        face: 0
                    }
                }
            );
            assert_eq!(
                game.events(viewer, None)
                    .unwrap()
                    .take(history[index].len())
                    .cloned()
                    .collect::<Vec<_>>(),
                history[index]
            );
        }
        assert_eq!(exact_authority(&game), before);
    }
}

fn start(game: &mut Game, seat: u32, index: usize) -> CardPlay {
    let source = game.internal_activated_sources("giver").unwrap()[index];
    game.activate_internal_card_instance("giver", source.instance, PlayerId { seat })
        .unwrap()
}
