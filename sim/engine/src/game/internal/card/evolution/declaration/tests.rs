use super::*;
use crate::catalog::Catalog;
use crate::game::player::{Command, Input, SubmissionKind};
use alloc::sync::Arc;

fn broken_material_question() -> Game {
    let snapshot = json!({"number":"unit","faces":[{"name":"unit","card_class":"ニュートラル","card_type":"フォロワー","cost":"0","power":"1","hp":"1","traits":[],"text":null,"sections":[]}]}).to_string();
    let registry = json!({"format":1_u8,"kind":"keyword_registry","keywords":{}}).to_string();
    let document = json!({"format":1_u8,"kind":"effect_set","cards":{"unit":{"status":"complete","review":"synthetic","abilities":[]}}}).to_string();
    let catalog = Arc::new(
        Catalog::from_documents(&snapshot, &registry, &[("unit.yaml".into(), document)]).unwrap(),
    );
    let initial = json!({"history":"explicit","turn":{"active":"P1","first_player":"P1","phase":"main","elapsed_turns":{"P1":3_u8,"P2":2_u8}},"players":{"P1":{"leader":{"class":"ニュートラル","life":20_u8},"pp":{"current":1_u8,"max":1_u8},"ep":0_u8,"sep":0_u8,"zones":{"field":[{"id":"s","card":"unit"}]}},"P2":{"leader":{"class":"ニュートラル","life":20_u8},"pp":{"current":1_u8,"max":1_u8},"ep":0_u8,"sep":0_u8,"zones":{}}}});
    let mut game = Game::new(
        catalog,
        &initial,
        &Value::Null,
        &Value::Null,
        "broken-evolution-question",
    )
    .unwrap();
    let source = Context {
        source: "s".into(),
        generation: Generation(game.object("s").unwrap().generation),
        controller: Viewer::P1.player(),
    };
    let selection = game.new_card_execution(source.clone()).unwrap();
    game.player_evolution = Some(Box::new(Declaration {
        actor: Viewer::P1,
        source,
        definition: AbilityId(1),
        code: Value::Null,
        play: PlaySpec {
            limit: None,
            captures: vec![],
            parameters: vec![],
            costs: CostSpec::Ordered { steps: vec![] },
        },
        selection,
        form: None,
        parameter: 0,
        materials: vec![],
        discard: vec![],
        hand_done: false,
        stage: Stage::Materials,
    }));
    game
}

#[test]
fn unavailable_evolution_question_faults_both_viewers_persistently_but_allows_concession() {
    let mut game = broken_material_question();
    game.advance_player_input();
    for viewer in [Viewer::P1, Viewer::P2] {
        assert_eq!(
            game.input(viewer),
            Input::EngineFault,
            "Authority handoff makes a broken evolution question a persistent fault for either seat"
        );
    }
    let stopped = format!("{game:?}");
    game.advance_player_input();
    assert_eq!(
        format!("{game:?}"),
        stopped,
        "Reentering handoff does not advance the damaged continuation"
    );
    assert_eq!(
        game.submit_player(Viewer::P1, Command::Concede).kind,
        SubmissionKind::Applied,
        "Concession remains available after an engine fault"
    );
    assert_eq!(
        game.input(Viewer::P2),
        Input::Finished {
            winner: Some(Viewer::P2)
        },
        "Concession clears the broken continuation and produces the native terminal result"
    );
}
