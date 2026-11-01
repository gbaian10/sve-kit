#![expect(
    clippy::panic,
    reason = "Unexpected decisions invalidate printed event scenarios."
)]
use super::event_cemetery::{finish_checks, move_named, position, prepared};
use super::*;

pub(super) const fn life(player: PlayerRef, amount: i64) -> Action {
    Action::Life {
        player,
        change: NumericChange::Add {
            amount: Expr::Literal(Int(amount)),
        },
    }
}
pub(super) fn material_objects() -> Selector {
    let Action::Move { objects, .. } =
        move_named(ZoneKind::Field, OwnerZone::Cemetery, "combat material")
    else {
        panic!("synthetic material selector");
    };
    objects
}
pub(super) fn grant(keywords: &[KeywordId]) -> Action {
    Action::Grant {
        subjects: material_objects(),
        abilities: keywords
            .iter()
            .map(|keyword| GrantedAbility::Keyword { keyword: *keyword })
            .collect(),
        duration: Duration {
            end: DurationEnd::NoTemporalExpiry {},
            anchor: TimeAnchor::Installation {},
            usage: None,
            intervals: vec![],
        },
        provider: TextProvider {
            provider: ObjectView::Current {
                object: Box::new(ObjectOperand::SourceHolder {}),
            },
            face: FaceId("0".into()),
            ability: AbilityId(1),
        },
    }
}
pub(super) fn initial(card: &str, material: &str, active: &str, side: &str) -> Value {
    let mut value = position(card, active, material, side, "field");
    value["players"]["P1"]["leader"]["life"] = json!(10_u8);
    value["players"]["P1"]["zones"]["deck"] = json!([
        {"id":"combat-draw-1","card":"movement-unit"},
        {"id":"combat-draw-2","card":"movement-unit"}
    ]);
    value["players"]["P1"]["zones"]["field"][0]["state"] = json!({"entered_this_turn":false});
    value["players"][side]["zones"]["field"][1]["state"] = json!({"entered_this_turn":false});
    value
}
pub(super) fn resolve(game: &mut Game, card: &str, expected: usize, accepted: bool) {
    finish_checks(game);
    let question = game.internal_pending_choice().unwrap();
    if expected == 0 {
        assert!(question.is_none(), "the counterexample cannot subscribe");
        return;
    }
    let question = question.unwrap();
    let InputDomain::Pending(options) = &question.domain else {
        panic!("printed occurrence");
    };
    assert_eq!(
        options.len(),
        expected,
        "the actual fact has exactly the printed occurrences"
    );
    assert!(
        options.iter().all(|option| option.card == card),
        "only the actual printed observer subscribes"
    );
    let mut play = game
        .answer_internal_card(
            question.actor,
            question.token,
            Answer::Pending(options[0].handle),
        )
        .unwrap();
    while let CardPlay::Awaiting(declaration) = play {
        let answer = if matches!(declaration.domain, InputDomain::OptionalCost) {
            Answer::OptionalCost(accepted)
        } else if let InputDomain::Targets(domain) = &declaration.domain {
            let enemy = domain
                .members
                .iter()
                .find(|member| member.card.as_deref() == Some("movement-enemy"))
                .unwrap();
            Answer::Targets(vec![enemy.handle])
        } else {
            panic!("printed optional cost or formal target");
        };
        play = game
            .answer_internal_card(declaration.actor, declaration.token, answer)
            .unwrap();
    }
    assert!(
        matches!(play, CardPlay::Resolved(_)) || (!accepted && play == CardPlay::Cancelled),
        "the selected occurrence completes or declines its own cost"
    );
    finish_checks(game);
}
pub(super) fn attack(game: &mut Game, attacker: &str, target: &str) {
    assert_eq!(
        game.internal_rule_action(&json!({"do":"attack","attacker":attacker,"target":target}))
            .unwrap()
            .outcome,
        "resolved",
        "the real attack declaration commits"
    );
}
pub(super) fn combat(game: &mut Game) {
    let question = game.internal_quick_choice().unwrap().unwrap();
    assert!(
        matches!(
            game.answer_internal_card(question.actor, question.token, Answer::Quick(None))
                .unwrap(),
            CardPlay::Continued(_)
        ),
        "the defending player's actual pass completes combat"
    );
    finish_checks(game);
}

#[test]
fn alexander_stands_after_each_real_own_turn_combat_but_not_opponent_turn_retaliation() {
    for card in ["BP02-020", "BP02-SL06"] {
        let mut game = prepared(
            &initial(card, "combat-low", "P1", "P2"),
            life(PlayerRef::Controller {}, 0),
        );
        for _ in 0_u8..2 {
            attack(&mut game, "source", "material");
            combat(&mut game);
            assert_eq!(
                game.projection(View::Referee).unwrap()["objects"]["source"]["acted"],
                true,
                "the real attack acts Alexander before the trigger"
            );
            resolve(&mut game, card, 1, true);
            assert_eq!(
                game.projection(View::Referee).unwrap()["objects"]["source"]["acted"],
                false,
                "each combat damage occurrence stands its own surviving source"
            );
        }
        let mut leader = prepared(
            &initial(card, "combat-low", "P1", "P2"),
            life(PlayerRef::Controller {}, 0),
        );
        attack(&mut leader, "source", "P2.leader");
        combat(&mut leader);
        resolve(&mut leader, card, 0, true);
        assert_eq!(
            leader.projection(View::Referee).unwrap()["P2"]["leader"]["life"],
            15_u8,
            "the real leader attack deals Alexander's printed five damage"
        );
        assert_eq!(
            leader.projection(View::Referee).unwrap()["objects"]["source"]["acted"],
            true,
            "leader attack damage is not combat damage (Q282)"
        );
        let mut value = initial(card, "combat-low", "P2", "P2");
        value["players"]["P1"]["zones"]["field"][0]["state"]["acted"] = json!(true);
        let mut defending = prepared(&value, life(PlayerRef::Controller {}, 0));
        attack(&mut defending, "material", "source");
        combat(&mut defending);
        resolve(&mut defending, card, 0, true);
        assert_eq!(
            defending.projection(View::Referee).unwrap()["objects"]["source"]["acted"],
            true,
            "retaliation on the opponent's turn does not stand Alexander"
        );
    }
}

#[test]
fn ruby_falcon_observes_other_effective_storm_or_ward_attackers_only_once() {
    for card in ["BP03-098", "BP03-P27"] {
        for keywords in [
            &[][..],
            &[KeywordId::Storm][..],
            &[KeywordId::Ward][..],
            &[KeywordId::Storm, KeywordId::Ward][..],
        ] {
            let action = if keywords.is_empty() {
                life(PlayerRef::Controller {}, 0)
            } else {
                grant(keywords)
            };
            let mut game = prepared(&initial(card, "combat-high", "P1", "P1"), action);
            activate(&mut game, "P1");
            finish_checks(&mut game);
            attack(&mut game, "material", "P2.leader");
            resolve(&mut game, card, usize::from(!keywords.is_empty()), true);
            assert_eq!(
                game.projection(View::Referee).unwrap()["P2"]["leader"]["life"],
                if keywords.is_empty() { 20_u8 } else { 19_u8 },
                "either or both effective keywords create one ability damage"
            );
            combat(&mut game);
        }
        let mut game = prepared(
            &initial(card, "combat-high", "P1", "P1"),
            life(PlayerRef::Controller {}, 0),
        );
        attack(&mut game, "source", "P2.leader");
        resolve(&mut game, card, 0, true);
        combat(&mut game);
    }
}

#[test]
fn dragon_scribe_reads_actual_attack_power_and_trait_at_declaration() {
    for card in ["BP18-062", "PR-466"] {
        for (material, buff, expected) in [
            ("combat-high", false, true),
            ("combat-low", false, false),
            ("combat-other", false, false),
            ("combat-low", true, true),
        ] {
            let action = if buff {
                Action::Modify {
                    subjects: material_objects(),
                    patch: CharacteristicPatch::Numeric {
                        stat: Stat::Attack,
                        change: NumericChange::Add {
                            amount: Expr::Literal(Int(1)),
                        },
                    },
                    expires: Duration {
                        end: DurationEnd::NoTemporalExpiry {},
                        anchor: TimeAnchor::Installation {},
                        usage: None,
                        intervals: vec![],
                    },
                    source: EffectReference::CurrentInstallation {},
                }
            } else {
                life(PlayerRef::Controller {}, 0)
            };
            let mut game = prepared(&initial(card, material, "P1", "P1"), action);
            activate(&mut game, "P1");
            finish_checks(&mut game);
            attack(&mut game, "material", "P2.leader");
            resolve(&mut game, card, usize::from(expected), true);
            assert_eq!(
                game.projection(View::Referee).unwrap()["P1"]["leader"]["life"],
                10_u8 + u8::from(expected),
                "only a qualifying effective attacker heals the observer's leader"
            );
            combat(&mut game);
        }
    }
}

#[test]
fn eiras_prayer_buffs_only_own_followers_for_the_first_actual_heal() {
    for card in ["BP02-095", "BP02-P26", "PR-494"] {
        let mut game = prepared(
            &initial(card, "combat-high", "P1", "P1"),
            life(PlayerRef::Controller {}, 1),
        );
        for expected in [1, 0] {
            activate(&mut game, "P1");
            finish_checks(&mut game);
            resolve(&mut game, card, expected, true);
            let state = game.projection(View::Referee).unwrap();
            assert_eq!(
                state["objects"]["material"]["power"], 5_u8,
                "the printed once-per-turn quota prevents a second buff"
            );
            assert_eq!(
                state["objects"]["material"]["hp"], 21_u8,
                "both follower stats receive the first buff"
            );
            assert_eq!(
                state["objects"]["enemy"]["power"], 10_u8,
                "the opponent receives no buff"
            );
        }
    }
    for action in [
        life(PlayerRef::Controller {}, 0),
        life(PlayerRef::Controller {}, -1),
        life(
            PlayerRef::RequireOne {
                players: Box::new(PlayerSet {
                    relation: PlayerRelation::Opponents,
                    relative_to: PlayerRef::Controller {},
                    cardinality: Cardinality {
                        minimum: Nat(1),
                        maximum: Some(Nat(1)),
                    },
                    text_quantifier: TextQuantifier::One,
                }),
            },
            1,
        ),
    ] {
        let mut game = prepared(&initial("BP02-095", "combat-high", "P1", "P1"), action);
        activate(&mut game, "P1");
        finish_checks(&mut game);
        resolve(&mut game, "BP02-095", 0, true);
        assert_eq!(
            game.projection(View::Referee).unwrap()["objects"]["material"]["power"],
            4_u8,
            "zero, loss and the opposing player's heal do not buff"
        );
    }
}

#[test]
fn winged_guardian_damages_the_opposing_leader_after_each_actual_heal() {
    let mut game = prepared(
        &initial("BP14-096", "combat-high", "P1", "P1"),
        life(PlayerRef::Controller {}, 1),
    );
    for count in 1_u8..=2 {
        activate(&mut game, "P1");
        finish_checks(&mut game);
        resolve(&mut game, "BP14-096", 1, true);
        let state = game.projection(View::Referee).unwrap();
        assert_eq!(
            state["P1"]["leader"]["life"],
            10_u8.checked_add(count).unwrap(),
            "each real heal changes life before the observer resolves"
        );
        assert_eq!(
            state["P2"]["leader"]["life"],
            20_u8.checked_sub(count).unwrap(),
            "the automatic ability damages the opposing leader once for each heal"
        );
        assert_eq!(
            state["objects"]["enemy"]["damage"], 0_u8,
            "the printed all-leader damage does not target followers"
        );
    }
}

#[test]
fn lethal_token_drain_heal_reaches_the_same_field_life_observer() {
    let mut value = initial("BP02-095", "BP13-T05", "P1", "P1");
    value["players"]["P2"]["zones"]["field"][0]["state"] = json!({"acted":true});
    let mut game = prepared(&value, life(PlayerRef::Controller {}, 0));
    let power = game.projection(View::Referee).unwrap()["objects"]["producer"]["power"]
        .as_u64()
        .unwrap();
    attack(&mut game, "material", "enemy");
    combat(&mut game);
    let state = game.projection(View::Referee).unwrap();
    assert!(
        !state["P1"]["field"]
            .as_array()
            .unwrap()
            .contains(&json!("material")),
        "the real lethal counterattack removes the Drain source before resolution"
    );
    assert!(
        !state["P1"]["cemetery"]
            .as_array()
            .unwrap()
            .contains(&json!("material")),
        "the defeated token disappears through the ordinary token rule"
    );
    resolve(&mut game, "BP13-T05", 1, true);
    assert_eq!(
        game.projection(View::Referee).unwrap()["P1"]["leader"]["life"],
        13_u8,
        "the retained real Drain occurrence heals its actual three damage"
    );
    assert_eq!(
        game.projection(View::Referee).unwrap()["objects"]["producer"]["power"],
        power,
        "the field life observer still waits for its own resolution"
    );
    resolve(&mut game, "BP02-095", 1, true);
    assert_eq!(
        game.projection(View::Referee).unwrap()["objects"]["producer"]["power"],
        power + 1,
        "the same native life hook observes a heal from the erased token's retained effect"
    );
}
