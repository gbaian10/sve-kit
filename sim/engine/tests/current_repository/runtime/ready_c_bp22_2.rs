#![expect(
    clippy::panic,
    clippy::wildcard_enum_match_arm,
    reason = "Unexpected decisions invalidate these game scenarios."
)]
use super::*;
use sve_engine::game::internal::SearchAnswer;

const fn actor() -> PlayerId {
    PlayerId { seat: 1 }
}

fn position(card: &str) -> Value {
    let mut initial = draw_position();
    initial["players"]["P1"]["zones"]["hand"] = json!([{"id":"source","card":card}]);
    initial["players"]["P1"]["zones"]["deck"] = json!([
        {"id":"first","card":"BP01-171"},
        {"id":"second","card":"BP01-171"},
        {"id":"third","card":"BP01-171"},
        {"id":"fourth","card":"BP01-171"}
    ]);
    initial["players"]["P2"]["zones"]["field"] = json!([{"id":"enemy","card":"BP01-037"}]);
    initial
}

fn on_field(card: &str) -> Value {
    let mut initial = position(card);
    initial["players"]["P1"]["zones"]["hand"] = json!([]);
    initial["players"]["P1"]["zones"]["field"] = json!([{"id":"source","card":card}]);
    initial
}

fn take(cardinality: &Cardinality, available: usize) -> usize {
    cardinality
        .maximum
        .map_or(available, |most| usize::try_from(most.0).unwrap())
        .min(available)
}

fn finish_with(game: &mut Game, mut play: CardPlay, modes: &[u64]) {
    while let CardPlay::Awaiting(question) = play {
        let answer = match &question.domain {
            InputDomain::WardEntry { .. } => Answer::WardEntry(false),
            InputDomain::OptionalCost => Answer::OptionalCost(true),
            InputDomain::Modes { .. } => Answer::Modes(modes.iter().copied().map(ModeId).collect()),
            InputDomain::Search(domain) => Answer::Search(domain.groups.as_ref().map_or_else(
                || {
                    SearchAnswer::Ungrouped(
                        domain
                            .cards
                            .iter()
                            .take(take(&domain.cardinality, domain.cards.len()))
                            .map(|card| card.handle)
                            .collect(),
                    )
                },
                |groups| {
                    SearchAnswer::Grouped(
                        groups
                            .iter()
                            .map(|group| {
                                group
                                    .handles
                                    .iter()
                                    .copied()
                                    .take(take(&group.cardinality, group.handles.len()))
                                    .collect()
                            })
                            .collect(),
                    )
                },
            )),
            InputDomain::Order(cards) => {
                Answer::Order(cards.iter().map(|card| card.handle).collect())
            }
            InputDomain::CostMaterials(domain) => Answer::CostMaterials(
                domain
                    .members
                    .iter()
                    .take(take(&domain.cardinality, domain.members.len()))
                    .map(|member| member.handle)
                    .collect(),
            ),
            InputDomain::Targets(domain) => Answer::Targets(
                domain
                    .members
                    .iter()
                    .take(take(&domain.cardinality, domain.members.len()))
                    .map(|member| member.handle)
                    .collect(),
            ),
            other => panic!("unexpected decision: {other:?}"),
        };
        play = game
            .answer_internal_card(question.actor, question.token, answer)
            .unwrap();
    }
}

fn finish(game: &mut Game, play: CardPlay) {
    finish_with(game, play, &[1]);
}

fn triggers(game: &mut Game) {
    triggers_with(game, &[1]);
}

fn triggers_with(game: &mut Game, modes: &[u64]) {
    loop {
        if game.internal_card_stage() == Some(ResumeStage::CheckTiming) {
            game.resume_internal_card().unwrap();
        }
        let Some(question) = game.internal_pending_choice().unwrap() else {
            break;
        };
        let InputDomain::Pending(options) = &question.domain else {
            panic!("pending ability expected")
        };
        let step = game
            .answer_internal_card(
                question.actor,
                question.token,
                Answer::Pending(options[0].handle),
            )
            .unwrap();
        finish_with(game, step, modes);
    }
}

fn play(game: &mut Game, id: &str) {
    play_with(game, id, &[1]);
}

fn play_with(game: &mut Game, id: &str, modes: &[u64]) {
    let step = game.play_internal_card(id, actor()).unwrap();
    finish_with(game, step, modes);
    triggers_with(game, modes);
}

fn evolve(game: &mut Game) {
    assert_eq!(
        game.internal_rule_action(
            &json!({"do":"evolve","source":"source","evolve_card":"form","pay":{"pp":1_u8}})
        )
        .unwrap()
        .outcome,
        "resolved",
        "the base pays its printed evolution cost"
    );
    triggers(game);
}

fn printed_activation(card: &str) -> AbilityId {
    selection().current[card]
        .program
        .card()
        .abilities
        .iter()
        .find(|a| {
            a.node.kind == AbilityKind::Activated
                && matches!(a.node.source.origin, AbilityOrigin::Printed { .. })
        })
        .unwrap()
        .node
        .id
}

fn activate(game: &mut Game, card: &str) {
    let step = game
        .activate_internal_card("source", printed_activation(card), actor())
        .unwrap();
    finish(game, step);
    triggers(game);
}

fn settle_combat(game: &mut Game) {
    loop {
        triggers(game);
        let Some(question) = game.internal_quick_choice().unwrap() else {
            break;
        };
        let step = game
            .answer_internal_card(question.actor, question.token, Answer::Quick(None))
            .unwrap();
        finish(game, step);
    }
}


fn attack(game: &mut Game, attacker: &str, target: &str) {
    assert_eq!(
        game.internal_rule_action(&json!({"do":"attack","attacker":attacker,"target":target}))
            .unwrap()
            .outcome,
        "resolved",
        "the attack is declared"
    );
    settle_combat(game);
}

fn graves(card: &str, count: u8) -> Value {
    json!(
        (0..count)
            .map(|i| json!({"id":format!("grave{i}"),"card":card}))
            .collect::<Vec<_>>()
    )
}

#[test]
fn bp22_063_attack_deals_damage_per_ex_dragonewt() {
    let mut initial = on_field("BP22-063");
    initial["players"]["P1"]["zones"]["ex"] =
        json!([{"id":"dragon1","card":"BP03-069"},{"id":"dragon2","card":"BP03-069"}]);
    let mut game = game(&initial);
    attack(&mut game, "source", "P2.leader");
    let state = game.projection(View::Referee).unwrap();
    assert_eq!(state["objects"]["enemy"]["damage"], 2_u8);
    assert_eq!(state["P2"]["leader"]["life"], 18_u8);
}

#[test]
fn bp22_067_is_free_beside_ignis_dragon() {
    let mut initial = position("BP22-067");
    initial["players"]["P1"]["pp"] = json!({"current":0_u8,"max":10_u8});
    initial["players"]["P1"]["zones"]["field"] = json!([{"id":"ignis","card":"BP22-057"}]);
    let mut game = game(&initial);
    play(&mut game, "source");
    let state = game.projection(View::Referee).unwrap();
    assert_eq!(state["P2"]["leader"]["life"], 18_u8);
    assert_eq!(state["P1"]["hand"], json!(["first", "second"]));
}

#[test]
fn bp22_072_paying_two_more_also_pings_every_opposing_follower() {
    let mut initial = position("BP22-072");
    initial["players"]["P2"]["zones"]["field"] =
        json!([{"id":"enemy","card":"BP01-037"},{"id":"goblin","card":"BP01-037"}]);
    let mut game = game(&initial);
    play(&mut game, "source");
    let state = game.projection(View::Referee).unwrap();
    assert_eq!(state["P1"]["pp"]["current"], 6_u8);
    assert_eq!(state["P2"]["cemetery"], json!(["enemy"]));
    assert_eq!(state["objects"]["goblin"]["damage"], 1_u8);
}

#[test]
fn bp22_075_evolves_itself_at_ten_life() {
    let mut initial = position("BP22-075");
    initial["players"]["P1"]["leader"]["life"] = json!(10_u8);
    initial["players"]["P1"]["zones"]["evolve_deck"] = json!([{"id":"form","card":"BP22-076"}]);
    let mut game = game(&initial);
    play(&mut game, "source");
    let state = game.projection(View::Referee).unwrap();
    assert_eq!(state["P1"]["evolve_deck"], json!([]));
    // The evolved form destroys and heals five.
    assert_eq!(state["P2"]["cemetery"], json!(["enemy"]));
    assert_eq!(state["P1"]["leader"]["life"], 15_u8);
}

#[test]
fn bp22_076_evolution_destroys_two_and_heals_five() {
    let mut initial = on_field("BP22-075");
    initial["players"]["P1"]["zones"]["evolve_deck"] = json!([{"id":"form","card":"BP22-076"}]);
    initial["players"]["P2"]["zones"]["field"] =
        json!([{"id":"enemy","card":"BP01-037"},{"id":"goblin","card":"BP01-171"}]);
    let mut game = game(&initial);
    evolve(&mut game);
    let state = game.projection(View::Referee).unwrap();
    assert_eq!(state["P2"]["cemetery"].as_array().unwrap().len(), 2);
    assert_eq!(state["P1"]["leader"]["life"], 25_u8);
}

#[test]
fn bp22_084_sacrifices_a_follower_to_deal_five() {
    let mut initial = position("BP22-084");
    initial["players"]["P1"]["zones"]["field"] = json!([{"id":"fodder","card":"BP01-171"}]);
    let mut game = game(&initial);
    play(&mut game, "source");
    let state = game.projection(View::Referee).unwrap();
    assert_eq!(state["P1"]["cemetery"], json!(["fodder"]));
    // Five damage destroys the 5/5.
    assert_eq!(state["P2"]["cemetery"], json!(["enemy"]));
    assert_eq!(state["P2"]["leader"]["life"], 19_u8);
    assert_eq!(state["P1"]["leader"]["life"], 21_u8);
}

#[test]
fn bp22_085_third_mode_pays_one_to_field_another_copy() {
    let mut initial = position("BP22-085");
    initial["players"]["P1"]["zones"]["deck"] =
        json!([{"id":"first","card":"BP01-171"},{"id":"copy","card":"BP22-085"}]);
    let mut game = game(&initial);
    play_with(&mut game, "source", &[3]);
    let state = game.projection(View::Referee).unwrap();
    assert_eq!(state["P1"]["field"], json!(["source", "copy"]));
    // The copy's fanfare also takes the third mode and pays one more.
    assert_eq!(state["P1"]["pp"]["current"], 6_u8);
}

#[test]
fn bp22_090_necrocharge_ten_takes_both_modes() {
    let mut initial = position("BP22-090");
    initial["players"]["P1"]["zones"]["cemetery"] = graves("BP01-171", 10);
    let mut game = game(&initial);
    play_with(&mut game, "source", &[1, 2]);
    let state = game.projection(View::Referee).unwrap();
    assert_eq!(state["P1"]["cemetery"].as_array().unwrap().len(), 12);
    assert_eq!(state["P1"]["pp"]["current"], 8_u8);
}

#[test]
fn bp22_096_takes_no_combat_damage() {
    let mut game = game(&on_field("BP22-096"));
    attack(&mut game, "source", "enemy");
    let state = game.projection(View::Referee).unwrap();
    assert_eq!(state["objects"]["source"]["damage"], 0_u8);
    assert_eq!(state["P1"]["field"], json!(["source"]));
}

#[test]
fn bp22_104_evolves_itself_beside_an_amulet() {
    let mut initial = position("BP22-104");
    initial["players"]["P1"]["zones"]["field"] = json!([{"id":"amulet","card":"BP01-133"}]);
    initial["players"]["P1"]["zones"]["evolve_deck"] = json!([{"id":"form","card":"BP22-105"}]);
    let mut game = game(&initial);
    play(&mut game, "source");
    let state = game.projection(View::Referee).unwrap();
    assert_eq!(state["P1"]["evolve_deck"], json!([]));
}

#[test]
fn bp22_106_gives_rush_and_heals_when_a_follower_attacks() {
    let mut initial = on_field("BP22-106");
    initial["players"]["P1"]["zones"]["field"]
        .as_array_mut()
        .unwrap()
        .push(json!({"id":"goblin","card":"BP01-171"}));
    let mut game = game(&initial);
    let before = game.projection(View::Referee).unwrap();
    assert!(before["objects"]["goblin"]["keywords"]
        .as_array()
        .unwrap()
        .contains(&json!("突進")));
    attack(&mut game, "goblin", "P2.leader");
    let state = game.projection(View::Referee).unwrap();
    assert_eq!(state["P1"]["leader"]["life"], 22_u8);
}

#[test]
fn bp22_107_activation_grows_a_beast_by_four() {
    let mut game = game(&on_field("BP22-107"));
    activate(&mut game, "BP22-107");
    let state = game.projection(View::Referee).unwrap();
    assert_eq!(state["objects"]["source"]["power"], 5_u8);
    assert_eq!(state["objects"]["source"]["hp"], 5_u8);
    assert_eq!(state["P1"]["pp"]["current"], 6_u8);
}

#[test]
fn bp22_108_pays_an_amulet_to_destroy_and_grow() {
    let mut initial = position("BP22-108");
    initial["players"]["P1"]["zones"]["field"] = json!([{"id":"amulet","card":"BP01-133"}]);
    let mut game = game(&initial);
    play(&mut game, "source");
    let state = game.projection(View::Referee).unwrap();
    assert_eq!(state["P1"]["cemetery"], json!(["amulet"]));
    assert_eq!(state["P2"]["cemetery"], json!(["enemy"]));
    assert_eq!(state["objects"]["source"]["power"], 5_u8);
}
