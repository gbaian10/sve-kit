use super::*;
use crate::battle::tests::{board_fixture, payment_fixtures, rule_fixtures};
use crate::catalog::{Catalog, EngineIdentityInput};
use crate::ir::lowering::ExpandedLowerer;
use crate::ir::{CardProgram, PlayerId, WellFormedCardProgram, payment_expanded_tree};
use alloc::sync::Arc;
use serde_json::Value;
use std::sync::OnceLock;

pub(super) fn catalog() -> Arc<Catalog> {
    static CATALOG: OnceLock<Arc<Catalog>> = OnceLock::new();
    Arc::clone(CATALOG.get_or_init(|| {
        let snapshot = &board_fixture::fixture().unwrap().snapshot;
        let (_, keywords, document) = board_fixtures::catalog_documents(&json!("1"));
        let rules = rule_fixtures::rules(
            snapshot,
            json!([rule_fixtures::title(
                "synthetic",
                "unit-follower",
                json!([])
            )]),
            json!([
                rule_fixtures::resource("meal_item", "unit-follower", "printed", false),
                rule_fixtures::resource("drive_point", "unit-spell", "printed", false)
            ]),
        );
        Arc::new(
            Catalog::from_documents_with_rules(
                snapshot,
                &keywords,
                &[("synthetic".into(), document)],
                &rules.to_string(),
                &EngineIdentityInput::LegacyJp,
            )
            .unwrap(),
        )
    }))
}

fn make(actor: Viewer, resources: &[&str], other_secret: &str, pp: u32, ep: u32) -> Game {
    let document = payment_fixtures::x_document(
        payment_expanded_tree(),
        "feed",
        json!({"cost":{"pp":1_u32}}),
    )
    .unwrap();
    let program = ExpandedLowerer::new()
        .unwrap()
        .lower(&document)
        .unwrap()
        .remove("unit-follower")
        .unwrap();
    let (own, other) = seats(actor);
    let mut initial = board_fixtures::setup(actor.player().seat, json!([]));
    initial["turn"]["active"] = json!(own);
    initial["players"][own]["pp"] = json!({"current":pp,"max":pp});
    initial["players"][own]["ep"] = json!(ep);
    initial["players"][own]["zones"] = json!({"field":[{"id":"source","card":"unit-follower"}],
        "evolve_deck":resources.iter().enumerate().map(|(i,card)|json!({"id":format!("resource-{i}"),"card":card})).collect::<Vec<_>>()});
    initial["players"][other]["zones"] = json!({"hand":[{"id":"hidden","card":other_secret}]});
    let mut game = Game::new(
        catalog(),
        &initial,
        &Value::Null,
        &Value::Null,
        "decision-resource",
    )
    .unwrap();
    let empty = WellFormedCardProgram::check(CardProgram {
        abilities: vec![],
        card_play_costs: vec![],
    })
    .unwrap();
    game.load_internal_card("unit-spell", &empty).unwrap();
    game.load_internal_card("unit-amulet", &empty).unwrap();
    game.load_internal_card("unit-follower", &program).unwrap();
    game.activate_internal_card(
        "source",
        program.card().abilities.first().unwrap().node.id,
        PlayerId {
            seat: actor.player().seat,
        },
    )
    .unwrap();
    game.advance_player_input();
    assert!(
        matches!(
            domain(&question(&game, actor).unwrap()),
            Some(DecisionShape::ResourceEntryPayment { .. })
        ),
        "native Meal payment stop exists"
    );
    game
}

#[test]
fn resource_payment_keeps_materials_and_ep_in_one_native_submission_both_seats() {
    for actor in [Viewer::P1, Viewer::P2] {
        let mut game = make(actor, &["unit-follower", "unit-spell"], "unit-spell", 0, 1);
        let native = question(&game, actor).unwrap();
        let (usage, shape, binding) =
            project_game(&game, actor, &QuestionId::new("joint".into())).unwrap();
        assert_eq!(usage, wire::QuestionUse::ResourceEntryPayment);
        let wire::QuestionShape::ResourcePayment {
            materials,
            ep_allowed,
        } = shape
        else {
            panic!("joint payment");
        };
        assert!(ep_allowed);
        assert_eq!(materials.bounds.min, WireNat::from(1_u32));
        assert_eq!(materials.candidates.len(), 1);
        assert_eq!(
            materials
                .candidates
                .first()
                .unwrap()
                .origin
                .as_ref()
                .unwrap()
                .zone,
            sve_battle_contract::Zone::EvolveDeck
        );
        let answer = Answer::ResourcePayment {
            materials: vec![materials.candidates.first().unwrap().id.clone()],
            use_ep: true,
        };
        let command = binding
            .answer(native.actor, &binding.question().id, &answer)
            .unwrap();
        assert!(matches!(
            &command,
            player::Command::Answer {
                answer: player::Answer::ResourceEntryPayment { use_ep: true, .. },
                ..
            }
        ));
        submit_projected(&mut game, &native, &binding, &answer);
        let state = game.projection(View::Referee).unwrap();
        let (own, _) = seats(actor);
        assert_eq!(state[own]["ep"], 0_u32);
        assert_eq!(state[own]["pp"]["current"], 0_u32);
        assert_eq!(state[own]["race"], json!(["resource-0"]));
    }
}

#[test]
fn resource_codec_refuses_wrong_shape_foreign_duplicates_counts_and_unpermitted_ep() {
    let game = make(
        Viewer::P1,
        &["unit-follower", "unit-follower"],
        "unit-spell",
        1,
        0,
    );
    let (_, _, shape, binding) = projected(&game, Viewer::P1);
    let wire::QuestionShape::ResourcePayment {
        materials,
        ep_allowed,
    } = shape
    else {
        panic!("resource domain");
    };
    assert!(!ep_allowed);
    let first = materials.candidates.first().unwrap().id.clone();
    for answer in [
        Answer::Confirm { accept: true },
        Answer::ResourcePayment {
            materials: vec![],
            use_ep: false,
        },
        Answer::ResourcePayment {
            materials: vec![first.clone(), first.clone()],
            use_ep: false,
        },
        Answer::ResourcePayment {
            materials: vec![sve_battle_contract::OptionId::new("foreign".into())],
            use_ep: false,
        },
        Answer::ResourcePayment {
            materials: vec![first.clone()],
            use_ep: true,
        },
    ] {
        assert_eq!(binding.decode(&answer), Err(AnswerError));
    }
    assert!(matches!(
        binding.decode(&Answer::ResourcePayment {
            materials: vec![first],
            use_ep: false
        }),
        Ok(player::Answer::ResourceEntryPayment { use_ep: false, .. })
    ));
}

#[test]
fn native_submission_checks_affordability_after_the_ep_input_permission() {
    let mut game = make(Viewer::P1, &["unit-follower"], "unit-spell", 0, 1);
    let (native, _, shape, binding) = projected(&game, Viewer::P1);
    let wire::QuestionShape::ResourcePayment { materials, .. } = shape else {
        panic!("resource domain");
    };
    let command = binding
        .answer(
            native.token,
            &Answer::ResourcePayment {
                materials: vec![materials.candidates.first().unwrap().id.clone()],
                use_ep: false,
            },
        )
        .unwrap();
    assert_eq!(
        game.submit_player(Viewer::P1, command).kind,
        SubmissionKind::Reverted
    );
    let state = game.projection(View::Referee).unwrap();
    assert_eq!(state["P1"]["ep"], 1_u32);
    assert_eq!(state["P1"]["pp"]["current"], 0_u32);
    assert_eq!(state["P1"]["evolve_deck"], json!(["resource-0"]));
}

mod privacy;
