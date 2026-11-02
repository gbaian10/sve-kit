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

fn graves(card: &str, count: u8) -> Value {
    json!(
        (0..count)
            .map(|i| json!({"id":format!("grave{i}"),"card":card}))
            .collect::<Vec<_>>()
    )
}

#[test]
fn bp18_123_damages_and_hits_its_leader_with_saito_on_field() {
    let mut initial = position("BP18-123");
    initial["players"]["P1"]["zones"]["field"] = json!([{"id":"saito","card":"BP18-117"}]);
    let mut game = game(&initial);
    play(&mut game, "source");
    let state = game.projection(View::Referee).unwrap();
    assert_eq!(state["objects"]["enemy"]["damage"], 2_u8);
    assert_eq!(state["P2"]["leader"]["life"], 19_u8);
}

#[test]
fn bp18_117_reveals_one_tokyo_card_from_the_top_three() {
    let mut initial = position("BP18-117");
    initial["players"]["P1"]["zones"]["deck"] = json!([
        {"id":"plain","card":"BP01-171"},
        {"id":"tokyo","card":"BP18-126"},
        {"id":"other","card":"BP01-171"},
        {"id":"rest","card":"BP01-171"}
    ]);
    let mut game = game(&initial);
    play(&mut game, "source");
    let state = game.projection(View::Referee).unwrap();
    assert_eq!(state["P1"]["hand"], json!(["tokyo"]));
    assert_eq!(state["P1"]["deck"].as_array().unwrap()[0], "rest");
}

#[test]
fn bp18_047_tenth_banished_card_heals_two() {
    let mut initial = position("BP18-047");
    initial["players"]["P1"]["zones"]["banish"] = graves("BP01-171", 9);
    let mut game = game(&initial);
    play(&mut game, "source");
    let state = game.projection(View::Referee).unwrap();
    assert_eq!(state["P1"]["banish"].as_array().unwrap().len(), 10);
    assert_eq!(state["P1"]["leader"]["life"], 22_u8);
}

#[test]
fn bp18_048_evolution_deals_two_damage() {
    let mut game = game(&evolved_position("BP18-047", "BP18-048"));
    evolve(&mut game);
    let state = game.projection(View::Referee).unwrap();
    assert_eq!(state["objects"]["enemy"]["damage"], 2_u8);
}

#[test]
fn bp18_p55_burns_by_tokyo_followers_and_heals() {
    let mut initial = position("BP18-P55");
    initial["players"]["P1"]["zones"]["hand"] = json!([]);
    initial["players"]["P1"]["zones"]["ex"] = json!([{"id":"source","card":"BP18-P55"}]);
    initial["players"]["P1"]["zones"]["field"] =
        json!([{"id":"clerk","card":"BP18-126"},{"id":"claw","card":"BP18-071"}]);
    let mut game = game(&initial);
    play(&mut game, "source");
    let state = game.projection(View::Referee).unwrap();
    assert_eq!(state["P2"]["leader"]["life"], 18_u8);
    assert_eq!(state["P1"]["leader"]["life"], 21_u8);
}

#[test]
fn bp18_034_fanfare_brings_two_knights() {
    let mut game = game(&position("BP18-034"));
    play(&mut game, "source");
    let state = game.projection(View::Referee).unwrap();
    assert_eq!(
        names(&state, &state["P1"]["field"]),
        ["蒼穹の提督・モニカ", "ナイト", "ナイト"]
    );
}

#[test]
fn bp18_035_evolution_deals_damage_equal_to_own_followers() {
    let mut initial = evolved_position("BP18-034", "BP18-035");
    initial["players"]["P1"]["zones"]["field"]
        .as_array_mut()
        .unwrap()
        .extend([
            json!({"id":"ally1","card":"BP01-171"}),
            json!({"id":"ally2","card":"BP01-171"}),
        ]);
    let mut game = game(&initial);
    evolve(&mut game);
    let state = game.projection(View::Referee).unwrap();
    assert_eq!(state["objects"]["enemy"]["damage"], 3_u8);
}

#[test]
fn bp18_110_heals_with_seishiro_and_draws() {
    let mut initial = position("BP18-110");
    initial["players"]["P1"]["zones"]["field"] = json!([{"id":"seishiro","card":"BP18-097"}]);
    let mut game = game(&initial);
    play(&mut game, "source");
    let state = game.projection(View::Referee).unwrap();
    assert_eq!(state["P1"]["leader"]["life"], 21_u8);
    assert_eq!(state["P1"]["hand"], json!(["first"]));
}

#[test]
fn bp18_109_draws_and_heals_with_a_tokyo_follower() {
    let mut initial = position("BP18-109");
    initial["players"]["P1"]["zones"]["field"] = json!([{"id":"clerk","card":"BP18-126"}]);
    let mut game = game(&initial);
    play(&mut game, "source");
    let state = game.projection(View::Referee).unwrap();
    assert_eq!(state["P1"]["hand"], json!(["first"]));
    assert_eq!(state["P1"]["leader"]["life"], 21_u8);
}

#[test]
fn bp18_105_heals_then_evolves_itself_with_seishiro() {
    let mut initial = position("BP18-105");
    initial["players"]["P1"]["zones"]["field"] = json!([{"id":"seishiro","card":"BP18-097"}]);
    initial["players"]["P1"]["zones"]["evolve_deck"] = json!([{"id":"form","card":"BP18-106"}]);
    let mut game = game(&initial);
    play(&mut game, "source");
    let state = game.projection(View::Referee).unwrap();
    // One from the fanfare and one from the evolved form's evolve trigger.
    assert_eq!(state["P1"]["leader"]["life"], 22_u8);
    assert_eq!(state["P1"]["evolve_deck"], json!([]));
}

#[test]
fn bp18_106_evolution_heals_one() {
    let mut game = game(&evolved_position("BP18-105", "BP18-106"));
    evolve_paying(&mut game, 2);
    let state = game.projection(View::Referee).unwrap();
    assert_eq!(state["P1"]["leader"]["life"], 21_u8);
}

#[test]
fn bp18_032_three_tokyo_cards_heal_two() {
    let mut initial = position("BP18-032");
    initial["players"]["P1"]["zones"]["field"] =
        json!([{"id":"clerk","card":"BP18-126"},{"id":"claw","card":"BP18-071"}]);
    let mut game = game(&initial);
    play(&mut game, "source");
    let state = game.projection(View::Referee).unwrap();
    assert_eq!(state["P1"]["leader"]["life"], 22_u8);
}

#[test]
fn bp18_033_evolution_reveals_one_tokyo_card() {
    let mut initial = evolved_position("BP18-032", "BP18-033");
    initial["players"]["P1"]["zones"]["deck"] = json!([
        {"id":"plain","card":"BP01-171"},
        {"id":"tokyo","card":"BP18-126"},
        {"id":"other","card":"BP01-171"},
        {"id":"rest","card":"BP01-171"}
    ]);
    let mut game = game(&initial);
    evolve(&mut game);
    let state = game.projection(View::Referee).unwrap();
    assert_eq!(state["P1"]["hand"], json!(["tokyo"]));
}
