#![expect(
    clippy::panic,
    reason = "Unexpected public questions invalidate the cost scenarios."
)]
use super::*;
use crate::player_pipeline::{answer, end_command, question};
use std::{path::PathBuf, process::Command as ProcessCommand, sync::OnceLock};
use sve_engine::game::player::observation::{InstalledChange, NumberChange, ObservedZone};
use sve_engine::game::player::{
    Command, DecisionShape, Input, MainActionAnswer, MainIntentKind, QuestionShape, SubmissionKind,
    Viewer,
};

fn program() -> &'static WellFormedCardProgram {
    static PROGRAM: OnceLock<WellFormedCardProgram> = OnceLock::new();
    PROGRAM.get_or_init(|| {
        let root = PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../..");
        let output = ProcessCommand::new("uv")
            .args(["run", "--directory"]).arg(root.join("carddb"))
            .args(["--locked", "--no-default-groups", "python", "-c", "import json,sys\nfrom pathlib import Path\nfrom sve_carddb.domains.effects.author_expansion import AuthorExpander\nprint(json.dumps(AuthorExpander(Path(sys.argv[1])).expand(Path(sys.argv[2]).read_bytes(), []).tree))"])
            .arg(root.join("dsl"))
            .arg(root.join("sim/engine/tests/fixtures/lowering/next-play-cost.yaml"))
            .output().unwrap();
        assert!(output.status.success(), "C0 next-play-cost fixture expansion failed");
        let tree: Value = serde_json::from_slice(&output.stdout).unwrap();
        lowerer().lower(&tree).unwrap().remove("unit-amulet").unwrap()
    })
}

fn catalog() -> Arc<Catalog> {
    let mut snapshot = Vec::new();
    let mut cards = serde_json::Map::new();
    for (number, name, kind, cost, traits) in [
        ("unit-amulet", "source", "アミュレット", "3", vec!["透京"]),
        ("tokyo", "other", "フォロワー", "3", vec!["透京"]),
        (
            "literal",
            "戒信の六銘伝・聖士郎",
            "フォロワー",
            "3",
            vec!["透京"],
        ),
        ("filler", "filler", "フォロワー", "1", vec![]),
    ] {
        snapshot.push(json!({"number":number,"faces":[{"name":name,"card_class":"ニュートラル","card_type":kind,"cost":cost,"power":if kind=="アミュレット" {"-"} else {"1"},"hp":if kind=="アミュレット" {"-"} else {"2"},"traits":traits,"text":null,"sections":[]}]}).to_string());
        cards.insert(number.into(), json!({"status":"complete","abilities":[]}));
    }
    Arc::new(
        Catalog::from_documents(
            &snapshot.join("\n"),
            &json!({"format":1,"kind":"keyword_registry","keywords":{}}).to_string(),
            &[(
                "next-cost".into(),
                json!({"format":1,"kind":"effect_set","cards":cards}).to_string(),
            )],
        )
        .unwrap(),
    )
}

fn board(pp: u8) -> Game {
    let mut initial = setup(
        1,
        json!([
            {"id":"same","card":"unit-amulet"}, {"id":"trait","card":"tokyo"},
            {"id":"named","card":"literal"}, {"id":"rest","card":"filler"}
        ]),
    );
    initial["players"]["P1"]["pp"] = json!({"current":pp,"max":10});
    initial["players"]["P1"]["zones"]["hand"] = json!([
        {"id":"source","card":"unit-amulet"}, {"id":"neutral","card":"filler"},
        {"id":"first","card":"literal"}, {"id":"second","card":"literal"}
    ]);
    initial["players"]["P2"]["zones"] = json!({"deck":[{"id":"draw","card":"filler"}]});
    let mut game = Game::new(
        catalog(),
        &initial,
        &Value::Null,
        &Value::Null,
        "next-play-cost",
    )
    .unwrap();
    game.load_internal_card("unit-amulet", program()).unwrap();
    for number in ["tokyo", "literal", "filler"] {
        game.load_internal_card(number, &card(vec![])).unwrap();
    }
    game.advance_player_input();
    game
}

fn main_command(game: &Game, number: &str, activate: bool) -> Command {
    let q = question(game, Viewer::P1);
    let QuestionShape::ActionWindow(actions) = &q.shape else {
        panic!("main actions")
    };
    let intent = actions
        .intents
        .iter()
        .find(|i| match &i.kind {
            MainIntentKind::Play { card } => !activate && card.properties.card == number,
            MainIntentKind::Activate { holder, line, .. } => {
                activate && holder.properties.card == number && *line == 2
            }
            MainIntentKind::Evolve { .. }
            | MainIntentKind::Attack { .. }
            | MainIntentKind::EndPhase => false,
        })
        .unwrap();
    Command::MainAction {
        token: q.token,
        intent: intent.id.clone(),
        answer: MainActionAnswer::None,
    }
}

fn play(game: &mut Game, number: &str) {
    let command = main_command(game, number, false);
    assert_eq!(
        game.submit_player(Viewer::P1, command).kind,
        SubmissionKind::Applied,
        "the public choice applies at this stage"
    );
}

fn grant(game: &mut Game) -> Command {
    let stale = main_command(game, "unit-amulet", false);
    play(game, "unit-amulet");
    let q = question(game, Viewer::P1);
    let QuestionShape::Decision(DecisionShape::Pending(options)) = q.shape else {
        panic!("fanfare pending")
    };
    assert_eq!(options.len(), 1, "only the source fanfare is pending");
    assert_eq!(
        answer(game, Viewer::P1, Answer::Pending(options[0].handle)).kind,
        SubmissionKind::Applied,
        "the public choice applies at this stage"
    );
    let q = question(game, Viewer::P1);
    let QuestionShape::Decision(DecisionShape::Targets(targets)) = q.shape else {
        panic!("private Look")
    };
    assert!(
        matches!(game.input(Viewer::P2), Input::Waiting(_)),
        "the opponent cannot answer the private Look"
    );
    let offered = targets
        .members
        .iter()
        .map(|m| m.card.as_deref().unwrap())
        .collect::<Vec<_>>();
    assert_eq!(
        offered,
        ["tokyo", "literal"],
        "same-name and nonmatching cards are excluded"
    );
    assert_eq!(
        answer(
            game,
            Viewer::P1,
            Answer::Targets(targets.members.iter().map(|m| m.handle).collect())
        )
        .kind,
        SubmissionKind::Applied,
        "the public choice applies at this stage"
    );
    let q = question(game, Viewer::P1);
    let QuestionShape::Decision(DecisionShape::Order(order)) = q.shape else {
        panic!("remainder order")
    };
    assert_eq!(
        order.len(),
        2,
        "only the two unselected cards need ordering"
    );
    assert_eq!(
        answer(
            game,
            Viewer::P1,
            Answer::Order(order.iter().rev().map(|m| m.handle).collect())
        )
        .kind,
        SubmissionKind::Applied,
        "the public choice applies at this stage"
    );
    stale
}

fn quote(game: &Game, number: &str) -> i64 {
    let observed = game.observe(Viewer::P1).unwrap();
    let handle = &observed
        .players
        .iter()
        .find(|p| p.player == Viewer::P1)
        .unwrap()
        .zones
        .iter()
        .find(|z| z.kind == ObservedZone::Hand)
        .unwrap()
        .cards
        .iter()
        .find(|c| c.card.properties.card == number)
        .unwrap()
        .card
        .handle;
    observed
        .costs
        .cards
        .iter()
        .find(|q| &q.card == handle)
        .unwrap()
        .cost
        .total_pp
        .unwrap()
}

fn discount_present(game: &Game) -> bool {
    game.observe(Viewer::P1)
        .unwrap()
        .players
        .iter()
        .find(|p| p.player == Viewer::P1)
        .unwrap()
        .effects
        .iter()
        .any(|e| {
            e.change
                == InstalledChange::PlayerCost {
                    change: NumberChange::Add(-2),
                    limited: true,
                }
        })
}

#[test]
fn full_look_then_next_discount_survives_source_departure_and_only_one_matching_payment() {
    let mut game = board(10);
    grant(&mut game);
    assert_eq!(
        game.projection(View::Referee).unwrap()["P1"]["pp"]["current"],
        7
    );
    let before = exact_authority(&game);
    for _ in 0..3 {
        assert_eq!(quote(&game, "literal"), 1);
        assert_eq!(quote(&game, "filler"), 1);
        question(&game, Viewer::P1);
    }
    assert_eq!(exact_authority(&game), before);
    play(&mut game, "filler");
    let command = main_command(&game, "unit-amulet", true);
    assert_eq!(
        game.submit_player(Viewer::P1, command).kind,
        SubmissionKind::Applied,
        "the public choice applies at this stage"
    );
    assert!(discount_present(&game));
    assert_eq!(quote(&game, "literal"), 1);
    let state = game.projection(View::Referee).unwrap();
    assert_eq!(state["P1"]["pp"]["current"], 5);
    assert_eq!(state["P1"]["leader"]["life"], 21);
    assert_eq!(state["P1"]["cemetery"], json!(["source"]));
    play(&mut game, "literal");
    assert_eq!(
        game.projection(View::Referee).unwrap()["P1"]["pp"]["current"],
        4
    );
    assert_eq!(quote(&game, "literal"), 3);
    play(&mut game, "literal");
    assert_eq!(
        game.projection(View::Referee).unwrap()["P1"]["pp"]["current"],
        1
    );
}

#[test]
fn discounted_candidate_is_affordable_and_a_rejected_stale_command_does_not_consume_it() {
    let mut game = board(4);
    let stale = grant(&mut game);
    assert_eq!(quote(&game, "literal"), 1);
    let command = main_command(&game, "literal", false);
    let before = exact_authority(&game);
    assert_eq!(
        game.submit_player(Viewer::P1, stale).kind,
        SubmissionKind::Rejected
    );
    assert_eq!(exact_authority(&game), before);
    assert!(discount_present(&game));
    assert_eq!(
        game.submit_player(Viewer::P1, command).kind,
        SubmissionKind::Applied,
        "the public choice applies at this stage"
    );
    assert_eq!(
        game.projection(View::Referee).unwrap()["P1"]["pp"]["current"],
        0
    );
}

#[test]
fn an_unused_next_discount_expires_when_the_granting_turn_ends() {
    let mut game = board(10);
    grant(&mut game);
    assert_eq!(quote(&game, "literal"), 1);
    let command = end_command(&question(&game, Viewer::P1));
    assert_eq!(
        game.submit_player(Viewer::P1, command).kind,
        SubmissionKind::Applied,
        "the public choice applies at this stage"
    );
    assert!(matches!(
        question(&game, Viewer::P2).shape,
        QuestionShape::Decision(DecisionShape::Quick { .. })
    ));
    assert_eq!(
        answer(&mut game, Viewer::P2, Answer::Quick(None)).kind,
        SubmissionKind::Applied,
        "the public choice applies at this stage"
    );
    assert_eq!(
        game.observe(Viewer::P1).unwrap().turn.active,
        Some(Viewer::P2)
    );
    assert!(!discount_present(&game));
    assert_eq!(quote(&game, "literal"), 3);
}
