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
    assert_eq!(
        game.internal_rule_action(
            &json!({"do":"evolve","source":"source","evolve_card":"form","pay":{"pp":1_u8}})
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



fn names(state: &Value, zone: &Value) -> Vec<String> {
    zone.as_array()
        .unwrap()
        .iter()
        .map(|id| {
            state["objects"][id.as_str().unwrap()]["name"]
                .as_str()
                .unwrap()
                .to_owned()
        })
        .collect()
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

#[test]
fn bp19_044_puts_a_seen_machine_in_ex_and_destroys_beside_a_big_machine() {
    let mut initial = position("BP19-044");
    initial["players"]["P1"]["zones"]["deck"] = json!([{"id":"machine","card":"BP07-047"}]);
    initial["players"]["P1"]["zones"]["field"] = json!([{"id":"kilzael","card":"BP19-041"}]);
    let mut game = game(&initial);
    play(&mut game, "source");
    let state = game.projection(View::Referee).unwrap();
    assert_eq!(state["P1"]["ex"], json!(["machine"]));
    assert!(activate(&mut game, "BP19-044"));
    let after = game.projection(View::Referee).unwrap();
    assert_eq!(after["P2"]["cemetery"], json!(["enemy"]));
}

#[test]
fn bp19_063_reveals_an_eight_prison_card_from_the_top_four() {
    let mut initial = position("BP19-063");
    initial["players"]["P1"]["zones"]["deck"] = json!([
        {"id":"plain","card":"BP01-171"},
        {"id":"scout","card":"BP19-030"},
        {"id":"third","card":"BP01-171"},
        {"id":"fourth","card":"BP01-171"},
        {"id":"rest","card":"BP01-171"}
    ]);
    let mut game = game(&initial);
    play(&mut game, "source");
    let state = game.projection(View::Referee).unwrap();
    assert_eq!(state["P1"]["hand"], json!(["scout"]));
    assert_eq!(state["P1"]["deck"].as_array().unwrap()[0], "rest");
}

#[test]
fn bp19_025_first_mode_summons_another_loyal_warden() {
    let mut initial = evolved_position("BP19-024", "BP19-025");
    initial["players"]["P1"]["zones"]["deck"] =
        json!([{"id":"plain","card":"BP01-171"},{"id":"warden","card":"BP19-024"}]);
    let mut game = game(&initial);
    evolve(&mut game);
    let state = game.projection(View::Referee).unwrap();
    assert_eq!(state["P1"]["field"], json!(["source", "warden"]));
}

#[test]
fn bp19_119_second_mode_regains_eight_pp() {
    let mut game = game(&position("BP19-119"));
    play_with(&mut game, "source", 2);
    let state = game.projection(View::Referee).unwrap();
    assert_eq!(state["P1"]["pp"]["current"], 10_u8);
}

#[test]
fn bp19_101_heals_one_at_own_end_phase() {
    let mut initial = on_field("BP19-101");
    initial["players"]["P2"]["zones"]["deck"] = json!([{"id":"oppdraw","card":"BP01-171"}]);
    let mut game = game(&initial);
    end_turn(&mut game);
    let state = game.projection(View::Referee).unwrap();
    assert_eq!(state["P1"]["leader"]["life"], 21_u8);
}

#[test]
fn bp19_029_evolution_searches_an_eight_prison_thief_follower() {
    let mut initial = evolved_position("BP19-028", "BP19-029");
    initial["players"]["P1"]["zones"]["deck"] =
        json!([{"id":"goblin","card":"BP01-171"},{"id":"scout","card":"BP19-030"}]);
    let mut game = game(&initial);
    evolve(&mut game);
    let state = game.projection(View::Referee).unwrap();
    assert_eq!(state["P1"]["hand"], json!(["scout"]));
}

#[test]
fn bp19_034_last_word_puts_a_golden_dagger_in_ex() {
    let mut initial = on_field("BP16-075");
    initial["players"]["P1"]["zones"]["field"]
        .as_array_mut()
        .unwrap()
        .push(json!({"id":"thief","card":"BP19-034"}));
    let mut game = game(&initial);
    assert!(activate(&mut game, "BP16-075"));
    let state = game.projection(View::Referee).unwrap();
    assert_eq!(names(&state, &state["P1"]["ex"]), ["黄金の小剣"]);
}

#[test]
fn bp19_037_two_soldiers_join_then_damage_counts_royal_followers() {
    let mut game = game(&position("BP19-037"));
    play(&mut game, "source");
    let state = game.projection(View::Referee).unwrap();
    assert_eq!(
        names(&state, &state["P1"]["field"]),
        ["シールドガーディアン", "ナイト"]
    );
    assert_eq!(state["objects"]["enemy"]["damage"], 2_u8);
}

#[test]
fn bp19_008_adds_two_puppets_even_without_matching_followers() {
    let mut game = game(&position("BP19-008"));
    play(&mut game, "source");
    let state = game.projection(View::Referee).unwrap();
    assert_eq!(names(&state, &state["P1"]["ex"]), ["操り人形", "操り人形"]);
}

#[test]
fn bp19_016_discards_a_doll_to_draw_and_add_two_puppets() {
    let mut initial = position("BP19-016");
    initial["players"]["P1"]["zones"]["hand"] =
        json!([{"id":"source","card":"BP19-016"},{"id":"doll","card":"BP05-014"}]);
    let mut game = game(&initial);
    play(&mut game, "source");
    let state = game.projection(View::Referee).unwrap();
    assert_eq!(state["P1"]["cemetery"], json!(["doll"]));
    assert_eq!(state["P1"]["hand"], json!(["first"]));
    assert_eq!(names(&state, &state["P1"]["ex"]), ["操り人形", "操り人形"]);
}

#[test]
fn bp19_026_playing_the_pirate_flag_triggers_a_ping() {
    let mut initial = position("BP19-026");
    initial["players"]["P2"]["zones"]["field"] = json!([]);
    let mut game = game(&initial);
    play(&mut game, "source");
    let state = game.projection(View::Referee).unwrap();
    let flag = state["P1"]["ex"].as_array().unwrap()[0]
        .as_str()
        .unwrap()
        .to_owned();
    play(&mut game, &flag);
    let after = game.projection(View::Referee).unwrap();
    // One from the gunner's trigger, one from the flag on an eight-prison thief.
    assert_eq!(after["P2"]["leader"]["life"], 18_u8);
}

#[test]
fn bp19_104_fanfare_tiger_gets_ward() {
    let mut game = game(&position("BP19-104"));
    play(&mut game, "source");
    let state = game.projection(View::Referee).unwrap();
    let tiger = state["P1"]["field"].as_array().unwrap()[1]
        .as_str()
        .unwrap()
        .to_owned();
    assert_eq!(state["objects"][&tiger]["name"], "ホーリータイガー");
    assert!(has_keyword(&state, &tiger, "守護"));
}

#[test]
fn bp19_105_evolution_tiger_gets_ward() {
    let mut game = game(&evolved_position("BP19-104", "BP19-105"));
    evolve(&mut game);
    let state = game.projection(View::Referee).unwrap();
    let tiger = state["P1"]["field"].as_array().unwrap()[1]
        .as_str()
        .unwrap()
        .to_owned();
    assert!(has_keyword(&state, &tiger, "守護"));
}

#[test]
fn bp19_109_fanfare_brings_a_holy_tiger() {
    let mut game = game(&position("BP19-109"));
    play(&mut game, "source");
    let state = game.projection(View::Referee).unwrap();
    assert_eq!(
        names(&state, &state["P1"]["field"]),
        ["聖獣の古跡", "ホーリータイガー"]
    );
}
