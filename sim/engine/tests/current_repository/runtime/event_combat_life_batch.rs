#![expect(
    clippy::panic,
    reason = "Unexpected decisions invalidate printed migration scenarios."
)]
use super::event_cemetery::{finish_checks, prepared};
use super::event_combat_life::{attack, combat, grant, initial, life, resolve};
use super::*;

#[test]
fn mana_elk_and_forest_warrior_use_the_actual_other_trait_attacker_and_formal_target() {
    for (cards, damage) in [(MANA, 1_u8), (HUNTER, 3_u8)] {
        for card in cards {
            for (material, expected) in [("combat-high", true), ("combat-other", false)] {
                let mut game = prepared(
                    &initial(card, material, "P1", "P1"),
                    life(PlayerRef::Controller {}, 0),
                );
                attack(&mut game, "material", "P2.leader");
                resolve(&mut game, card, usize::from(expected), true);
                assert_eq!(
                    game.projection(View::Referee).unwrap()["objects"]["enemy"]["damage"],
                    if expected { damage } else { 0_u8 },
                    "the printed trait head controls the formal follower damage"
                );
                combat(&mut game);
            }
        }
    }
    let mut leader_game = prepared(
        &initial("BP01-022", "combat-high", "P1", "P1"),
        life(PlayerRef::Controller {}, 0),
    );
    attack(&mut leader_game, "material", "P2.leader");
    finish_checks(&mut leader_game);
    let pending = leader_game.internal_pending_choice().unwrap().unwrap();
    let InputDomain::Pending(options) = &pending.domain else {
        panic!("the Fairy attack creates Mana's occurrence");
    };
    assert_eq!(options.len(), 1_usize, "one actual Fairy attack");
    assert_eq!(options[0].card, "BP01-022", "the printed observer");
    let CardPlay::Awaiting(targets) = leader_game
        .answer_internal_card(
            pending.actor,
            pending.token,
            Answer::Pending(options[0].handle),
        )
        .unwrap()
    else {
        panic!("Mana asks for its formal target");
    };
    let InputDomain::Targets(domain) = &targets.domain else {
        panic!("formal enemy leader or follower targets");
    };
    assert!(
        domain.members.iter().all(|member| member.player.seat == 2),
        "only opposing targets"
    );
    assert!(
        domain.members.iter().any(|member| member.card.is_some()),
        "followers remain eligible beside the leader"
    );
    let leader = domain
        .members
        .iter()
        .find(|member| member.card.is_none())
        .unwrap();
    resolved(
        leader_game
            .answer_internal_card(
                targets.actor,
                targets.token,
                Answer::Targets(vec![leader.handle]),
            )
            .unwrap(),
    );
    finish_checks(&mut leader_game);
    let state = leader_game.projection(View::Referee).unwrap();
    assert_eq!(
        state["P2"]["leader"]["life"], 19_u8,
        "Mana's chosen leader takes one before combat"
    );
    assert_eq!(
        state["objects"]["enemy"]["damage"], 0_u8,
        "the unchosen follower takes nothing"
    );
    combat(&mut leader_game);
    let mut game = prepared(
        &initial("BP12-008", "combat-high", "P1", "P1"),
        life(PlayerRef::Controller {}, 0),
    );
    attack(&mut game, "source", "P2.leader");
    resolve(&mut game, "BP12-008", 0, true);
    combat(&mut game);
}

#[test]
fn evolved_ruby_and_needle_dragonewt_damage_the_leader_before_actual_combat() {
    for (cards, keywords, damage) in [(RUBY_EVOLVED, true, 1_u8), (NEEDLE, false, 2_u8)] {
        for card in cards {
            let action = if keywords {
                grant(&[KeywordId::Storm, KeywordId::Ward])
            } else {
                life(PlayerRef::Controller {}, 0)
            };
            let mut game = prepared(&initial(card, "combat-high", "P1", "P1"), action);
            activate(&mut game, "P1");
            finish_checks(&mut game);
            attack(&mut game, "material", "P2.leader");
            resolve(&mut game, card, 1, true);
            assert_eq!(
                game.projection(View::Referee).unwrap()["P2"]["leader"]["life"],
                20_u8.checked_sub(damage).unwrap(),
                "the printed automatic damage precedes the battle"
            );
            combat(&mut game);
            assert_eq!(
                game.projection(View::Referee).unwrap()["P2"]["leader"]["life"],
                16_u8.checked_sub(damage).unwrap(),
                "the declared attack then deals its own four damage"
            );
        }
    }
}

fn resolve_attack_token(game: &mut Game, card: &str) {
    finish_checks(game);
    let question = game.internal_pending_choice().unwrap().unwrap();
    let InputDomain::Pending(options) = &question.domain else {
        panic!("printed token occurrence");
    };
    assert_eq!(
        options.len(),
        1_usize,
        "one attack creates one printed occurrence"
    );
    assert_eq!(
        options[0].card, card,
        "the attack retains its printed token provider"
    );
    let mut progress = game
        .answer_internal_card(
            question.actor,
            question.token,
            Answer::Pending(options[0].handle),
        )
        .unwrap();
    while let CardPlay::Awaiting(declaration) = progress {
        let InputDomain::TokenCapacity { choices } = &declaration.domain else {
            panic!("the single EX token has only a capacity question");
        };
        assert_eq!(
            choices.len(),
            1_usize,
            "the available EX space admits the complete single token"
        );
        progress = game
            .answer_internal_card(
                declaration.actor,
                declaration.token,
                Answer::TokenCapacity(0),
            )
            .unwrap();
    }
    assert!(
        matches!(progress, CardPlay::Resolved(_)),
        "the real token occurrence completes"
    );
    finish_checks(game);
}

#[test]
fn dark_elf_keeps_both_its_attack_token_and_repeated_combat_stand() {
    for card in DARKELF {
        let mut value = initial(card, "combat-low", "P1", "P2");
        value["players"]["P2"]["zones"]["field"][1]["state"] =
            json!({"entered_this_turn":false,"acted":true,"power":1_u8});
        let mut game = prepared(&value, life(PlayerRef::Controller {}, 0));
        let before = game.projection(View::Referee).unwrap()["P1"]["ex"]
            .as_array()
            .unwrap()
            .len();
        for n in 1_usize..=2 {
            attack(&mut game, "source", "material");
            resolve_attack_token(&mut game, card);
            assert_eq!(
                game.projection(View::Referee).unwrap()["P1"]["ex"]
                    .as_array()
                    .unwrap()
                    .len(),
                before.checked_add(n).unwrap(),
                "each actual attack creates one real Fairy Wisp in EX"
            );
            combat(&mut game);
            resolve(&mut game, card, 1, true);
            assert_eq!(
                game.projection(View::Referee).unwrap()["objects"]["source"]["acted"],
                false,
                "each real combat damage stands the surviving Dark Elf"
            );
        }
    }
}
#[test]
fn saint_damages_the_formally_chosen_follower_after_each_actual_heal() {
    for card in SAINT {
        let mut game = prepared(
            &initial(card, "combat-high", "P1", "P1"),
            life(PlayerRef::Controller {}, 1),
        );
        for count in 1_u8..=2 {
            activate(&mut game, "P1");
            finish_checks(&mut game);
            resolve(&mut game, card, 1, true);
            let state = game.projection(View::Referee).unwrap();
            assert_eq!(
                state["P2"]["leader"]["life"], 20_u8,
                "the formal follower target does not damage the leader"
            );
            assert_eq!(
                state["objects"]["enemy"]["damage"],
                count.checked_mul(2).unwrap(),
                "each heal damages the selected opponent follower"
            );
        }
    }
}

#[test]
fn tenmei_counts_real_allies_heals_then_targets_and_cannot_repeat_its_activation() {
    for card in TENMEI {
        let mut value = initial(card, "combat-high", "P1", "P1");
        value["players"]["P1"]["zones"]["field"]
            .as_array_mut()
            .unwrap()
            .push(json!({
                "id":"tenmei-ally", "card":card, "state":{"entered_this_turn":false}
            }));
        let mut game = prepared(&value, life(PlayerRef::Controller {}, 0));
        let id = selection_with_rules().current[*card]
            .program
            .card()
            .abilities
            .iter()
            .find(|ability| ability.node.kind == AbilityKind::Activated)
            .unwrap()
            .node
            .id;
        resolved(
            game.activate_internal_card("source", id, PlayerId { seat: 1 })
                .unwrap(),
        );
        finish_checks(&mut game);
        resolve(&mut game, card, 2, true);
        resolve(&mut game, card, 1, true);
        let state = game.projection(View::Referee).unwrap();
        assert_eq!(
            state["P2"]["leader"]["life"], 18_u8,
            "the two actual allied 透京 followers are counted while combat-high is excluded"
        );
        assert_eq!(
            state["P1"]["leader"]["life"], 11_u8,
            "the same activation really heals once"
        );
        assert_eq!(
            state["objects"]["enemy"]["damage"], 4_u8,
            "one heal creates one two-damage formal-target occurrence for each actual Tenmei"
        );
        assert!(
            game.activate_internal_card("source", id, PlayerId { seat: 1 })
                .is_err(),
            "the printed once-per-turn activation cannot repeat"
        );
        assert_eq!(
            game.projection(View::Referee).unwrap(),
            state,
            "the rejected second activation changes no authority"
        );
    }
}

#[test]
fn cheetah_heals_into_real_storm_and_can_then_attack_on_its_entry_turn() {
    for card in CHEETAH {
        let mut value = initial(card, "combat-high", "P1", "P1");
        value["players"]["P1"]["zones"]["field"][0]["state"]["entered_this_turn"] = json!(true);
        let mut game = prepared(&value, life(PlayerRef::Controller {}, 1));
        assert_eq!(
            game.internal_rule_action(
                &json!({"do":"attack","attacker":"source","target":"P2.leader"})
            )
            .unwrap()
            .outcome,
            "cannot-attack",
            "the newly entered source cannot attack before its printed grant"
        );
        activate(&mut game, "P1");
        finish_checks(&mut game);
        resolve(&mut game, card, 1, true);
        assert_eq!(
            game.projection(View::Referee).unwrap()["P1"]["leader"]["life"],
            11_u8,
            "the grant follows an actual one-point heal"
        );
        attack(&mut game, "source", "P2.leader");
        combat(&mut game);
        assert!(
            game.projection(View::Referee).unwrap()["P2"]["leader"]["life"]
                .as_u64()
                .unwrap()
                < 20,
            "Storm allows the actual attack to complete on the entry turn"
        );
    }
}

const MANA: &[&str] = &["BP01-022"];

const RUBY_EVOLVED: &[&str] = &["BP03-099", "BP03-P28"];

const NEEDLE: &[&str] = &["BP07-063"];

const DARKELF: &[&str] = &["BP08-014"];

const HUNTER: &[&str] = &["BP12-008"];

const SAINT: &[&str] = &["BP18-097", "BP18-SL22"];

const TENMEI: &[&str] = &["BP18-099", "BP18-SL24"];

const CHEETAH: &[&str] = &["BP21-107"];
