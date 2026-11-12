#![expect(
    clippy::panic,
    reason = "Unexpected public questions invalidate the printed evolution scenario."
)]
use super::*;
use alloc::collections::BTreeSet;
use sve_engine::game::internal::DamageShare;
use sve_engine::game::player::{
    Command, DecisionShape, Input, Question, QuestionShape, SubmissionKind, Viewer,
};

const CARD: &str = "BP16-006";
const CARD_ID: &str = "c:d6d609a7709556d784d927fa312aa70e";
const FACE: &str = "f:a7960eb5fd4a5c849344ae37cf90caec";

#[test]
fn bp16_006_current_covers_both_lines_and_all_registered_jp_printings() {
    let selected = selection_with_rules();
    let author = inputs().2["authors"]
        .as_array()
        .unwrap()
        .iter()
        .find(|a| a["status"] == "accepted" && a["input"]["card_id"] == CARD_ID)
        .unwrap();
    let printings = author["printings"]
        .as_array()
        .unwrap()
        .iter()
        .map(|p| p.as_str().unwrap())
        .collect::<BTreeSet<_>>();
    assert_eq!(
        printings,
        BTreeSet::from(["BP16-006", "BP16-P02", "EBD01-011"])
    );
    let prototypes = serde_json::to_value(&*selected.prototypes).unwrap();
    for printing in printings {
        assert_eq!(selected.current[printing].card_id, CARD_ID);
        assert!(prototypes["programs"].get(printing).is_none());
    }
    let card = &selected.current[CARD];
    let faces = card.source_payload["faces"].as_array().unwrap();
    assert_eq!(faces.len(), 1);
    assert_eq!(faces[0]["face_id"], FACE);
    assert_eq!(card.program.card().abilities.len(), 2);
    for (ability, head, step, action, line) in [
        (0_u8, "evolve_time", 0_u8, "draw", 1),
        (0_u8, "evolve_time", 1_u8, "create", 1),
        (1_u8, "super_evolve_time", 0_u8, "damage", 2),
    ] {
        let source = card
            .source_at(&format!(
                "/cards/{CARD}/faces/{FACE}/abilities/{ability}/{head}/do/{step}/{action}"
            ))
            .unwrap();
        assert_eq!(source.source.card_id, CARD_ID);
        assert_eq!(source.source.face, FACE);
        assert_eq!(source.source.field, "effect");
        assert_eq!(source.source.line, line);
        assert_eq!(source.source.section, None);
    }
    assert_eq!(
        selected.current["BP01-T03"].card_id,
        "c:1c5a74b1e95a5d3594bae80573767824"
    );
}

fn question(game: &Game) -> Question {
    let Input::Question(question) = game.input(Viewer::P1) else {
        panic!("public question")
    };
    *question
}

fn answer(game: &mut Game, answer: Answer) {
    let current = question(game);
    assert_eq!(
        game.submit_player(
            Viewer::P1,
            Command::Answer {
                token: current.token,
                answer
            }
        )
        .kind,
        SubmissionKind::Applied,
        "the printed ability accepts the player answer"
    );
}

fn pending(game: &mut Game, line: u64, count: usize) {
    let current = question(game);
    let QuestionShape::Decision(DecisionShape::Pending(options)) = current.shape else {
        panic!("evolution pending")
    };
    assert_eq!(
        options.len(),
        count,
        "only the unresolved printed lines remain"
    );
    let selected = options
        .iter()
        .find(|o| o.card == CARD && o.line == line)
        .unwrap();
    answer(game, Answer::Pending(selected.handle));
}

fn distribute(game: &mut Game, total: u64) {
    let targets = question(game);
    let QuestionShape::Decision(DecisionShape::Targets(target_domain)) = targets.shape else {
        panic!("printed targets")
    };
    assert_eq!(
        target_domain.members.len(),
        1,
        "the opposing high HP follower is available"
    );
    assert_eq!(
        target_domain.cardinality,
        Cardinality {
            minimum: Nat(0),
            maximum: None
        },
        "the printed target count has no numeric cap"
    );
    answer(game, Answer::Targets(vec![target_domain.members[0].handle]));
    let allocation = question(game);
    let QuestionShape::Decision(DecisionShape::Distribution(allocation_domain)) = allocation.shape
    else {
        panic!("printed distribution")
    };
    assert_eq!(
        allocation_domain.total,
        Nat(total),
        "the chosen pending order determines Hand plus Field"
    );
    assert_eq!(
        allocation_domain.minimum,
        Nat(1),
        "each chosen follower takes positive damage"
    );
    assert_eq!(
        allocation_domain.members.len(),
        1,
        "all damage goes to the selected follower"
    );
    answer(
        game,
        Answer::Distribution(vec![DamageShare {
            handle: allocation_domain.members[0].handle,
            amount: Nat(total),
        }]),
    );
}

fn position() -> Value {
    let mut initial = evolution_payment::position("BP16-005", CARD);
    initial["players"]["P1"]["sep"] = json!(1_u8);
    initial["players"]["P1"]["zones"]["hand"] = json!([
        {"id":"hand-a","card":"BP01-171"}, {"id":"hand-b","card":"BP01-171"}
    ]);
    initial["players"]["P1"]["zones"]["deck"] = json!([{"id":"drawn","card":"BP01-171"}]);
    initial["players"]["P1"]["zones"]["ex"] = json!([{"id":"extra","card":"BP01-171"}]);
    initial["players"]["P2"]["zones"]["field"] = json!([{"id":"enemy","card":"BP01-080"}]);
    initial
}

fn assert_finished(game: &Game, total: u64) {
    let state = game.projection(View::Referee).unwrap();
    assert_eq!(
        state["P1"]["hand"],
        json!(["hand-a", "hand-b", "drawn"]),
        "the printed normal line draws one card"
    );
    assert_eq!(
        state["P1"]["ex"],
        json!(["extra"]),
        "EX remains outside this distribution total"
    );
    let field = state["P1"]["field"].as_array().unwrap();
    assert_eq!(
        field.len(),
        3,
        "the evolved source and two Fairy tokens remain"
    );
    for id in field.iter().filter(|id| *id != "source") {
        let number = state["objects"][id.as_str().unwrap()]["card"]
            .as_str()
            .unwrap();
        assert_eq!(
            selection_with_rules().current[number].card_id,
            "c:1c5a74b1e95a5d3594bae80573767824",
            "each created object is the canonical Fairy identity"
        );
    }
    assert_eq!(
        state["objects"]["source"]["evolved_with"], "form",
        "the selected evolved card is attached"
    );
    assert_eq!(
        state["objects"]["source"]["super_evolved"], true,
        "the public command committed super evolution"
    );
    assert_eq!(
        state["objects"]["enemy"]["damage"], total,
        "the printed total is dealt in the chosen order"
    );
    assert_eq!(
        state["P2"]["field"],
        json!(["enemy"]),
        "the high HP enemy survives without another trigger"
    );
    assert_eq!(
        state["P1"]["pp"]["current"], 9_u8,
        "the printed one PP cost is paid once"
    );
    assert_eq!(state["P1"]["sep"], 0_u8, "one SEP pays for super evolution");
    assert!(
        matches!(question(game).shape, QuestionShape::ActionWindow(_)),
        "all pending lines finish and return priority"
    );
}

#[test]
fn bp16_006_player_super_evolution_distributes_three_or_six_in_the_chosen_order() {
    for normal_first in [false, true] {
        let mut game = evolution_payment::game(&position());
        assert_eq!(
            player_evolution::evolve(&mut game, true),
            SubmissionKind::Applied
        );
        if normal_first {
            pending(&mut game, 1, 2);
        }
        pending(&mut game, 2, if normal_first { 1 } else { 2 });
        let total = if normal_first { 6 } else { 3 };
        distribute(&mut game, total);
        if !normal_first {
            pending(&mut game, 1, 1);
        }
        assert_finished(&game, total);
    }
}
