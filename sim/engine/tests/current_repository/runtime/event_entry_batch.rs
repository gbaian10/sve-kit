#![expect(
    clippy::panic,
    reason = "Unexpected decisions invalidate the printed event scenarios."
)]
use super::event_cemetery::{finish_checks, move_material, position, prepared, resolve_event};
use super::*;

fn enter(game: &mut Game, id: &str) {
    let mut play = game.play_internal_card(id, PlayerId { seat: 1 }).unwrap();
    if let CardPlay::Awaiting(question) = &play {
        assert!(
            matches!(question.domain, InputDomain::WardEntry { .. }),
            "only printed Ward posture precedes entry"
        );
        play = game
            .answer_internal_card(question.actor, question.token, Answer::WardEntry(false))
            .unwrap();
    }
    assert!(
        matches!(play, CardPlay::Entered(_)),
        "the material completes its field play"
    );
    finish_checks(game);
}

fn material_objects() -> Selector {
    let Action::Move { objects, .. } = move_material(ZoneKind::Field, OwnerZone::Cemetery) else {
        panic!("material selector")
    };
    objects
}

fn create_token(name: &str) -> Action {
    Action::Token {
        tokens: vec![TokenSpec {
            template: TokenTemplate::Fixed {
                template: TokenTemplateId(name.into()),
            },
            count: Expr::Literal(Nat(1)),
        }],
        destination: TokenDestination::Single {
            zone: Expr::Literal(ZoneRef {
                player: PlayerRef::Controller {},
                kind: ZoneKind::Field,
            }),
        },
        output: Binding::new(50, ScopeId(1)),
        initial_state: None,
        initial_posture: Posture::Standing,
        entry_effects: vec![],
    }
}

#[test]
fn soldiers_and_amulet_observer_change_only_printed_power() {
    for (card, material) in [
        ("BP01-043", "movement-unit"),
        ("BP01-044", "movement-unit"),
        ("BP16-109", "movement-amulet"),
    ] {
        let mut game = prepared(
            &position(card, "P1", material, "P1", "ex"),
            move_material(ZoneKind::Field, OwnerZone::Cemetery),
        );
        let before = game.projection(View::Referee).unwrap()["objects"]["source"].clone();
        enter(&mut game, "material");
        resolve_event(&mut game, card, 1);
        let state = game.projection(View::Referee).unwrap();
        assert_eq!(
            state["objects"]["source"]["power"],
            before["power"].as_u64().unwrap() + 1
        );
        assert_eq!(state["objects"]["source"]["hp"], before["hp"]);
    }
}

#[test]
fn sergeant_recovers_life_when_another_allied_follower_enters() {
    for card in SERGEANT {
        let mut initial = position(card, "P1", "movement-unit", "P1", "ex");
        initial["players"]["P1"]["leader"]["life"] = json!(10_u8);
        let mut game = prepared(
            &initial,
            move_material(ZoneKind::Field, OwnerZone::Cemetery),
        );
        enter(&mut game, "material");
        resolve_event(&mut game, card, 1);
        assert_eq!(
            game.projection(View::Referee).unwrap()["P1"]["leader"]["life"],
            11_u8
        );
    }
}

#[test]
fn latica_recovers_pp_only_for_the_named_tree() {
    for card in LATICA {
        for material in ["BP07-T03", "movement-amulet"] {
            let mut game = prepared(
                &position(card, "P1", material, "P1", "ex"),
                move_material(ZoneKind::Field, OwnerZone::Cemetery),
            );
            enter(&mut game, "material");
            let before = game.projection(View::Referee).unwrap()["P1"]["pp"]["current"]
                .as_u64()
                .unwrap();
            let matches = material == "BP07-T03";
            resolve_event(&mut game, card, usize::from(matches));
            assert_eq!(
                game.projection(View::Referee).unwrap()["P1"]["pp"]["current"],
                before + u64::from(matches)
            );
        }
    }
}

#[test]
fn vania_named_bat_entry_heals_or_damages_once_on_its_own_turn() {
    for (cards, heal) in [(VANIA_DAY, true), (VANIA_NIGHT, false)] {
        for card in cards {
            let mut initial = position(card, "P1", "movement-unit", "P1", "hand");
            initial["players"]["P1"]["leader"]["life"] = json!(10_u8);
            let mut game = prepared(&initial, create_token("フォレストバット"));
            activate(&mut game, "P1");
            finish_checks(&mut game);
            let before = game.projection(View::Referee).unwrap();
            resolve_event(&mut game, card, 1);
            activate(&mut game, "P1");
            finish_checks(&mut game);
            resolve_event(&mut game, card, 0);
            let after = game.projection(View::Referee).unwrap();
            assert_eq!(after["P1"]["leader"]["life"], 10 + u8::from(heal));
            assert_eq!(
                after["P2"]["leader"]["life"],
                before["P2"]["leader"]["life"].as_u64().unwrap() - u64::from(!heal)
            );
        }
    }
}

#[test]
fn stringmeister_creates_two_puppets_in_ex_for_one_named_entry() {
    for card in STRINGMEISTER {
        let mut game = prepared(
            &position(card, "P1", "movement-unit", "P1", "hand"),
            create_token("操り人形"),
        );
        activate(&mut game, "P1");
        finish_checks(&mut game);
        resolve_event(&mut game, card, 1);
        activate(&mut game, "P1");
        finish_checks(&mut game);
        resolve_event(&mut game, card, 0);
        let state = game.projection(View::Referee).unwrap();
        assert_eq!(state["P1"]["ex"].as_array().unwrap().len(), 2);
        assert!(
            state["P1"]["ex"]
                .as_array()
                .unwrap()
                .iter()
                .all(|id| state["objects"][id.as_str().unwrap()]["card"] == "BP08-T09")
        );
    }
}

#[test]
fn punish_mermaid_filters_ocean_trait_on_another_follower() {
    for material in ["BP15-062", "movement-unit"] {
        let mut game = prepared(
            &position("BP15-062", "P1", material, "P1", "ex"),
            move_material(ZoneKind::Field, OwnerZone::Cemetery),
        );
        enter(&mut game, "material");
        let before = game.projection(View::Referee).unwrap()["P2"]["leader"]["life"]
            .as_u64()
            .unwrap();
        let matched = material == "BP15-062";
        resolve_event(&mut game, "BP15-062", usize::from(matched));
        assert_eq!(
            game.projection(View::Referee).unwrap()["P2"]["leader"]["life"],
            before - u64::from(matched)
        );
    }
}

#[test]
fn tribunal_demon_and_evolved_hedgehog_observe_opposing_destruction() {
    for (cards, demon) in [(DEMON, true), (&["BP03-035"], false)] {
        for card in cards {
            let mut initial = position(card, "P1", "movement-unit", "P2", "field");
            initial["players"]["P1"]["leader"]["life"] = json!(10_u8);
            let mut game = prepared(
                &initial,
                Action::Destroy {
                    objects: material_objects(),
                },
            );
            let before = game.projection(View::Referee).unwrap();
            activate(&mut game, "P1");
            finish_checks(&mut game);
            resolve_event(&mut game, card, 1);
            let after = game.projection(View::Referee).unwrap();
            if demon {
                assert_eq!(after["P1"]["leader"]["life"], 11_u8);
                assert_eq!(
                    after["P2"]["leader"]["life"],
                    before["P2"]["leader"]["life"].as_u64().unwrap() - 1
                );
            } else {
                assert_eq!(
                    after["objects"]["source"]["power"],
                    before["objects"]["source"]["power"].as_u64().unwrap() + 1
                );
            }
        }
    }
}

#[test]
fn guardian_grants_ward_to_its_created_tiger() {
    for card in GUARDIAN {
        let mut game = prepared(
            &position(card, "P1", "movement-amulet", "P1", "field"),
            move_material(ZoneKind::Field, OwnerZone::Cemetery),
        );
        activate(&mut game, "P1");
        finish_checks(&mut game);
        resolve_event(&mut game, card, 1);
        let state = game.projection(View::Referee).unwrap();
        let tiger = state["P1"]["field"]
            .as_array()
            .unwrap()
            .iter()
            .find_map(|id| {
                let id = id.as_str().unwrap();
                (state["objects"][id]["card"] == "BP01-T17").then_some(id)
            })
            .unwrap();
        assert!(
            state["objects"][tiger]["keywords"]
                .as_array()
                .unwrap()
                .contains(&json!("守護"))
        );
    }
}

#[test]
fn calydon_buffs_and_gains_assail_after_an_amulet_leaves() {
    for card in CALYDON {
        let mut game = prepared(
            &position(card, "P1", "movement-amulet", "P1", "field"),
            move_material(ZoneKind::Field, OwnerZone::Cemetery),
        );
        let before = game.projection(View::Referee).unwrap()["objects"]["source"].clone();
        activate(&mut game, "P1");
        finish_checks(&mut game);
        resolve_event(&mut game, card, 1);
        let after = game.projection(View::Referee).unwrap();
        for stat in ["power", "hp"] {
            assert_eq!(
                after["objects"]["source"][stat],
                before[stat].as_u64().unwrap() + 2
            );
        }
        assert!(
            after["objects"]["source"]["keywords"]
                .as_array()
                .unwrap()
                .contains(&json!("指定攻撃"))
        );
    }
}

#[test]
fn karat_observes_allied_amulet_departure() {
    for card in KARAT {
        let mut game = prepared(
            &position(card, "P1", "movement-amulet", "P1", "field"),
            move_material(ZoneKind::Field, OwnerZone::Cemetery),
        );
        let before = game.projection(View::Referee).unwrap()["P2"]["leader"]["life"]
            .as_u64()
            .unwrap();
        activate(&mut game, "P1");
        finish_checks(&mut game);
        resolve_event(&mut game, card, 1);
        assert_eq!(
            game.projection(View::Referee).unwrap()["P2"]["leader"]["life"],
            before - 1
        );
    }
}
const SERGEANT: &[&str] = &["BP21-031", "BP21-P16"];
const LATICA: &[&str] = &["BP07-002", "BP07-SL02", "BP07-U01"];
const VANIA_DAY: &[&str] = &["BP09-082", "BP09-P24"];
const VANIA_NIGHT: &[&str] = &["BP09-083", "BP09-P25"];
const STRINGMEISTER: &[&str] = &["BP11-010", "PR-302"];
const DEMON: &[&str] = &["BP01-106"];
const GUARDIAN: &[&str] = &["BP01-141"];
const CALYDON: &[&str] = &["BP04-108"];
const KARAT: &[&str] = &["BP12-088", "BP12-SL25"];

#[test]
fn tribunal_demon_keeps_its_occurrence_after_simultaneous_destruction_and_uses_field_control() {
    for card in DEMON {
        for simultaneous in [false, true] {
            let mut objects = material_objects();
            if simultaneous {
                let Selector::Zone { predicate, .. } = &mut objects else {
                    panic!("field subjects")
                };
                let Expr::Compute(predicate) = predicate.as_mut() else {
                    panic!("material names")
                };
                let Predicate::NamesIntersect {
                    right: Expr::Literal(names),
                    ..
                } = predicate.as_mut()
                else {
                    panic!("named subjects")
                };
                names.push(CardName("裁きの悪魔".into()));
            }
            let mut initial = position(card, "P1", "movement-unit", "P2", "field");
            initial["players"]["P1"]["leader"]["life"] = json!(10_u8);
            initial["players"]["P2"]["zones"]["field"][1]["owner"] = json!("P1");
            let mut game = prepared(&initial, Action::Destroy { objects });
            let before = game.projection(View::Referee).unwrap()["P2"]["leader"]["life"]
                .as_u64()
                .unwrap();
            activate(&mut game, "P1");
            finish_checks(&mut game);
            resolve_event(&mut game, card, 1);
            let state = game.projection(View::Referee).unwrap();
            assert_eq!(state["P2"]["leader"]["life"], before - 1);
            assert_eq!(state["P1"]["leader"]["life"], 11_u8);
            assert!(
                state["P1"]["cemetery"]
                    .as_array()
                    .unwrap()
                    .contains(&json!("material"))
            );
            assert_eq!(
                state["P1"]["field"]
                    .as_array()
                    .unwrap()
                    .contains(&json!("source")),
                !simultaneous
            );
        }
    }
}

#[test]
fn karat_fanfare_pays_the_amulet_then_deals_four_recovers_pp_and_triggers_departure() {
    let mut initial = position("BP12-088", "P1", "movement-amulet", "P1", "field");
    initial["players"]["P1"]["zones"]["field"] =
        json!([{"id":"material","card":"movement-amulet"}]);
    initial["players"]["P1"]["zones"]["hand"] = json!([{"id":"source","card":"BP12-088"}]);
    let mut game = prepared(
        &initial,
        move_material(ZoneKind::Field, OwnerZone::Cemetery),
    );
    enter(&mut game, "source");
    let pending = game.internal_pending_choice().unwrap().unwrap();
    let InputDomain::Pending(options) = &pending.domain else {
        panic!("printed fanfare")
    };
    assert_eq!(options.len(), 1);
    assert_eq!(options[0].line, 2);
    let mut play = game
        .answer_internal_card(
            pending.actor,
            pending.token,
            Answer::Pending(options[0].handle),
        )
        .unwrap();
    while let CardPlay::Awaiting(question) = play {
        let answer = if matches!(question.domain, InputDomain::OptionalCost) {
            Answer::OptionalCost(true)
        } else if let InputDomain::CostMaterials(domain) = &question.domain {
            assert_eq!(domain.members.len(), 1);
            Answer::CostMaterials(vec![domain.members[0].handle])
        } else if let InputDomain::Targets(domain) = &question.domain {
            let enemy = domain
                .members
                .iter()
                .find(|member| member.card.as_deref() == Some("movement-enemy"))
                .unwrap();
            Answer::Targets(vec![enemy.handle])
        } else {
            panic!("printed fanfare target or material payment")
        };
        play = game
            .answer_internal_card(question.actor, question.token, answer)
            .unwrap();
    }
    assert!(!matches!(play, CardPlay::Cancelled));
    finish_checks(&mut game);
    resolve_event(&mut game, "BP12-088", 1);
    let state = game.projection(View::Referee).unwrap();
    assert_eq!(state["objects"]["enemy"]["damage"], 4_u8);
    assert_eq!(state["P1"]["pp"]["current"], 9_u8);
    assert_eq!(state["P2"]["leader"]["life"], 19_u8);
    assert!(
        state["P1"]["cemetery"]
            .as_array()
            .unwrap()
            .contains(&json!("material"))
    );
}
