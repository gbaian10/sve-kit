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

fn evolved_position(base: &str, form: &str) -> Value {
    let mut initial = on_field(base);
    initial["players"]["P1"]["zones"]["evolve_deck"] = json!([{"id":"form","card":form}]);
    initial
}

fn take(cardinality: &Cardinality, available: usize) -> usize {
    cardinality
        .maximum
        .map_or(available, |most| usize::try_from(most.0).unwrap())
        .min(available)
}

fn finish_with(game: &mut Game, mut play: CardPlay, mode: u64) {
    while let CardPlay::Awaiting(question) = play {
        let answer = match &question.domain {
            InputDomain::WardEntry { .. } => Answer::WardEntry(false),
            InputDomain::OptionalCost => Answer::OptionalCost(true),
            InputDomain::Modes { .. } => Answer::Modes(vec![ModeId(mode)]),
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
                    // Each printed group greedily takes cards no earlier group used.
                    let mut used = Vec::new();
                    SearchAnswer::Grouped(
                        groups
                            .iter()
                            .map(|group| {
                                let picked: Vec<u64> = group
                                    .handles
                                    .iter()
                                    .copied()
                                    .filter(|handle| !used.contains(handle))
                                    .take(take(&group.cardinality, group.handles.len()))
                                    .collect();
                                used.extend(picked.iter().copied());
                                picked
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
            InputDomain::Integer(bounds) => {
                Answer::Integer(bounds.maximum.unwrap_or(bounds.minimum))
            }
            other => panic!("unexpected decision: {other:?}"),
        };
        play = game
            .answer_internal_card(question.actor, question.token, answer)
            .unwrap();
    }
}

fn finish(game: &mut Game, play: CardPlay) {
    finish_with(game, play, 1);
}

fn triggers_with(game: &mut Game, mode: u64) {
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
        finish_with(game, step, mode);
    }
}

fn triggers(game: &mut Game) {
    triggers_with(game, 1);
}

fn play_with(game: &mut Game, id: &str, mode: u64) {
    let step = game.play_internal_card(id, actor()).unwrap();
    finish_with(game, step, mode);
    triggers_with(game, mode);
}

fn play(game: &mut Game, id: &str) {
    play_with(game, id, 1);
}

fn evolve(game: &mut Game) {
    evolve_paying(game, 1);
}

fn evolve_paying(game: &mut Game, cost: u8) {
    assert_eq!(
        game.internal_rule_action(
            &json!({"do":"evolve","source":"source","evolve_card":"form","pay":{"pp":cost}})
        )
        .unwrap()
        .outcome,
        "resolved",
        "the real base uses its printed evolution cost"
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

fn activate(game: &mut Game, card: &str) -> bool {
    let Ok(step) = game.activate_internal_card("source", printed_activation(card), actor()) else {
        return false;
    };
    finish(game, step);
    triggers(game);
    true
}





fn has_keyword(state: &Value, object: &str, keyword: &str) -> bool {
    state["objects"][object]["keywords"]
        .as_array()
        .unwrap()
        .contains(&json!(keyword))
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


#[test]
fn bp19_023_evolution_sweeps_followers_and_hits_leaders() {
    let mut game = game(&evolved_position("BP19-022", "BP19-023"));
    evolve_paying(&mut game, 2);
    let state = game.projection(View::Referee).unwrap();
    assert_eq!(state["objects"]["enemy"]["damage"], 4_u8);
    assert_eq!(state["P2"]["leader"]["life"], 18_u8);
}

#[test]
fn bp19_118_evolution_heals_four_without_a_cutthroat() {
    // No Cutthroat card has an admitted program yet, so only the unconditional heal is played out.
    let mut game = game(&evolved_position("BP19-117", "BP19-118"));
    evolve(&mut game);
    let state = game.projection(View::Referee).unwrap();
    assert_eq!(state["objects"]["enemy"]["damage"], 0_u8);
    assert_eq!(state["P1"]["leader"]["life"], 24_u8);
}

#[test]
fn bp19_089_own_fanfare_damage_is_healed_back() {
    let mut game = game(&position("BP19-089"));
    let step = game.play_internal_card("source", actor()).unwrap();
    finish(&mut game, step);
    // Record the leader after each pending ability: the fanfare hits, then the decrease heals.
    let mut lives = Vec::new();
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
        let next = game
            .answer_internal_card(
                question.actor,
                question.token,
                Answer::Pending(options[0].handle),
            )
            .unwrap();
        finish(&mut game, next);
        lives.push(game.projection(View::Referee).unwrap()["P1"]["leader"]["life"].clone());
    }
    assert_eq!(lives, [json!(19_u8), json!(20_u8)]);
}

#[test]
fn bp19_120_storm_attack_without_a_cutthroat_hits_only_the_leader() {
    // No Cutthroat card has an admitted program yet, so the conditional damage stays off.
    let mut game = game(&on_field("BP19-120"));
    assert_eq!(
        game.internal_rule_action(&json!({"do":"attack","attacker":"source","target":"P2.leader"}))
            .unwrap()
            .outcome,
        "resolved",
        "printed Storm permits the leader attack"
    );
    settle_combat(&mut game);
    let state = game.projection(View::Referee).unwrap();
    assert_eq!(state["objects"]["enemy"]["damage"], 0_u8);
    assert_eq!(state["P2"]["leader"]["life"], 17_u8);
}

#[test]
fn bp19_051_evolution_hits_the_only_opposing_leader() {
    let mut initial = evolved_position("BP19-050", "BP19-051");
    initial["players"]["P2"]["zones"]["field"] = json!([]);
    let mut game = game(&initial);
    evolve(&mut game);
    let state = game.projection(View::Referee).unwrap();
    assert_eq!(state["P2"]["leader"]["life"], 18_u8);
}

#[test]
fn bp19_103_pays_an_amulet_to_ping_a_follower() {
    let mut initial = on_field("BP19-103");
    initial["players"]["P1"]["zones"]["field"]
        .as_array_mut()
        .unwrap()
        .push(json!({"id":"wish","card":"BP01-133"}));
    let mut game = game(&initial);
    assert!(activate(&mut game, "BP19-103"));
    let state = game.projection(View::Referee).unwrap();
    assert_eq!(state["objects"]["enemy"]["damage"], 1_u8);
    assert_eq!(state["P1"]["pp"]["current"], 9_u8);
    assert_eq!(state["P1"]["cemetery"].as_array().unwrap().len(), 1);
}

#[test]
fn bp19_021_five_royal_followers_banish_the_opposing_field() {
    let mut initial = position("BP19-021");
    initial["players"]["P1"]["zones"]["field"] = json!(
        (0..4_u8)
            .map(|i| json!({"id":format!("lancer{i}"),"card":"BP01-046"}))
            .collect::<Vec<_>>()
    );
    let mut game = game(&initial);
    play(&mut game, "source");
    let state = game.projection(View::Referee).unwrap();
    assert_eq!(state["P2"]["field"], json!([]));
    assert_eq!(state["P2"]["banish"], json!(["enemy"]));
}

#[test]
fn bp19_059_three_martial_dragonewts_give_storm() {
    let mut initial = position("BP19-059");
    initial["players"]["P1"]["zones"]["field"] =
        json!([{"id":"blade1","card":"BP09-065"},{"id":"blade2","card":"BP09-065"}]);
    let mut game = game(&initial);
    play(&mut game, "source");
    let state = game.projection(View::Referee).unwrap();
    assert!(has_keyword(&state, "source", "疾走"));
}

#[test]
fn bp19_017_returns_another_own_card_to_hand() {
    let mut initial = position("BP19-017");
    initial["players"]["P1"]["zones"]["field"] = json!([{"id":"goblin","card":"BP01-171"}]);
    let mut game = game(&initial);
    play(&mut game, "source");
    let state = game.projection(View::Referee).unwrap();
    assert_eq!(state["P1"]["hand"], json!(["goblin"]));
    assert_eq!(state["P1"]["field"], json!(["source"]));
}

#[test]
fn bp19_009_combo_three_evolves_itself() {
    let mut initial = position("BP19-009");
    initial["players"]["P1"]["zones"]["hand"] = json!([
        {"id":"g1","card":"BP01-171"},
        {"id":"g2","card":"BP01-171"},
        {"id":"source","card":"BP19-009"}
    ]);
    initial["players"]["P1"]["zones"]["evolve_deck"] = json!([{"id":"form","card":"BP19-010"}]);
    let mut game = game(&initial);
    for id in ["g1", "g2", "source"] {
        play(&mut game, id);
    }
    let state = game.projection(View::Referee).unwrap();
    assert_eq!(state["P1"]["evolve_deck"], json!([]));
}

#[test]
fn bp19_010_evolution_reveals_an_eight_prison_card() {
    let mut initial = evolved_position("BP19-009", "BP19-010");
    initial["players"]["P1"]["zones"]["deck"] = json!([
        {"id":"plain","card":"BP01-171"},
        {"id":"scout","card":"BP19-030"},
        {"id":"third","card":"BP01-171"},
        {"id":"fourth","card":"BP01-171"},
        {"id":"rest","card":"BP01-171"}
    ]);
    let mut game = game(&initial);
    evolve_paying(&mut game, 2);
    let state = game.projection(View::Referee).unwrap();
    assert_eq!(state["P1"]["hand"], json!(["scout"]));
    assert_eq!(state["P1"]["deck"].as_array().unwrap()[0], "rest");
}


#[test]
fn bp19_115_draws_without_regaining_pp_absent_a_cutthroat() {
    // No Cutthroat card has an admitted program yet, so only the unconditional draw is played out.
    let mut game = game(&position("BP19-115"));
    play(&mut game, "source");
    let state = game.projection(View::Referee).unwrap();
    assert_eq!(state["P1"]["hand"], json!(["first"]));
    assert_eq!(state["P1"]["pp"]["current"], 8_u8);
}
