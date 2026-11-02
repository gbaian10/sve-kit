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



#[test]
fn bp17_011_from_ex_damages_an_enemy_and_buffs_a_beast() {
    let mut initial = position("BP17-011");
    initial["players"]["P1"]["pp"] = json!({"current":1_u8,"max":10_u8});
    initial["players"]["P1"]["zones"]["field"] = json!([{"id":"beast","card":"BP01-126"}]);
    initial["players"]["P1"]["zones"]["hand"] = json!([]);
    initial["players"]["P1"]["zones"]["ex"] = json!([{"id":"source","card":"BP17-011"}]);
    let mut game = game(&initial);
    play(&mut game, "source");
    let state = game.projection(View::Referee).unwrap();
    assert_eq!(state["P1"]["pp"]["current"], 0_u8);
    assert_eq!(state["objects"]["enemy"]["damage"], 4_u8);
    assert_eq!(state["objects"]["beast"]["power"], 5_u8);
    assert_eq!(state["objects"]["beast"]["hp"], 6_u8);
}

#[test]
fn bp17_008_arrow_counters_set_the_bow_damage() {
    let mut initial = position("BP17-008");
    initial["players"]["P1"]["zones"]["field"] = json!([{"id":"alisa","card":"BP17-001"}]);
    initial["players"]["P1"]["zones"]["hand"]
        .as_array_mut()
        .unwrap()
        .extend([
            json!({"id":"elf1","card":"BP04-010"}),
            json!({"id":"elf2","card":"BP04-010"}),
        ]);
    let mut game = game(&initial);
    play(&mut game, "source");
    play(&mut game, "elf1");
    play(&mut game, "elf2");
    assert!(activate(&mut game, "BP17-008"));
    let state = game.projection(View::Referee).unwrap();
    // Two fanfare counters plus one entry counter; the second entry is past the turn limit.
    assert_eq!(state["objects"]["enemy"]["damage"], 3_u8);
    assert!(state["P1"]["cemetery"].as_array().unwrap().contains(&json!("source")));
}

#[test]
fn bp17_103_second_mode_returns_a_munyal_follower() {
    let mut initial = position("BP17-103");
    initial["players"]["P1"]["zones"]["cemetery"] = json!([{"id":"munyal","card":"BP17-095"}]);
    let mut game = game(&initial);
    play_with(&mut game, "source", 2);
    let state = game.projection(View::Referee).unwrap();
    assert_eq!(state["objects"]["enemy"]["damage"], 0_u8);
    assert_eq!(state["P1"]["field"], json!(["munyal"]));
}

#[test]
fn bp17_058_activation_pays_an_ocean_ex_card_for_power_and_storm() {
    let mut initial = on_field("BP17-058");
    initial["players"]["P1"]["zones"]["ex"] =
        json!([{"id":"ocean","card":"BP04-069"},{"id":"plain","card":"BP01-171"}]);
    let mut game = game(&initial);
    assert!(activate(&mut game, "BP17-058"));
    let state = game.projection(View::Referee).unwrap();
    assert_eq!(state["P1"]["ex"], json!(["plain"]));
    assert_eq!(state["P1"]["cemetery"], json!(["ocean"]));
    assert_eq!(state["P1"]["pp"]["current"], 8_u8);
    assert_eq!(state["objects"]["source"]["power"], 8_u8);
    assert!(
        state["objects"]["source"]["keywords"]
            .as_array()
            .unwrap()
            .contains(&json!("疾走"))
    );
}

#[test]
fn bp17_118_searched_meishia_is_cheaper_with_five_cemetery_followers() {
    let mut initial = position("BP17-118");
    initial["players"]["P1"]["pp"] = json!({"current":2_u8,"max":10_u8});
    initial["players"]["P1"]["zones"]["deck"] =
        json!([{"id":"wrong","card":"BP01-171"},{"id":"meishia","card":"BP17-110"}]);
    initial["players"]["P1"]["zones"]["cemetery"] = json!(
        (0..5_u8)
            .map(|i| json!({"id":format!("grave{i}"),"card":"BP01-171"}))
            .collect::<Vec<_>>()
    );
    let mut game = game(&initial);
    play(&mut game, "source");
    assert_eq!(
        game.projection(View::Referee).unwrap()["P1"]["ex"],
        json!(["meishia"])
    );
    // One PP remains for the two-cost follower.
    play(&mut game, "meishia");
    assert_eq!(
        game.projection(View::Referee).unwrap()["P1"]["field"],
        json!(["meishia"])
    );
}

#[test]
fn bp17_093_puts_a_seen_follower_in_ex_then_discards() {
    let mut initial = position("BP17-093");
    initial["players"]["P1"]["zones"]["hand"] =
        json!([{"id":"source","card":"BP17-093"},{"id":"spare","card":"BP01-171"}]);
    initial["players"]["P1"]["zones"]["deck"] = json!([
        {"id":"amulet","card":"BP01-133"},
        {"id":"follower","card":"BP01-171"},
        {"id":"rest","card":"BP01-171"}
    ]);
    let mut game = game(&initial);
    play(&mut game, "source");
    let state = game.projection(View::Referee).unwrap();
    assert_eq!(state["P1"]["ex"], json!(["follower"]));
    assert_eq!(state["P1"]["deck"], json!(["rest", "amulet"]));
    assert_eq!(state["P1"]["cemetery"], json!(["spare"]));
    assert_eq!(state["P1"]["hand"], json!([]));
}

#[test]
fn bp17_060_evolution_discards_two_nature_cards_for_max_pp() {
    let mut initial = evolved_position("BP17-059", "BP17-060");
    initial["players"]["P1"]["pp"] = json!({"current":5_u8,"max":5_u8});
    initial["players"]["P1"]["zones"]["hand"] = json!([
        {"id":"plain","card":"BP01-171"},
        {"id":"nature1","card":"BP07-013"},
        {"id":"nature2","card":"BP07-013"}
    ]);
    let mut game = game(&initial);
    evolve(&mut game);
    let state = game.projection(View::Referee).unwrap();
    assert_eq!(state["P1"]["cemetery"], json!(["nature1", "nature2"]));
    assert_eq!(state["P1"]["hand"], json!(["plain"]));
    assert_eq!(state["P1"]["pp"]["max"], 6_u8);
}

#[test]
fn bp17_002_reveals_one_nature_card_from_the_top_five() {
    let mut initial = position("BP17-002");
    initial["players"]["P1"]["zones"]["deck"] = json!([
        {"id":"first","card":"BP01-171"},
        {"id":"nature","card":"BP07-013"},
        {"id":"third","card":"BP01-171"},
        {"id":"fourth","card":"BP01-171"},
        {"id":"fifth","card":"BP01-171"},
        {"id":"sixth","card":"BP01-171"}
    ]);
    let mut game = game(&initial);
    play(&mut game, "source");
    let state = game.projection(View::Referee).unwrap();
    assert_eq!(state["P1"]["hand"], json!(["nature"]));
    assert_eq!(state["P1"]["deck"].as_array().unwrap()[0], "sixth");
}

#[test]
fn bp17_115_searches_another_angel_and_heals_with_five_in_cemetery() {
    let mut initial = position("BP17-115");
    initial["players"]["P1"]["zones"]["deck"] =
        json!([{"id":"same","card":"BP17-115"},{"id":"angel","card":"BP01-159"}]);
    initial["players"]["P1"]["zones"]["cemetery"] = json!(
        (0..5_u8)
            .map(|i| json!({"id":format!("grave{i}"),"card":"BP01-159"}))
            .collect::<Vec<_>>()
    );
    let mut game = game(&initial);
    play(&mut game, "source");
    let state = game.projection(View::Referee).unwrap();
    assert_eq!(state["P1"]["hand"], json!(["angel"]));
    assert_eq!(state["P1"]["leader"]["life"], 22_u8);
}

#[test]
fn bp17_026_destroys_then_pays_two_and_sacrifices_itself_to_search() {
    let mut initial = position("BP17-026");
    // The named Erika is still a prototype, so the search finds no match here.
    initial["players"]["P1"]["zones"]["deck"] = json!([{"id":"wrong","card":"BP01-171"}]);
    let mut game = game(&initial);
    play(&mut game, "source");
    assert!(activate(&mut game, "BP17-026"));
    let state = game.projection(View::Referee).unwrap();
    assert_eq!(state["P2"]["cemetery"], json!(["enemy"]));
    assert_eq!(state["P1"]["field"], json!([]));
    assert_eq!(state["P1"]["deck"], json!(["wrong"]));
    assert_eq!(state["P1"]["cemetery"], json!(["source"]));
    assert_eq!(state["P1"]["pp"]["current"], 5_u8);
}

#[test]
fn bp17_102_summons_a_marlon_and_a_cheap_follower_then_heals() {
    let mut initial = position("BP17-102");
    initial["players"]["P1"]["zones"]["deck"] = json!([
        {"id":"large","card":"BP01-037"},
        {"id":"marlon","card":"BP17-096"},
        {"id":"cheap","card":"BP01-171"}
    ]);
    let mut game = game(&initial);
    play(&mut game, "source");
    let state = game.projection(View::Referee).unwrap();
    let field = state["P1"]["field"].as_array().unwrap();
    assert!(field.contains(&json!("cheap")));
    assert!(field.contains(&json!("marlon")));
    assert!(!field.contains(&json!("large")));
    assert_eq!(state["P1"]["leader"]["life"], 22_u8);
}

#[test]
fn bp17_025_second_mode_pays_three_more_for_a_larger_nature_follower() {
    let mut initial = position("BP17-025");
    initial["players"]["P1"]["zones"]["deck"] =
        json!([{"id":"rhino","card":"BP12-018"},{"id":"small","card":"BP07-013"}]);
    let mut game = game(&initial);
    play_with(&mut game, "source", 2);
    let state = game.projection(View::Referee).unwrap();
    assert_eq!(state["P1"]["field"], json!(["rhino"]));
    assert_eq!(state["P1"]["pp"]["current"], 4_u8);
}

#[test]
fn bp17_080_second_mode_pays_six_for_three_machines() {
    let mut initial = position("BP17-080");
    initial["players"]["P1"]["zones"]["deck"] = json!([
        {"id":"m1","card":"BP07-047"},
        {"id":"m2","card":"BP07-046"},
        {"id":"m3","card":"BP07-044"}
    ]);
    let mut game = game(&initial);
    play_with(&mut game, "source", 2);
    let state = game.projection(View::Referee).unwrap();
    assert_eq!(state["P1"]["field"].as_array().unwrap().len(), 3);
    assert_eq!(state["P1"]["deck"], json!([]));
    assert_eq!(state["P1"]["pp"]["current"], 3_u8);
}

#[test]
fn bp17_084_damages_and_necrocharge_searches_only_luna() {
    let mut initial = position("BP17-084");
    // Luna has no admitted program yet, so a non-matching card stands in the deck.
    initial["players"]["P1"]["zones"]["deck"] = json!([{"id":"wrong","card":"BP01-171"}]);
    initial["players"]["P1"]["zones"]["cemetery"] = json!(
        (0..10_u8)
            .map(|i| json!({"id":format!("grave{i}"),"card":"BP01-171"}))
            .collect::<Vec<_>>()
    );
    let mut game = game(&initial);
    play(&mut game, "source");
    let state = game.projection(View::Referee).unwrap();
    assert_eq!(state["objects"]["enemy"]["damage"], 4_u8);
    assert_eq!(state["P1"]["field"], json!([]));
    assert_eq!(state["P1"]["deck"], json!(["wrong"]));
}

#[test]
fn bp17_059_fanfare_puts_a_natera_tree_on_the_field() {
    let mut game = game(&position("BP17-059"));
    play(&mut game, "source");
    let state = game.projection(View::Referee).unwrap();
    let field = state["P1"]["field"].as_array().unwrap();
    assert_eq!(field.len(), 2);
    assert!(
        field
            .iter()
            .any(|id| state["objects"][id.as_str().unwrap()]["name"] == "ナテラの大樹")
    );
}

#[test]
fn bp17_094_evolution_puts_an_ancient_artifact_in_ex() {
    let mut game = game(&evolved_position("BP17-093", "BP17-094"));
    evolve(&mut game);
    let state = game.projection(View::Referee).unwrap();
    let ex = state["P1"]["ex"].as_array().unwrap();
    assert_eq!(ex.len(), 1);
    assert_eq!(
        state["objects"][ex[0].as_str().unwrap()]["name"],
        "エンシェントアーティファクト"
    );
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

#[test]
fn bp17_001_fifth_played_card_unlocks_the_bow_search() {
    let mut initial = position("BP17-001");
    initial["players"]["P1"]["zones"]["hand"] = json!([
        {"id":"source","card":"BP17-001"},
        {"id":"g1","card":"BP01-171"},
        {"id":"g2","card":"BP01-171"},
        {"id":"g3","card":"BP01-171"},
        {"id":"circle","card":"BP01-023"}
    ]);
    initial["players"]["P1"]["zones"]["deck"] =
        json!([{"id":"wrong","card":"BP01-171"},{"id":"bow","card":"BP17-008"}]);
    let mut game = game(&initial);
    play_with(&mut game, "source", 2);
    let state = game.projection(View::Referee).unwrap();
    assert_eq!(names(&state, &state["P1"]["ex"]), ["ストームアロー"]);
    for id in ["g1", "g2", "g3", "circle"] {
        play(&mut game, id);
    }
    assert!(activate(&mut game, "BP17-001"));
    let after = game.projection(View::Referee).unwrap();
    assert!(after["P1"]["field"].as_array().unwrap().contains(&json!("bow")));
    assert_eq!(after["P1"]["deck"], json!(["wrong"]));
}

#[test]
fn bp17_103_full_field_keeps_the_target_and_cleans_up_the_spell() {
    let mut initial = position("BP17-103");
    initial["players"]["P1"]["zones"]["cemetery"] = json!([{"id":"munyal","card":"BP17-095"}]);
    initial["players"]["P1"]["zones"]["field"] = json!([]);
    for index in 0..5_u8 {
        initial["players"]["P1"]["zones"]["field"]
            .as_array_mut()
            .unwrap()
            .push(json!({"id":format!("full{index}"),"card":"BP01-171"}));
    }
    let mut game = game(&initial);
    play_with(&mut game, "source", 2);
    let state = game.projection(View::Referee).unwrap();
    assert_eq!(state["P1"]["field"].as_array().unwrap().len(), 5);
    assert_eq!(state["P1"]["cemetery"], json!(["munyal", "source"]));
}
