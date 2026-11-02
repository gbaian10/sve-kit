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
    evolve_with(game, 1, 1);
}

fn evolve_with(game: &mut Game, cost: u8, mode: u64) {
    assert_eq!(
        game.internal_rule_action(
            &json!({"do":"evolve","source":"source","evolve_card":"form","pay":{"pp":cost}})
        )
        .unwrap()
        .outcome,
        "resolved",
        "the real base uses its printed evolution cost"
    );
    triggers_with(game, mode);
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
fn bp20_044_fanfare_pays_another_idol_to_destroy_and_burn() {
    let mut initial = position("BP20-044");
    initial["players"]["P1"]["zones"]["field"] = json!([{"id":"idol","card":"BP05-048"}]);
    let mut game = game(&initial);
    play(&mut game, "source");
    let state = game.projection(View::Referee).unwrap();
    assert_eq!(state["P1"]["cemetery"], json!(["idol"]));
    assert_eq!(state["P2"]["cemetery"], json!(["enemy"]));
    assert_eq!(state["P2"]["leader"]["life"], 18_u8);
}

#[test]
fn bp20_080_second_mode_returns_a_cheap_nightmare_absolute() {
    let mut initial = evolved_position("BP20-079", "BP20-080");
    initial["players"]["P1"]["zones"]["cemetery"] =
        json!([{"id":"large","card":"BP01-037"},{"id":"servant","card":"BP05-080"}]);
    let mut game = game(&initial);
    evolve_with(&mut game, 1, 2);
    let state = game.projection(View::Referee).unwrap();
    assert_eq!(state["P1"]["field"], json!(["source", "servant"]));
    assert_eq!(state["objects"]["enemy"]["damage"], 0_u8);
}

#[test]
fn bp20_080_second_mode_keeps_its_target_in_the_cemetery_when_the_field_is_full() {
    let mut initial = evolved_position("BP20-079", "BP20-080");
    for index in 0..3_u8 {
        initial["players"]["P1"]["zones"]["field"]
            .as_array_mut()
            .unwrap()
            .push(json!({"id":format!("goblin{index}"),"card":"BP01-171"}));
    }
    initial["players"]["P1"]["zones"]["field"]
        .as_array_mut()
        .unwrap()
        .push(json!({"id":"amulet","card":"BP01-133"}));
    initial["players"]["P1"]["zones"]["cemetery"] = json!([{"id":"servant","card":"BP05-080"}]);
    let mut game = game(&initial);
    evolve_with(&mut game, 1, 2);
    let state = game.projection(View::Referee).unwrap();
    assert_eq!(
        state["P1"]["field"],
        json!(["source", "goblin0", "goblin1", "goblin2", "amulet"])
    );
    assert_eq!(state["P1"]["cemetery"], json!(["servant"]));
    assert_eq!(state["P1"]["evolve_deck"], json!([]));
    assert_eq!(state["P1"]["pp"]["current"], 9_u8);
}

#[test]
fn bp20_089_second_mode_pings_every_opposing_follower_and_leader() {
    let mut initial = position("BP20-089");
    initial["players"]["P2"]["zones"]["field"] =
        json!([{"id":"enemy","card":"BP01-037"},{"id":"small","card":"BP01-171"}]);
    let mut game = game(&initial);
    play_with(&mut game, "source", 2);
    let state = game.projection(View::Referee).unwrap();
    assert_eq!(state["objects"]["enemy"]["damage"], 1_u8);
    assert_eq!(state["objects"]["small"]["damage"], 1_u8);
    assert_eq!(state["P2"]["leader"]["life"], 19_u8);
}

#[test]
fn bp20_057_fanfare_sweeps_with_another_absolute_card() {
    let mut initial = position("BP20-057");
    initial["players"]["P1"]["zones"]["ex"] = json!([{"id":"absolute","card":"BP05-058"}]);
    initial["players"]["P2"]["zones"]["field"] =
        json!([{"id":"enemy","card":"BP01-037"},{"id":"small","card":"BP01-171"}]);
    let mut game = game(&initial);
    play(&mut game, "source");
    let state = game.projection(View::Referee).unwrap();
    // Its own sweep is ability damage on its own turn, so the opposing followers take one more.
    assert_eq!(state["objects"]["source"]["damage"], 1_u8);
    assert_eq!(state["objects"]["enemy"]["damage"], 2_u8);
    assert_eq!(state["P2"]["cemetery"], json!(["small"]));
}

#[test]
fn bp20_062_awakened_sweep_raises_its_max_pp() {
    let mut initial = position("BP20-062");
    initial["players"]["P1"]["pp"] = json!({"current":9_u8,"max":9_u8});
    let mut game = game(&initial);
    play(&mut game, "source");
    let state = game.projection(View::Referee).unwrap();
    assert_eq!(state["objects"]["source"]["damage"], 1_u8);
    assert_eq!(state["objects"]["enemy"]["damage"], 1_u8);
    assert_eq!(state["P1"]["pp"]["max"], 10_u8);
}

#[test]
fn bp20_032_evolution_deals_four() {
    let mut game = game(&evolved_position("BP20-031", "BP20-032"));
    evolve(&mut game);
    let state = game.projection(View::Referee).unwrap();
    assert_eq!(state["objects"]["enemy"]["damage"], 4_u8);
}

#[test]
fn bp20_048_activation_pays_another_idol_to_hit_the_leader() {
    let mut initial = on_field("BP20-048");
    initial["players"]["P1"]["zones"]["field"]
        .as_array_mut()
        .unwrap()
        .push(json!({"id":"idol","card":"BP05-048"}));
    initial["players"]["P2"]["zones"]["field"] = json!([]);
    let mut game = game(&initial);
    assert!(activate(&mut game, "BP20-048"));
    let state = game.projection(View::Referee).unwrap();
    assert_eq!(state["P1"]["cemetery"], json!(["idol"]));
    assert_eq!(state["P2"]["leader"]["life"], 18_u8);
}

#[test]
fn bp20_092_hits_two_followers_and_the_leaders() {
    let mut initial = position("BP20-092");
    initial["players"]["P2"]["zones"]["field"] =
        json!([{"id":"enemy","card":"BP01-152"},{"id":"small","card":"BP01-171"}]);
    let mut game = game(&initial);
    play(&mut game, "source");
    let state = game.projection(View::Referee).unwrap();
    assert_eq!(state["objects"]["enemy"]["damage"], 5_u8);
    assert_eq!(state["P2"]["cemetery"], json!(["small"]));
    assert_eq!(state["P2"]["leader"]["life"], 17_u8);
}

#[test]
fn bp20_022_costs_less_per_royal_card_and_draws_on_attack() {
    let mut initial = position("BP20-022");
    initial["players"]["P1"]["pp"] = json!({"current":3_u8,"max":10_u8});
    initial["players"]["P1"]["zones"]["field"] =
        json!([{"id":"lancer1","card":"BP01-046"},{"id":"lancer2","card":"BP01-046"}]);
    initial["players"]["P2"]["zones"]["field"] = json!([{"id":"enemy","card":"BP01-171"}]);
    let mut game = game(&initial);
    play(&mut game, "source");
    let state = game.projection(View::Referee).unwrap();
    assert_eq!(state["P1"]["pp"]["current"], 0_u8);
    assert_eq!(
        game.internal_rule_action(&json!({"do":"attack","attacker":"source","target":"enemy"}))
            .unwrap()
            .outcome,
        "resolved",
        "printed Rush permits the entry-turn follower attack"
    );
    settle_combat(&mut game);
    let after = game.projection(View::Referee).unwrap();
    assert_eq!(after["P1"]["hand"], json!(["first"]));
}

#[test]
fn bp20_009_draws_when_an_opposing_follower_has_one_hp() {
    let mut initial = position("BP20-009");
    initial["players"]["P2"]["zones"]["field"] = json!([{"id":"enemy","card":"BP01-T03"}]);
    let mut game = game(&initial);
    play(&mut game, "source");
    let state = game.projection(View::Referee).unwrap();
    assert_eq!(state["P1"]["hand"], json!(["first"]));
}

#[test]
fn bp20_010_evolution_heals_when_an_opposing_follower_has_one_hp() {
    let mut initial = evolved_position("BP20-009", "BP20-010");
    initial["players"]["P2"]["zones"]["field"] = json!([{"id":"enemy","card":"BP01-T03"}]);
    let mut game = game(&initial);
    evolve(&mut game);
    let state = game.projection(View::Referee).unwrap();
    assert_eq!(state["P1"]["leader"]["life"], 22_u8);
}

#[test]
fn bp20_039_evolution_destroys_a_follower() {
    let mut game = game(&evolved_position("BP20-038", "BP20-039"));
    evolve(&mut game);
    let state = game.projection(View::Referee).unwrap();
    assert_eq!(state["P2"]["cemetery"], json!(["enemy"]));
}
