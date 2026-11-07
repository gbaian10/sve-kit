#![expect(
    clippy::panic,
    clippy::wildcard_enum_match_arm,
    reason = "Unexpected decisions invalidate the printed scenarios."
)]
use super::*;

const fn actor() -> PlayerId {
    PlayerId { seat: 1 }
}

fn finish_checks(game: &mut Game) {
    while game.internal_card_stage() == Some(ResumeStage::CheckTiming) {
        game.resume_internal_card().unwrap();
    }
}

/// Answers printed questions: targets take the first member; 【守護】 enters standing.
fn answer(game: &mut Game, step: CardPlay) {
    answer_capacity(game, step, &[]);
}

/// Like `answer`, and a full EX keeps exactly the `kept` token names (Q2146).
fn answer_capacity(game: &mut Game, mut step: CardPlay, kept: &[&str]) {
    while let CardPlay::Awaiting(q) = step {
        let reply = match &q.domain {
            InputDomain::TokenCapacity { choices } => Answer::TokenCapacity(
                u64::try_from(
                    choices
                        .iter()
                        .position(|choice| choice == kept)
                        .unwrap_or_else(|| panic!("{kept:?} among {choices:?}")),
                )
                .unwrap(),
            ),
            InputDomain::Targets(d) => {
                Answer::Targets(d.members.first().map(|m| m.handle).into_iter().collect())
            }
            InputDomain::WardEntry { .. } => Answer::WardEntry(false),
            other => panic!("unexpected printed decision: {other:?}"),
        };
        step = game.answer_internal_card(q.actor, q.token, reply).unwrap();
    }
    finish_checks(game);
}

/// Resolves every pending printed trigger in offer order; returns how many resolved.
fn drain(game: &mut Game) -> usize {
    let mut resolved = 0_usize;
    finish_checks(game);
    while let Some(q) = game.internal_pending_choice().unwrap() {
        let InputDomain::Pending(options) = &q.domain else {
            panic!("printed pending ability");
        };
        let step = game
            .answer_internal_card(q.actor, q.token, Answer::Pending(options[0].handle))
            .unwrap();
        answer(game, step);
        resolved = resolved.saturating_add(1);
    }
    resolved
}

/// Plays サタン with 10 PP and returns the game plus the EX token ids by name.
fn satan(printing: &str) -> (Game, Vec<(String, String)>) {
    let mut initial = draw_position();
    initial["players"]["P1"]["pp"] = json!({"current":10_u8,"max":10_u8});
    initial["players"]["P1"]["zones"]["hand"] = json!([{"id":"source","card":printing}]);
    let mut game = game(&initial);
    let entered = game.play_internal_card("source", actor()).unwrap();
    assert!(matches!(entered, CardPlay::Entered(_)), "{printing}: サタン enters");
    assert_eq!(drain(&mut game), 1, "{printing}: Fanfare");
    let view = game.projection(View::Referee).unwrap();
    let tokens = view["P1"]["ex"]
        .as_array()
        .unwrap()
        .iter()
        .map(|id| {
            let id = id.as_str().unwrap().to_owned();
            let name = view["objects"][id.as_str()]["name"].as_str().unwrap().to_owned();
            (name, id)
        })
        .collect();
    (game, tokens)
}

/// Plays サタン beside `occupied` EX cards and keeps `kept`; returns the new EX token names.
fn satan_into_full_ex(occupied: u8, kept: &[&str]) -> Vec<String> {
    let mut initial = draw_position();
    initial["players"]["P1"]["pp"] = json!({"current":10_u8,"max":10_u8});
    initial["players"]["P1"]["zones"]["hand"] = json!([{"id":"source","card":"BP16-113"}]);
    initial["players"]["P1"]["zones"]["ex"] = json!(
        (0..occupied)
            .map(|n| json!({"id":format!("old{n}"),"card":"BP01-037"}))
            .collect::<Vec<_>>()
    );
    let mut game = game(&initial);
    let entered = game.play_internal_card("source", actor()).unwrap();
    assert!(matches!(entered, CardPlay::Entered(_)), "サタン enters");
    finish_checks(&mut game);
    let q = game.internal_pending_choice().unwrap().unwrap();
    let InputDomain::Pending(options) = &q.domain else {
        panic!("Fanfare");
    };
    let step = game
        .answer_internal_card(q.actor, q.token, Answer::Pending(options[0].handle))
        .unwrap();
    answer_capacity(&mut game, step, kept);
    let view = game.projection(View::Referee).unwrap();
    assert_eq!(view["P1"]["leader"]["life"], 14_u8, "サタン hits its own leader");
    let mut names: Vec<_> = view["P1"]["ex"]
        .as_array()
        .unwrap()
        .iter()
        .filter(|id| !id.as_str().unwrap().starts_with("old"))
        .map(|id| view["objects"][id.as_str().unwrap()]["name"].as_str().unwrap().to_owned())
        .collect();
    names.sort_unstable();
    names
}

#[test]
fn bp16_113_q2146_a_short_ex_keeps_any_chosen_tokens() {
    assert_eq!(satan_into_full_ex(4, &["アスタロトの宣告"]), ["アスタロトの宣告"]);
    assert_eq!(
        satan_into_full_ex(3, &["サタンズサーヴァント", "アスタロトの宣告"]),
        ["アスタロトの宣告", "サタンズサーヴァント"]
    );
    assert!(satan_into_full_ex(5, &[]).is_empty(), "a full EX keeps none");
}

fn token(tokens: &[(String, String)], name: &str) -> String {
    tokens
        .iter()
        .find(|(own, _)| own == name)
        .map_or_else(|| panic!("{name} in EX: {tokens:?}"), |(_, id)| id.clone())
}

#[test]
fn bp16_113_all_printings_hit_their_own_leader_and_put_four_tokens_into_ex() {
    for printing in ["BP16-113", "BP16-SL29", "EBD04-009"] {
        let (game, tokens) = satan(printing);
        let view = game.projection(View::Referee).unwrap();
        assert_eq!(view["P1"]["leader"]["life"], 14_u8, "{printing}");
        assert_eq!(view["P2"]["leader"]["life"], 20_u8, "{printing}");
        let mut names: Vec<_> = tokens.iter().map(|(name, _)| name.as_str()).collect();
        names.sort_unstable();
        assert_eq!(
            names,
            ["アスタロトの宣告", "サタンズサーヴァント", "沈黙の魔将", "辺獄の悪鬼"],
            "{printing}"
        );
    }
}

#[test]
fn bp16_113_servant_token_enters_from_ex_without_abilities() {
    let (mut game, tokens) = satan("BP16-113");
    // サタン costs 6, leaving 4 PP for the cost-1 servant.
    let servant = token(&tokens, "サタンズサーヴァント");
    let entered = game.play_internal_card(&servant, actor()).unwrap();
    assert!(matches!(entered, CardPlay::Entered(_)), "the servant enters");
    assert_eq!(drain(&mut game), 0, "the servant has no printed ability");
    let view = game.projection(View::Referee).unwrap();
    assert!(
        view["P1"]["field"].as_array().unwrap().contains(&json!(servant)),
        "{}",
        view["P1"]
    );
    assert_eq!(view["P1"]["pp"]["current"], 3_u8);
}

#[test]
fn bp16_p33_all_printings_enter_from_ex_for_one_pp() {
    for printing in ["BP16-P33", "BP16-T07", "EBD04-T03"] {
        let mut initial = draw_position();
        initial["players"]["P1"]["pp"] = json!({"current":1_u8,"max":1_u8});
        initial["players"]["P1"]["zones"]["ex"] = json!([{"id":"servant","card":printing}]);
        let mut game = game(&initial);
        let entered = game.play_internal_card("servant", actor()).unwrap();
        assert!(matches!(entered, CardPlay::Entered(_)), "{printing}");
        assert_eq!(drain(&mut game), 0, "{printing}");
        let view = game.projection(View::Referee).unwrap();
        assert_eq!(view["P1"]["field"], json!(["servant"]), "{printing}");
        assert_eq!(view["P1"]["pp"]["current"], 0_u8, "{printing}");
    }
}

#[test]
fn bp16_p35_all_printings_set_only_the_enemy_leader_to_one() {
    for printing in ["BP16-P35", "BP16-T09", "EBD04-T05"] {
        let mut initial = draw_position();
        initial["players"]["P1"]["pp"] = json!({"current":10_u8,"max":10_u8});
        initial["players"]["P1"]["zones"]["ex"] = json!([{"id":"astaroth","card":printing}]);
        initial["players"]["P1"]["leader"]["life"] = json!(5_u8);
        let mut game = game(&initial);
        let cast = game.play_internal_card("astaroth", actor()).unwrap();
        answer(&mut game, cast);
        drain(&mut game);
        let view = game.projection(View::Referee).unwrap();
        assert_eq!(view["P2"]["leader"]["life"], 1_u8, "{printing}");
        assert_eq!(view["P1"]["leader"]["life"], 5_u8, "{printing}");
        assert_eq!(view["P1"]["pp"]["current"], 0_u8, "{printing}");
    }
}
