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
            InputDomain::CostMaterials(domain) => {
                assert!(
                    domain.members.iter().all(|member| member.player == actor()),
                    "payment materials must belong to the paying player"
                );
                Answer::CostMaterials(
                    domain
                        .members
                        .iter()
                        .take(take(&domain.cardinality, domain.members.len()))
                        .map(|member| member.handle)
                        .collect(),
                )
            }
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

#[test]
fn bp22_108_pays_an_amulet_to_destroy_and_grow() {
    let mut initial = position("BP22-108");
    initial["players"]["P1"]["zones"]["field"] = json!([{"id":"amulet","card":"BP01-133"}]);
    initial["players"]["P2"]["zones"]["field"]
        .as_array_mut()
        .unwrap()
        .push(json!({"id":"opponent_amulet","card":"BP01-133"}));
    let mut game = game(&initial);
    play(&mut game, "source");
    let state = game.projection(View::Referee).unwrap();
    assert_eq!(state["P1"]["cemetery"], json!(["amulet"]));
    assert_eq!(state["P2"]["cemetery"], json!(["enemy"]));
    assert_eq!(state["P2"]["field"], json!(["opponent_amulet"]));
    assert_eq!(state["objects"]["source"]["power"], 5_u8);
}
