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

fn finish(game: &mut Game, mut play: CardPlay) {
    while let CardPlay::Awaiting(question) = play {
        let answer = match &question.domain {
            InputDomain::WardEntry { .. } => Answer::WardEntry(false),
            InputDomain::OptionalCost => Answer::OptionalCost(true),
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

fn triggers(game: &mut Game) {
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
        finish(game, step);
    }
}

fn play(game: &mut Game, id: &str) {
    let step = game.play_internal_card(id, actor()).unwrap();
    finish(game, step);
    triggers(game);
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


fn end_turn(game: &mut Game) {
    assert_eq!(
        game.internal_rule_action(&json!({"do":"end-phase"}))
            .unwrap()
            .outcome,
        "resolved",
        "the controller ends its turn"
    );
    settle_combat(game);
}

fn with_field(card: &str, others: &[(&str, &str)]) -> Value {
    let mut initial = position(card);
    initial["players"]["P1"]["zones"]["field"] = json!(
        others
            .iter()
            .map(|(id, number)| json!({"id":id,"card":number}))
            .collect::<Vec<_>>()
    );
    initial["players"]["P2"]["zones"]["deck"] = json!([{"id":"oppdraw","card":"BP01-171"}]);
    initial
}

#[test]
fn bp21_058_fanfare_fields_two_dragonewts_and_the_end_sweep_follows() {
    let mut initial = with_field("BP21-058", &[]);
    initial["players"]["P1"]["zones"]["hand"] =
        json!([{"id":"source","card":"BP21-058"},{"id":"junk","card":"BP01-171"}]);
    initial["players"]["P1"]["zones"]["deck"] = json!([
        {"id":"goblin","card":"BP01-171"},
        {"id":"summoner","card":"BP03-069"},
        {"id":"silver","card":"BP06-056"}
    ]);
    let mut game = game(&initial);
    play(&mut game, "source");
    let state = game.projection(View::Referee).unwrap();
    assert_eq!(state["P1"]["cemetery"], json!(["junk"]));
    assert_eq!(state["P1"]["field"].as_array().unwrap().len(), 3);
    end_turn(&mut game);
    let after = game.projection(View::Referee).unwrap();
    assert_eq!(after["P2"]["leader"]["life"], 17_u8);
    assert_eq!(after["objects"]["enemy"]["damage"], 3_u8);
}

#[test]
fn bp21_059_awakened_fanfare_reveals_an_academy_card() {
    let mut initial = position("BP21-059");
    initial["players"]["P1"]["zones"]["deck"] = json!([
        {"id":"first","card":"BP01-171"},
        {"id":"academy","card":"BP02-046"},
        {"id":"third","card":"BP01-171"}
    ]);
    let mut game = game(&initial);
    play(&mut game, "source");
    let state = game.projection(View::Referee).unwrap();
    assert_eq!(state["P1"]["hand"], json!(["academy"]));
}

#[test]
fn bp21_062_draws_and_heals_three_at_end_beside_lumiore() {
    let initial = with_field("BP21-062", &[("lumiore", "BP21-058")]);
    let mut game = game(&initial);
    play(&mut game, "source");
    end_turn(&mut game);
    let state = game.projection(View::Referee).unwrap();
    assert_eq!(state["P1"]["hand"], json!(["first"]));
    assert_eq!(state["P1"]["leader"]["life"], 23_u8);
}

#[test]
fn bp21_089_end_phase_locks_two_followers_and_heals() {
    let mut initial = with_field("BP21-089", &[("source", "BP21-089")]);
    initial["players"]["P1"]["zones"]["hand"] = json!([]);
    initial["players"]["P2"]["zones"]["field"] =
        json!([{"id":"enemy","card":"BP01-037"},{"id":"enemy2","card":"BP01-171"}]);
    let mut game = game(&initial);
    end_turn(&mut game);
    let state = game.projection(View::Referee).unwrap();
    // Both stay acted through the opponent's start phase.
    assert_eq!(state["objects"]["enemy"]["acted"], true);
    assert_eq!(state["objects"]["enemy2"]["acted"], true);
    assert_eq!(state["P1"]["leader"]["life"], 23_u8);
}

#[test]
fn bp21_106_puts_a_spell_in_ex_that_costs_five_less_this_turn() {
    let mut initial = position("BP21-106");
    initial["players"]["P1"]["pp"] = json!({"current":7_u8,"max":10_u8});
    initial["players"]["P1"]["zones"]["deck"] =
        json!([{"id":"first","card":"BP01-171"},{"id":"lore","card":"BP01-064"}]);
    let mut game = game(&initial);
    play(&mut game, "source");
    let state = game.projection(View::Referee).unwrap();
    assert_eq!(state["P1"]["ex"], json!(["lore"]));
    play(&mut game, "lore");
    let after = game.projection(View::Referee).unwrap();
    assert_eq!(after["P1"]["pp"]["current"], 0_u8);
    assert_eq!(after["objects"]["enemy"]["damage"], 4_u8);
}

#[test]
fn bp21_111_evolution_discards_an_academy_card_to_deal_four() {
    let mut initial = on_field("BP21-110");
    initial["players"]["P1"]["zones"]["hand"] = json!([{"id":"academy","card":"BP02-046"}]);
    initial["players"]["P1"]["zones"]["evolve_deck"] = json!([{"id":"form","card":"BP21-111"}]);
    let mut game = game(&initial);
    evolve(&mut game);
    let state = game.projection(View::Referee).unwrap();
    assert_eq!(state["P1"]["cemetery"], json!(["academy"]));
    assert_eq!(state["objects"]["enemy"]["damage"], 4_u8);
}

#[test]
fn bp21_112_has_storm_at_zero_sep_and_discards_to_deal_five() {
    let mut initial = on_field("BP21-112");
    initial["players"]["P1"]["zones"]["hand"] = json!([{"id":"academy","card":"BP02-046"}]);
    let mut game = game(&initial);
    let before = game.projection(View::Referee).unwrap();
    assert!(before["objects"]["source"]["keywords"]
        .as_array()
        .unwrap()
        .contains(&json!("疾走")));
    activate(&mut game, "BP21-112");
    let state = game.projection(View::Referee).unwrap();
    assert_eq!(state["P2"]["cemetery"], json!(["enemy"]));
    assert_eq!(state["P1"]["hand"], json!(["first"]));
}

#[test]
fn bp21_113_reveals_an_academy_card_and_grows_with_three_academy_followers() {
    let mut initial = with_field(
        "BP21-113",
        &[("academy1", "BP02-046"), ("academy2", "BP02-046")],
    );
    initial["players"]["P1"]["zones"]["deck"] = json!([
        {"id":"first","card":"BP01-171"},
        {"id":"academy3","card":"BP02-046"},
        {"id":"third","card":"BP01-171"}
    ]);
    let mut game = game(&initial);
    play(&mut game, "source");
    let state = game.projection(View::Referee).unwrap();
    assert_eq!(state["P1"]["hand"], json!(["academy3"]));
    assert_eq!(state["objects"]["source"]["power"], 3_u8);
}

#[test]
fn bp21_115_heals_one_with_three_academy_followers() {
    let initial = with_field(
        "BP21-115",
        &[("academy1", "BP02-046"), ("academy2", "BP02-046")],
    );
    let mut game = game(&initial);
    play(&mut game, "source");
    let state = game.projection(View::Referee).unwrap();
    assert_eq!(state["P1"]["leader"]["life"], 21_u8);
}

#[test]
fn bp21_116_reveals_two_academy_cards_and_recovers_one_pp() {
    let mut initial = position("BP21-116");
    initial["players"]["P1"]["zones"]["deck"] = json!([
        {"id":"academy1","card":"BP02-046"},
        {"id":"goblin1","card":"BP01-171"},
        {"id":"academy2","card":"BP02-046"},
        {"id":"goblin2","card":"BP01-171"}
    ]);
    let mut game = game(&initial);
    play(&mut game, "source");
    let state = game.projection(View::Referee).unwrap();
    assert_eq!(state["P1"]["hand"], json!(["academy1", "academy2"]));
    assert_eq!(state["P1"]["pp"]["current"], 8_u8);
}

#[test]
fn bp21_p65_hits_the_opposing_leader_and_followers_for_two() {
    let mut game = game(&position("BP21-P65"));
    play(&mut game, "source");
    let state = game.projection(View::Referee).unwrap();
    assert_eq!(state["P2"]["leader"]["life"], 18_u8);
    assert_eq!(state["objects"]["enemy"]["damage"], 2_u8);
}
