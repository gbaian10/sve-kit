use super::*;
use crate::battle::tests::{current_fixture, rule_fixtures};
use crate::battle::{BattleReferences, PrintingFaceIdentity, ReferenceInput, build_observation};
use crate::catalog::{Catalog, SelectedEffects};
use crate::game::Game;
use serde_json::Value;
use sve_battle_contract::{CardRef, Nat};

fn fixture(granted: bool) -> (SelectedEffects, BattleReferences) {
    let abilities = if granted {
        vec![
            json!({"activate":{"cost":{"pp":0_i32},"targets":[{"bind":"friend","from":{"who":"self","zone":"field","kind":"follower"},"n":1_i32}],"do":[
            {"life":{"who":"self","delta":1_i32}},
            {"play_ability":{"of":"friend","ability":{"ub":true,"n":1_i32},"cost_policy":"waive_original"}},
            {"life":{"who":"self","delta":10_i32}}]}}),
        ]
    } else {
        vec![
            json!({"activate":{"cost":{"pp":0_i32},"do":[
            {"life":{"who":"self","delta":1_i32}},
            {"play_ability":{"of":"this","trigger":"evolve_time"}},
            {"life":{"who":"self","delta":10_i32}}]}}),
            json!({"evolve_time":{"cost":{"pp":0_i32},"do":[{"life":{"who":"self","delta":2_i32}}]}}),
        ]
    };
    let child = json!({"activate":{"ub":true,"cost":{"pp":0_i32},"do":[{"life":{"who":"self","delta":2_i32}}]}});
    let (equipment, equipment_author) = current_fixture::author(
        "unit-equipment",
        "22222222222222222222222222222222",
        "イクイップメント・トークン",
        if granted {
            vec![
                json!({"static":{"zone":"equipment","effects":[{"grant":{"on":"host","abilities":[child]}}]}}),
            ]
        } else {
            vec![]
        },
    );
    let (mut row, author) = current_fixture::author(
        "unit-follower",
        "11111111111111111111111111111111",
        "フォロワー",
        abilities,
    );
    row["faces"][0]["title"] = json!("synthetic-title");
    let snapshot = format!("{row}\n{equipment}\n");
    let rules = rule_fixtures::rules(
        &snapshot,
        json!([rule_fixtures::title(
            "synthetic",
            "unit-follower",
            json!([{"kind":"ub_enabled"}])
        )]),
        json!([]),
    );
    let selected = rule_fixtures::selection(
        &snapshot,
        &json!({"format":1_i32,"kind":"keyword_registry","rule_names":{}}).to_string(),
        &json!({"documents":[],"authors":[author,equipment_author]}),
        &rules.to_string(),
    )
    .unwrap();
    let references = references(&selected.catalogue, &snapshot);
    (selected, references)
}

fn references(catalog: &Catalog, snapshot: &str) -> BattleReferences {
    let faces = [
        ("unit-follower", "current-follower"),
        ("unit-equipment", "current-equipment"),
    ]
    .into_iter()
    .map(|(number, printing)| PrintingFaceIdentity {
        card_no: number.into(),
        native_face: 0,
        card: CardRef {
            printing_id: printing.into(),
            face: Nat::from(0_u32),
        },
    })
    .collect();
    BattleReferences::new(catalog,ReferenceInput {
        rules:None,region:"jp".into(),snapshot_sha256:rule_fixtures::hash(snapshot.as_bytes()),faces,
        wording:Some(serde_json::from_value(json!({
            "manifest_sha256":format!("sha256:{}","a".repeat(64_usize)),
            "faces":[{"card_no":"unit-follower","native_face":0_i32,"face_id":"f:11111111111111111111111111111111",
                "revisions":[{"revision_id":"revision-unit-follower","fields":[{"kind":"effect"}]}]},
                {"card_no":"unit-equipment","native_face":0_i32,"face_id":"f:22222222222222222222222222222222",
                "revisions":[{"revision_id":"revision-unit-equipment","fields":[{"kind":"effect"}]}]}]
        })).unwrap()),
    }).unwrap()
}

fn board(selected: &SelectedEffects, actor: Viewer, granted: bool) -> Game {
    let (own, other) = seats(actor);
    let mut initial = board_fixtures::setup(actor.player().seat, json!([]));
    initial["turn"]["active"] = json!(own);
    initial["players"][own]["construction"] = json!("title");
    initial["players"][own]["title_code"] = json!("synthetic");
    initial["players"][own]["zones"] = json!({"field":[{"id":"source","card":"unit-follower"}]});
    if granted {
        initial["players"][own]["zones"]["equipment"] =
            json!([{"id":"giver","card":"unit-equipment","state":{"equipped_to":"source"}}]);
    }
    initial["players"][other]["zones"] = json!({});
    let mut game = Game::from_selected_effects(
        selected.clone(),
        &initial,
        &Value::Null,
        &Value::Null,
        "current-ability",
    )
    .unwrap();
    let ability = selected.current["unit-follower"].program.card().abilities[0]
        .node
        .id;
    game.activate_internal_card("source", ability, actor.player())
        .unwrap();
    game.advance_player_input();
    if granted {
        let target = question(&game, actor).unwrap();
        let DecisionShape::Targets(selection) = domain(&target).unwrap() else {
            panic!("grant target")
        };
        assert_eq!(
            game.submit_player(
                actor,
                player::Command::Answer {
                    token: target.token,
                    answer: player::Answer::Targets(vec![selection.members[0].handle])
                }
            )
            .kind,
            SubmissionKind::Applied
        );
    }
    game
}

#[test]
fn admitted_current_printed_and_granted_abilities_keep_exact_wording_and_native_answers() {
    for granted in [false, true] {
        let (selected, references) = fixture(granted);
        for actor in [Viewer::P1, Viewer::P2] {
            let mut game = board(&selected, actor, granted);
            let native = question(&game, actor).unwrap();
            let before = board_fixtures::exact_authority(&game);
            let catalog = &selected.catalogue;
            let observation =
                build_observation(catalog, &references, &game.observe(actor).unwrap(), vec![])
                    .unwrap();
            let id = QuestionId::new("exact-ability".into());
            let (_, shape, binding) = project(
                domain(&native).unwrap(),
                Some(Context {
                    id: &id,
                    actor,
                    purpose: native.purpose,
                    observation: &observation,
                    catalog,
                    references: &references,
                }),
            )
            .unwrap();
            let wire::QuestionShape::Choose { options, .. } = shape else {
                panic!("ability choose")
            };
            let wire::ChoiceValue::Ability {
                source: SourceRef::Text { text },
            } = &options[0].value
            else {
                panic!("exact admitted wording")
            };
            assert_eq!(
                text.card.printing_id,
                if granted {
                    "current-equipment"
                } else {
                    "current-follower"
                }
            );
            assert_eq!(
                text.wording_revision,
                if granted {
                    "revision-unit-equipment"
                } else {
                    "revision-unit-follower"
                }
            );
            assert_eq!(text.section, "effect");
            assert_eq!(text.line.get(), if granted { 1 } else { 2 });
            assert_eq!(
                text.granted_by
                    .as_ref()
                    .map(|giver| giver.printing_id.as_str()),
                granted.then_some("current-equipment")
            );
            assert!(text.holder.is_some());
            assert!(
                board_fixtures::exact_authority(&game) == before,
                "source projection preserves authority"
            );
            let answer = Answer::Choose {
                chosen: vec![options[0].id.clone()],
            };
            assert_eq!(
                game.submit_player(actor, binding.answer(native.token, &answer).unwrap())
                    .kind,
                SubmissionKind::Applied
            );
            if !granted {
                let payment = question(&game, actor).unwrap();
                assert_eq!(
                    game.submit_player(
                        actor,
                        player::Command::Answer {
                            token: payment.token,
                            answer: player::Answer::OptionalCost(true)
                        }
                    )
                    .kind,
                    SubmissionKind::Applied
                );
            }
            assert_eq!(
                game.projection(View::Referee).unwrap()[seats(actor).0]["leader"]["life"],
                33_i32
            );
        }
    }
}
