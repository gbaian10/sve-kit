use super::*;
use crate::rule_fixtures;
use std::sync::OnceLock;
use sve_engine::game::internal::InputDomain;
use sve_engine::game::player::observation::{
    Observation, ObservedZone, Relationship, RelationshipKind,
};
use sve_engine::game::player::{ObjectHandle, Viewer};

fn resources() -> Arc<Catalog> {
    static CATALOG: OnceLock<Arc<Catalog>> = OnceLock::new();
    Arc::clone(CATALOG.get_or_init(|| {
        let (snapshot, registry, document) = catalog_documents(&json!("1"));
        let snapshot = snapshot
            .lines()
            .map(|line| {
                let mut card: Value = serde_json::from_str(line).unwrap();
                if matches!(card["number"].as_str(), Some("unit-spell" | "unit-amulet")) {
                    card["faces"][0]["card_type"] = json!("エボルヴ・スペル");
                    card["faces"][0]["power"] = json!("-");
                    card["faces"][0]["hp"] = json!("-");
                }
                card.to_string()
            })
            .collect::<Vec<_>>()
            .join("\n");
        let rules = rule_fixtures::rules(
            &snapshot,
            json!([]),
            json!([
                rule_fixtures::resource("meal_item", "unit-spell", "printed", false),
                rule_fixtures::resource("drive_point", "unit-amulet", "printed", false)
            ]),
        );
        Arc::new(
            rule_fixtures::catalog(
                &snapshot,
                &registry,
                &[("synthetic.yaml".into(), document)],
                &rules.to_string(),
            )
            .unwrap(),
        )
    }))
}

fn entry(ride: bool) -> WellFormedCardProgram {
    let tree = evolution_linked_movement::tree(&[
        (if ride { "ride" } else { "feed" }, json!({"cost":{"pp":1}})),
        (
            "activate",
            json!({"cost":{"pp":0},"do":[{"move":{"ref":"this","to":"hand"}}]}),
        ),
    ]);
    lowerer()
        .lower(&tree)
        .unwrap()
        .remove("unit-follower")
        .unwrap()
}

fn board(seat: u32, program: &WellFormedCardProgram, ride: bool) -> Game {
    let mut initial = setup(seat, json!([]));
    let actor = if seat == 1 { "P1" } else { "P2" };
    initial["turn"]["active"] = json!(actor);
    initial["players"][actor]["pp"] = json!({"current":4,"max":4});
    initial["players"][actor]["zones"]["hand"] = json!([]);
    initial["players"][actor]["zones"]["field"] = json!([{"id":"s","card":"unit-follower"}]);
    initial["players"][actor]["zones"]["evolve_deck"] = json!([
        {"id":"resource","card":if ride { "unit-amulet" } else { "unit-spell" }}
    ]);
    let mut game = Game::new(
        resources(),
        &initial,
        &Value::Null,
        &Value::Null,
        "board-relations",
    )
    .unwrap();
    game.load_internal_card("unit-follower", program).unwrap();
    game.load_internal_card("unit-spell", &card(vec![]))
        .unwrap();
    game.load_internal_card("unit-amulet", &card(vec![]))
        .unwrap();
    game
}

fn zone_handle(observation: &Observation, owner: Viewer, kind: ObservedZone) -> ObjectHandle {
    let zone = observation
        .players
        .iter()
        .find(|player| player.player == owner)
        .unwrap()
        .zones
        .iter()
        .find(|zone| zone.kind == kind)
        .unwrap();
    zone.cards
        .first()
        .unwrap_or_else(|| {
            panic!(
                "missing authorized member: viewer={:?}, owner={owner:?}, zone={kind:?}, count={}",
                observation.viewer, zone.count
            )
        })
        .card
        .handle
        .clone()
}

#[test]
fn board_relationships_follow_typed_meal_and_ride_payment_and_departure() {
    for ride in [false, true] {
        let program = entry(ride);
        for actor in [Viewer::P1, Viewer::P2] {
            let mut game = board(actor.player().seat, &program, ride);
            let CardPlay::Awaiting(question) = game
                .activate_internal_card("s", program.card().abilities[0].node.id, actor.player())
                .unwrap()
            else {
                panic!("entry payment")
            };
            assert!(matches!(
                question.domain,
                InputDomain::ResourceEntryPayment { .. }
            ));
            for viewer in [Viewer::P1, Viewer::P2] {
                assert_eq!(
                    game.observe(viewer).unwrap().relationships,
                    Vec::<Relationship>::new()
                );
            }
            resolved(
                game.answer_internal_card(
                    question.actor,
                    question.token,
                    Answer::ResourceEntryPayment {
                        materials: vec![0],
                        use_ep: false,
                    },
                )
                .unwrap(),
            );
            while game.internal_card_stage() == Some(ResumeStage::CheckTiming) {
                game.resume_internal_card().unwrap();
            }
            let me = if actor == Viewer::P1 { "P1" } else { "P2" };
            assert_eq!(
                game.projection(View::Referee).unwrap()[me]["pp"]["current"],
                3
            );
            let before = exact_authority(&game);
            for viewer in [Viewer::P1, Viewer::P2] {
                let observed = game.observe(viewer).unwrap();
                assert_eq!(
                    observed.relationships,
                    vec![Relationship {
                        kind: if ride {
                            RelationshipKind::Drive
                        } else {
                            RelationshipKind::Race
                        },
                        from: zone_handle(
                            &observed,
                            actor,
                            if ride {
                                ObservedZone::Drive
                            } else {
                                ObservedZone::Race
                            }
                        ),
                        to: zone_handle(&observed, actor, ObservedZone::Field),
                    }]
                );
                assert!(!format!("{:?}", observed.relationships).contains("resource"));
                assert_eq!(observed, game.observe(viewer).unwrap());
            }
            assert_eq!(exact_authority(&game), before);
            resolved(
                game.activate_internal_card(
                    "s",
                    program.card().abilities[1].node.id,
                    actor.player(),
                )
                .unwrap(),
            );
            while game.internal_card_stage() == Some(ResumeStage::CheckTiming) {
                game.resume_internal_card().unwrap();
            }
            for viewer in [Viewer::P1, Viewer::P2] {
                assert_eq!(
                    game.observe(viewer).unwrap().relationships,
                    Vec::<Relationship>::new()
                );
            }
        }
    }
}
