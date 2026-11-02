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







fn graves(card: &str, count: u8) -> Value {
    json!(
        (0..count)
            .map(|i| json!({"id":format!("grave{i}"),"card":card}))
            .collect::<Vec<_>>()
    )
}

fn has_keyword(state: &Value, object: &str, keyword: &str) -> bool {
    state["objects"][object]["keywords"]
        .as_array()
        .unwrap()
        .contains(&json!(keyword))
}



#[test]
fn bp19_086_fanfare_burns_its_own_leader() {
    // Garodart has no admitted program yet, so only the unconditional half is played out.
    let mut initial = position("BP19-086");
    initial["players"]["P1"]["zones"]["evolve_deck"] = json!([{"id":"form","card":"BP19-087"}]);
    let mut game = game(&initial);
    play(&mut game, "source");
    let state = game.projection(View::Referee).unwrap();
    assert_eq!(state["P1"]["leader"]["life"], 19_u8);
    assert_eq!(state["P1"]["evolve_deck"], json!(["form"]));
}

#[test]
fn bp19_087_evolution_hits_every_leader() {
    let mut game = game(&evolved_position("BP19-086", "BP19-087"));
    evolve(&mut game);
    let state = game.projection(View::Referee).unwrap();
    assert_eq!(state["P1"]["leader"]["life"], 19_u8);
    assert_eq!(state["P2"]["leader"]["life"], 19_u8);
}

#[test]
fn bp19_036_ninja_on_field_raises_damage_to_three() {
    let mut initial = position("BP19-036");
    initial["players"]["P1"]["zones"]["field"] = json!([{"id":"ninja","card":"BP11-029"}]);
    let mut game = game(&initial);
    play(&mut game, "source");
    let state = game.projection(View::Referee).unwrap();
    assert_eq!(state["objects"]["enemy"]["damage"], 3_u8);
    assert_eq!(state["P1"]["hand"], json!(["first"]));
}

#[test]
fn bp19_018_returns_an_own_card_to_deal_six_and_two() {
    let mut initial = position("BP19-018");
    initial["players"]["P1"]["zones"]["field"] = json!([{"id":"goblin","card":"BP01-171"}]);
    initial["players"]["P2"]["zones"]["field"] = json!([{"id":"enemy","card":"BP01-152"}]);
    let mut game = game(&initial);
    play(&mut game, "source");
    let state = game.projection(View::Referee).unwrap();
    assert_eq!(state["P1"]["hand"], json!(["goblin"]));
    assert_eq!(state["objects"]["enemy"]["damage"], 6_u8);
    assert_eq!(state["P2"]["leader"]["life"], 18_u8);
}



#[test]
fn bp19_060_pays_an_ex_eight_prison_card_for_four_damage() {
    let mut initial = position("BP19-060");
    initial["players"]["P1"]["zones"]["ex"] =
        json!([{"id":"plain","card":"BP01-171"},{"id":"prisoner","card":"BP19-030"}]);
    let mut game = game(&initial);
    play(&mut game, "source");
    let state = game.projection(View::Referee).unwrap();
    assert_eq!(state["P1"]["ex"], json!(["plain"]));
    assert_eq!(state["P1"]["cemetery"], json!(["prisoner"]));
    assert_eq!(state["objects"]["enemy"]["damage"], 4_u8);
}

#[test]
fn bp19_055_damage_counts_the_hand_after_drawing() {
    let mut initial = position("BP19-055");
    initial["players"]["P1"]["zones"]["hand"] = json!([
        {"id":"source","card":"BP19-055"},
        {"id":"a","card":"BP01-171"},
        {"id":"b","card":"BP01-171"}
    ]);
    let mut game = game(&initial);
    play(&mut game, "source");
    let state = game.projection(View::Referee).unwrap();
    assert_eq!(state["objects"]["enemy"]["damage"], 3_u8);
}


#[test]
fn bp19_084_draws_without_miroel_on_field() {
    // Miroel has no admitted program yet, so only the unconditional draw is played out.
    let mut game = game(&position("BP19-084"));
    play(&mut game, "source");
    let state = game.projection(View::Referee).unwrap();
    assert_eq!(state["P1"]["hand"], json!(["first"]));
    assert_eq!(state["P1"]["leader"]["life"], 20_u8);
}


#[test]
fn bp19_078_necrocharge_gives_storm() {
    let mut initial = on_field("BP19-078");
    initial["players"]["P1"]["zones"]["cemetery"] = graves("BP01-171", 10);
    let game = game(&initial);
    let state = game.projection(View::Referee).unwrap();
    assert!(has_keyword(&state, "source", "疾走"));
}

#[test]
fn bp19_079_evolution_heals_two() {
    let mut game = game(&evolved_position("BP19-078", "BP19-079"));
    evolve(&mut game);
    let state = game.projection(View::Referee).unwrap();
    assert_eq!(state["P1"]["leader"]["life"], 22_u8);
}
