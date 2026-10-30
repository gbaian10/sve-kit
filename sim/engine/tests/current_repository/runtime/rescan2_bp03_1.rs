#![expect(
    clippy::panic,
    reason = "Unexpected decisions invalidate the printed card scenarios."
)]
use super::*;
use sve_engine::game::internal::SearchAnswer;

const fn actor() -> PlayerId {
    PlayerId { seat: 1 }
}

fn position(card: &str) -> Value {
    let mut initial = draw_position();
    initial["players"]["P1"]["zones"]["hand"] = json!([{"id":"source","card":card}]);
    for seat in ["P1", "P2"] {
        initial["players"][seat]["zones"]["deck"] = json!([{"id":format!("{seat}-first"),"card":"BP01-171"},{"id":format!("{seat}-second"),"card":"BP01-171"}]);
    }
    initial["players"]["P2"]["zones"]["field"] =
        json!([{"id":"enemy","card":"BP01-037","state":{"acted":true}}]);
    initial
}

fn field(card: &str) -> Value {
    let mut initial = position(card);
    initial["players"]["P1"]["zones"]["hand"] = json!([]);
    initial["players"]["P1"]["zones"]["field"] = json!([{"id":"source","card":card}]);
    initial
}

fn finish_checks(game: &mut Game) {
    while game.internal_card_stage() == Some(ResumeStage::CheckTiming) {
        game.resume_internal_card().unwrap();
    }
}

fn finish(game: &mut Game, mut step: CardPlay) {
    while let CardPlay::Awaiting(q) = step {
        let answer = if matches!(&q.domain, InputDomain::WardEntry { .. }) {
            Answer::WardEntry(false)
        } else if matches!(&q.domain, InputDomain::OptionalCost) {
            Answer::OptionalCost(true)
        } else if let InputDomain::Targets(d) = &q.domain {
            Answer::Targets(
                d.members
                    .iter()
                    .take(usize::try_from(d.cardinality.maximum.unwrap().0).unwrap())
                    .map(|m| m.handle)
                    .collect(),
            )
        } else if let InputDomain::CostMaterials(d) = &q.domain {
            Answer::CostMaterials(vec![d.members[0].handle])
        } else if let InputDomain::Search(d) = &q.domain {
            Answer::Search(SearchAnswer::Ungrouped(
                d.cards
                    .iter()
                    .take(usize::try_from(d.cardinality.maximum.unwrap().0).unwrap())
                    .map(|m| m.handle)
                    .collect(),
            ))
        } else if let InputDomain::Order(cards) = &q.domain {
            Answer::Order(cards.iter().rev().map(|m| m.handle).collect())
        } else {
            panic!("unexpected decision")
        };
        step = game.answer_internal_card(q.actor, q.token, answer).unwrap();
    }
}

fn play(game: &mut Game, id: &str) {
    let step = game.play_internal_card(id, actor()).unwrap();
    finish(game, step);
}

fn trigger(game: &mut Game) {
    let q = game.internal_pending_choice().unwrap().unwrap();
    let InputDomain::Pending(options) = &q.domain else {
        panic!("pending printed ability")
    };
    assert_eq!(
        options.len(),
        1,
        "only the intended printed ability is pending"
    );
    let step = game
        .answer_internal_card(q.actor, q.token, Answer::Pending(options[0].handle))
        .unwrap();
    finish(game, step);
    finish_checks(game);
}

fn activate(game: &mut Game, card: &str) {
    let id = selection().current[card]
        .program
        .card()
        .abilities
        .iter()
        .find(|a| a.node.kind == AbilityKind::Activated)
        .unwrap()
        .node
        .id;
    let step = game.activate_internal_card("source", id, actor()).unwrap();
    finish(game, step);
    finish_checks(game);
}

fn evolve(game: &mut Game, cost: u8) {
    assert_eq!(
        game.internal_rule_action(
            &json!({"do":"evolve","source":"source","evolve_card":"form","pay":{"pp":cost}})
        )
        .unwrap()
        .outcome,
        "resolved",
        "the real base evolves for its printed PP cost"
    );
}

fn quick_pass(game: &mut Game) {
    let q = game.internal_quick_choice().unwrap().unwrap();
    let step = game
        .answer_internal_card(q.actor, q.token, Answer::Quick(None))
        .unwrap();
    finish(game, step);
}

fn evolution(base: &str, form: &str) -> Value {
    let mut initial = field(base);
    initial["players"]["P1"]["zones"]["evolve_deck"] = json!([{"id":"form","card":form}]);
    initial
}

fn named(state: &Value, zone: &str, name: &str) -> usize {
    state["P1"][zone]
        .as_array()
        .unwrap()
        .iter()
        .filter(|id| state["objects"][id.as_str().unwrap()]["name"] == name)
        .count()
}

#[test]
fn bp03_016_returns_an_amulet_or_removes_a_returned_token_then_heals() {
    for card in ["BP19-108", "BP01-T03"] {
        let mut initial = position("BP03-016");
        initial["players"]["P1"]["zones"]["field"] = json!([{"id":"ally","card":card}]);
        let mut game = game(&initial);
        play(&mut game, "source");
        finish_checks(&mut game);
        let s = game.projection(View::Referee).unwrap();
        assert_eq!(
            s["P1"]["leader"]["life"], 21_u8,
            "The printed spell heals its controller by one"
        );
        assert_eq!(
            s["P1"]["field"],
            json!([]),
            "The selected card leaves the field"
        );
        assert_eq!(
            s["P1"]["hand"],
            if card == "BP19-108" {
                json!(["ally"])
            } else {
                json!([])
            },
            "returned tokens disappear while ordinary amulets remain in hand"
        );
    }
}

#[test]
fn bp03_023_attack_buffs_only_when_another_hero_is_present() {
    for other in [false, true] {
        let mut initial = field("BP03-023");
        if other {
            initial["players"]["P1"]["zones"]["field"]
                .as_array_mut()
                .unwrap()
                .push(json!({"id":"ally","card":"BP03-023"}));
        }
        let mut game = game(&initial);
        assert_eq!(
            game.internal_rule_action(
                &json!({"do":"attack","attacker":"source","target":"P2.leader"})
            )
            .unwrap()
            .outcome,
            "resolved",
            "the existing follower declares its attack"
        );
        trigger(&mut game);
        quick_pass(&mut game);
        let s = game.projection(View::Referee).unwrap();
        assert_eq!(
            s["objects"]["source"]["power"],
            if other { 3_u8 } else { 2_u8 },
            "The attack bonus requires another Hero follower"
        );
        assert_eq!(
            s["objects"]["source"]["hp"],
            if other { 3_u8 } else { 2_u8 },
            "The attack bonus also increases health"
        );
        assert_eq!(
            s["P2"]["leader"]["life"],
            if other { 17_u8 } else { 18_u8 },
            "Leader damage includes the attack-time bonus"
        );
    }
}

#[test]
fn bp03_028_fanfare_grants_storm_only_on_a_non_hand_entry() {
    for zone in ["hand", "ex"] {
        let mut initial = position("BP03-028");
        initial["players"]["P1"]["zones"]["hand"] = json!([]);
        initial["players"]["P1"]["zones"][zone] = json!([{"id":"source","card":"BP03-028"}]);
        let mut game = game(&initial);
        play(&mut game, "source");
        trigger(&mut game);
        assert_eq!(
            game.internal_rule_action(
                &json!({"do":"attack","attacker":"source","target":"P2.leader"})
            )
            .unwrap()
            .outcome,
            if zone == "hand" {
                "cannot-attack"
            } else {
                "resolved"
            },
            "only an EX entry satisfies the non-hand condition"
        );
    }
}

#[test]
fn bp03_033_bane_destroys_a_follower_with_more_remaining_health() {
    let mut game = game(&field("BP03-033"));
    assert_eq!(
        game.internal_rule_action(&json!({"do":"attack","attacker":"source","target":"enemy"}))
            .unwrap()
            .outcome,
        "resolved",
        "the Bane follower attacks the rested enemy"
    );
    quick_pass(&mut game);
    assert_eq!(
        game.projection(View::Referee).unwrap()["P2"]["cemetery"],
        json!(["enemy"]),
        "Bane destroys the opposing follower despite its remaining health"
    );
}

#[test]
fn bp03_043_evolution_counts_only_own_chess_followers() {
    let mut initial = evolution("BP03-042", "BP03-043");
    initial["players"]["P1"]["zones"]["field"]
        .as_array_mut()
        .unwrap()
        .extend([
            json!({"id":"pawn-a","card":"BP02-T04"}),
            json!({"id":"pawn-b","card":"BP02-T04"}),
            json!({"id":"goblin","card":"BP01-171"}),
            json!({"id":"amulet","card":"BP19-108"}),
        ]);
    let mut game = game(&initial);
    evolve(&mut game, 1);
    trigger(&mut game);
    let s = game.projection(View::Referee).unwrap();
    assert_eq!(
        s["objects"]["enemy"]["damage"], 3_u8,
        "Only the Queen and the two allied Chess Pawns contribute damage"
    );
}

#[test]
fn bp03_059_activation_replaces_five_power_with_ten_in_awakening() {
    for max in [6_u8, 7] {
        let mut initial = field("BP03-059");
        initial["players"]["P1"]["pp"] = json!({"current":max,"max":max});
        let mut game = game(&initial);
        activate(&mut game, "BP03-059");
        let s = game.projection(View::Referee).unwrap();
        assert_eq!(
            s["objects"]["source"]["power"],
            if max == 6 { 5_u8 } else { 10_u8 },
            "Awakening replaces the power bonus instead of adding both bonuses"
        );
        assert_eq!(
            s["objects"]["source"]["hp"], 5_u8,
            "The power-only bonus preserves printed health"
        );
        assert_eq!(
            s["objects"]["source"]["acted"], true,
            "Activation pays by resting the source"
        );
    }
}

#[test]
fn bp03_063_awakened_fanfare_buffs_and_grants_entry_turn_rush() {
    for max in [6_u8, 7] {
        let mut initial = position("BP03-063");
        initial["players"]["P1"]["pp"] = json!({"current":max,"max":max});
        initial["players"]["P2"]["zones"]["field"][0]["state"]["acted"] = json!(false);
        let mut game = game(&initial);
        play(&mut game, "source");
        trigger(&mut game);
        let s = game.projection(View::Referee).unwrap();
        assert_eq!(
            s["objects"]["source"]["power"],
            if max == 6 { 2_u8 } else { 6_u8 },
            "Awakening supplies exactly the printed power bonus"
        );
        assert_eq!(
            s["objects"]["source"]["hp"], 4_u8,
            "The Fanfare preserves printed health"
        );
        assert_eq!(
            game.internal_rule_action(&json!({"do":"attack","attacker":"source","target":"enemy"}))
                .unwrap()
                .outcome,
            if max == 6 {
                "cannot-attack"
            } else {
                "resolved"
            },
            "Awakening adds Rush; printed Assail permits the standing target"
        );
    }
}

#[test]
fn bp03_077_activation_pays_life_once_then_bane_and_lastword_resolve() {
    let mut game = game(&field("BP03-077"));
    activate(&mut game, "BP03-077");
    assert_eq!(
        game.projection(View::Referee).unwrap()["P1"]["leader"]["life"],
        18_u8,
        "Activation pays exactly two leader life"
    );
    let id = selection().current["BP03-077"]
        .program
        .card()
        .abilities
        .iter()
        .find(|a| a.node.kind == AbilityKind::Activated)
        .unwrap()
        .node
        .id;
    assert!(
        game.activate_internal_card("source", id, actor()).is_err(),
        "the printed per-turn limit rejects a second activation"
    );
    assert_eq!(
        game.projection(View::Referee).unwrap()["P1"]["leader"]["life"],
        18_u8,
        "A rejected second activation does not charge life"
    );
    assert_eq!(
        game.internal_rule_action(&json!({"do":"attack","attacker":"source","target":"enemy"}))
            .unwrap()
            .outcome,
        "resolved",
        "the newly granted Bane enters a real battle"
    );
    quick_pass(&mut game);
    trigger(&mut game);
    let s = game.projection(View::Referee).unwrap();
    assert_eq!(
        s["P2"]["cemetery"],
        json!(["enemy"]),
        "Bane destroys the opposing high-health follower"
    );
    assert_eq!(
        s["P1"]["cemetery"],
        json!(["source"]),
        "The source dies in the same battle"
    );
    assert_eq!(
        s["P1"]["leader"]["life"], 20_u8,
        "Lastword restores the two life paid for activation"
    );
}

#[test]
fn bp03_080_activation_pays_two_life_and_draws_only_once_per_turn() {
    let mut game = game(&field("BP03-080"));
    activate(&mut game, "BP03-080");
    let s = game.projection(View::Referee).unwrap();
    assert_eq!(
        s["P1"]["leader"]["life"], 18_u8,
        "Activation pays exactly two life"
    );
    assert_eq!(
        s["P1"]["hand"],
        json!(["P1-first"]),
        "Activation draws the top card"
    );
    let id = selection().current["BP03-080"]
        .program
        .card()
        .abilities
        .iter()
        .find(|a| a.node.kind == AbilityKind::Activated)
        .unwrap()
        .node
        .id;
    assert!(
        game.activate_internal_card("source", id, actor()).is_err(),
        "the printed per-turn limit prevents another draw"
    );
    let after = game.projection(View::Referee).unwrap();
    assert_eq!(
        after["P1"]["leader"]["life"], 18_u8,
        "The rejected second activation does not charge life"
    );
    assert_eq!(
        after["P1"]["hand"],
        json!(["P1-first"]),
        "The per-turn limit also prevents another draw"
    );
}

#[test]
fn bp03_093_fanfare_excludes_itself_but_lastword_buffs_every_remaining_ally() {
    let mut initial = position("BP03-093");
    initial["players"]["P1"]["zones"]["hand"]
        .as_array_mut()
        .unwrap()
        .push(json!({"id":"removal","card":"BP01-116"}));
    initial["players"]["P1"]["zones"]["field"] = json!([{"id":"ally","card":"BP01-171"}]);
    let mut game = game(&initial);
    play(&mut game, "source");
    trigger(&mut game);
    let before = game.projection(View::Referee).unwrap();
    assert_eq!(
        before["objects"]["source"]["hp"], 6_u8,
        "The Fanfare excludes the source from its health bonus"
    );
    assert_eq!(
        before["objects"]["ally"]["hp"], 4_u8,
        "The other allied follower gains two health"
    );
    assert_eq!(
        before["P1"]["leader"]["life"], 22_u8,
        "The Fanfare heals only the controlling leader"
    );
    let CardPlay::Awaiting(q) = game.play_internal_card("removal", actor()).unwrap() else {
        panic!("Soul Conversion target")
    };
    let InputDomain::Targets(d) = &q.domain else {
        panic!("own follower target")
    };
    let target = d
        .members
        .iter()
        .find(|m| m.card.as_deref() == Some("BP03-093"))
        .unwrap()
        .handle;
    let step = game
        .answer_internal_card(q.actor, q.token, Answer::Targets(vec![target]))
        .unwrap();
    finish(&mut game, step);
    finish_checks(&mut game);
    trigger(&mut game);
    let s = game.projection(View::Referee).unwrap();
    assert!(
        s["P1"]["cemetery"]
            .as_array()
            .unwrap()
            .contains(&json!("source")),
        "the actual Swan's Lastword survives departure"
    );
    assert_eq!(
        s["objects"]["ally"]["hp"], 6_u8,
        "Lastword buffs every remaining allied follower"
    );
    assert_eq!(
        s["P1"]["leader"]["life"], 24_u8,
        "The two printed triggers heal a total of four life"
    );
    assert_eq!(
        s["P2"]["leader"]["life"], 20_u8,
        "Neither trigger heals the opponent"
    );
}

#[test]
fn bp03_096_searches_a_fairy_tale_card_then_sacrifices_to_grant_rush_and_assail() {
    let mut initial = position("BP03-096");
    initial["players"]["P1"]["zones"]["field"] =
        json!([{"id":"ally","card":"BP03-033","state":{"entered_this_turn":true}}]);
    initial["players"]["P1"]["zones"]["deck"] =
        json!([{"id":"found","card":"BP03-115"},{"id":"other","card":"BP01-171"}]);
    initial["players"]["P2"]["zones"]["field"][0]["state"]["acted"] = json!(false);
    let mut game = game(&initial);
    play(&mut game, "source");
    trigger(&mut game);
    activate(&mut game, "BP03-096");
    let s = game.projection(View::Referee).unwrap();
    assert_eq!(
        s["P1"]["hand"],
        json!(["found"]),
        "The search finds the Fairy Tale card"
    );
    assert_eq!(
        s["P1"]["cemetery"],
        json!(["source"]),
        "Activation puts its source in the cemetery as a cost"
    );
    assert_eq!(
        s["P1"]["pp"]["current"], 7_u8,
        "Playing and activating the amulet pays two plus one PP"
    );
    assert_eq!(
        game.internal_rule_action(&json!({"do":"attack","attacker":"ally","target":"P2.leader"}))
            .unwrap()
            .outcome,
        "cannot-attack",
        "Rush does not permit an entry-turn leader attack"
    );
    assert_eq!(
        game.internal_rule_action(&json!({"do":"attack","attacker":"ally","target":"enemy"}))
            .unwrap()
            .outcome,
        "resolved",
        "Rush and Assail together permit the standing enemy target on entry"
    );
}

#[test]
fn bp03_104_fanfare_acts_only_an_own_amulet_to_create_a_falcon() {
    let mut initial = position("BP03-104");
    initial["players"]["P1"]["zones"]["field"] = json!([{"id":"own-amulet","card":"BP03-096"}]);
    initial["players"]["P2"]["zones"]["field"]
        .as_array_mut()
        .unwrap()
        .push(json!({"id":"enemy-amulet","card":"BP03-096"}));
    let mut game = game(&initial);
    play(&mut game, "source");
    trigger(&mut game);
    let s = game.projection(View::Referee).unwrap();
    assert_eq!(
        s["objects"]["own-amulet"]["acted"], true,
        "The Fanfare rests the selected allied amulet"
    );
    assert_eq!(
        s["objects"]["enemy-amulet"]["acted"], false,
        "The opposing amulet cannot pay the activation material"
    );
    assert_eq!(
        s["P1"]["pp"]["current"], 6_u8,
        "The Fanfare adds no PP charge to the printed play cost"
    );
    assert_eq!(
        named(&s, "field", "ホーリーファルコン"),
        1,
        "the material payment creates one Falcon"
    );
}

#[test]
fn bp03_116_free_evolution_damages_every_follower_only_enemy_leaders_and_discards_all() {
    let mut initial = evolution("BP03-115", "BP03-116");
    initial["players"]["P1"]["zones"]["field"]
        .as_array_mut()
        .unwrap()
        .push(json!({"id":"ally","card":"BP01-171"}));
    initial["players"]["P1"]["zones"]["hand"] =
        json!([{"id":"a","card":"BP01-171"},{"id":"b","card":"BP01-171"}]);
    let mut game = game(&initial);
    evolve(&mut game, 0);
    trigger(&mut game);
    let s = game.projection(View::Referee).unwrap();
    assert_eq!(
        s["P1"]["field"],
        json!([]),
        "Five damage also removes the source and its allied follower"
    );
    assert_eq!(
        s["P2"]["field"],
        json!([]),
        "Five damage removes the opposing follower"
    );
    assert_eq!(
        s["P1"]["hand"],
        json!([]),
        "Discard all empties the controller hand"
    );
    assert_eq!(
        s["P1"]["leader"]["life"], 20_u8,
        "The effect does not damage its controller leader"
    );
    assert_eq!(
        s["P2"]["leader"]["life"], 17_u8,
        "The effect damages only opposing leaders by three"
    );
    assert_eq!(
        s["P1"]["pp"]["current"], 10_u8,
        "Normal evolution of Humpty costs zero PP"
    );
}

#[test]
fn bp03_064_excludes_armed_followers_and_non_armed_spells_then_orders_all_four() {
    let mut initial = position("BP03-064");
    initial["players"]["P1"]["zones"]["deck"] = json!([{"id":"armed-follower","card":"BP03-064"},{"id":"spell","card":"BP01-007"},{"id":"third","card":"BP01-171"},{"id":"fourth","card":"BP03-033"},{"id":"tail","card":"BP01-171"}]);
    let mut game = game(&initial);
    play(&mut game, "source");
    trigger(&mut game);
    let s = game.projection(View::Referee).unwrap();
    assert_eq!(
        s["P1"]["hand"],
        json!([]),
        "No eligible Armed Spell means the optional selection takes zero cards"
    );
    assert_eq!(
        s["P1"]["deck"],
        json!(["tail", "fourth", "third", "spell", "armed-follower"]),
        "All four observed cards go below the untouched deck in the chosen order"
    );
    assert_eq!(
        s["P1"]["pp"]["current"], 8_u8,
        "The follower pays its printed two PP play cost"
    );
}

#[test]
fn bp03_095_normal_evolution_searches_only_followers_with_power_at_most_two() {
    let mut initial = position("BP03-094");
    initial["players"]["P1"]["zones"]["evolve_deck"] =
        json!([{"id":"form","card":"BP03-095"}]);
    initial["players"]["P1"]["zones"]["deck"] = json!([
        {"id":"too-strong","card":"BP01-158"},
        {"id":"spell","card":"BP01-007"},
        {"id":"good","card":"BP01-171"}
    ]);
    let mut game = game(&initial);
    play(&mut game, "source");
    finish_checks(&mut game);
    evolve(&mut game, 2);
    let pending = game.internal_pending_choice().unwrap().unwrap();
    let InputDomain::Pending(options) = &pending.domain else {
        panic!("evolution ability")
    };
    assert_eq!(options.len(), 1);
    let CardPlay::Awaiting(question) = game
        .answer_internal_card(
            pending.actor,
            pending.token,
            Answer::Pending(options[0].handle),
        )
        .unwrap()
    else {
        panic!("filtered deck search")
    };
    let InputDomain::Search(domain) = &question.domain else {
        panic!("deck search domain")
    };
    assert_eq!(
        domain.cards.iter().map(|card| card.card.as_str()).collect::<Vec<_>>(),
        vec!["BP01-171"],
        "the power-two follower qualifies, while a stronger follower and a spell do not"
    );
    let step = game
        .answer_internal_card(
            question.actor,
            question.token,
            Answer::Search(SearchAnswer::Ungrouped(vec![domain.cards[0].handle])),
        )
        .unwrap();
    finish(&mut game, step);
    finish_checks(&mut game);
    let state = game.projection(View::Referee).unwrap();
    assert_eq!(state["P1"]["hand"], json!(["good"]));
    assert_eq!(state["P1"]["deck_count"], 2_u8);
    assert_eq!(state["P1"]["pp"]["current"], 7_u8);
    assert_eq!(state["objects"]["source"]["power"], 2_u8);
}

#[test]
fn bp03_065_normal_evolution_damages_only_the_opposing_leader() {
    let mut game = game(&evolution("BP03-064", "BP03-065"));
    evolve(&mut game, 1);
    trigger(&mut game);
    let s = game.projection(View::Referee).unwrap();
    assert_eq!(
        s["P1"]["leader"]["life"], 20_u8,
        "Evolution damage excludes the controlling leader"
    );
    assert_eq!(
        s["P2"]["leader"]["life"], 18_u8,
        "Evolution deals two damage to the opposing leader"
    );
    assert_eq!(
        s["objects"]["enemy"]["damage"], 0_u8,
        "Evolution does not damage opposing followers"
    );
    assert_eq!(
        s["P1"]["pp"]["current"], 9_u8,
        "The normal evolution head charges one PP"
    );
}
