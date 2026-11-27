use super::*;
use crate::battle::tests::rule_fixtures;
use crate::battle::{BattleReferences, PrintingFaceIdentity, ReferenceInput, build_observation};
use crate::catalog::Catalog;
use std::sync::OnceLock;
use sve_battle_contract::{CardRef, Nat as WireNat, observation::BattleObservation};

pub(super) struct Fixture {
    pub(super) catalog: Arc<Catalog>,
    pub(super) references: BattleReferences,
}

pub(super) fn fixture() -> &'static Fixture {
    static FIXTURE: OnceLock<Fixture> = OnceLock::new();
    FIXTURE.get_or_init(|| build(false))
}

pub(super) fn shared_fixture() -> &'static Fixture {
    static FIXTURE: OnceLock<Fixture> = OnceLock::new();
    FIXTURE.get_or_init(|| build(true))
}

fn build(shared: bool) -> Fixture {
    let base = board_fixture::fixture().unwrap();
    let mut rows = base
        .snapshot
        .lines()
        .map(|line| serde_json::from_str::<Value>(line).unwrap())
        .collect::<Vec<_>>();
    let prototype = rows
        .iter()
        .find(|row| row["number"] == "unit-follower")
        .unwrap()["faces"][0]
        .clone();
    for (number, name) in [
        ("capacity-token-a", "capacity-a"),
        ("capacity-token-b", "capacity-b"),
        ("capacity-token-c", "capacity-c"),
    ] {
        let mut front = prototype.clone();
        front["name"] = json!(name);
        front["card_type"] = json!("フォロワー・トークン");
        front["title"] = Value::Null;
        let mut back = front.clone();
        back["name"] = json!(format!("{name}-back"));
        rows.push(json!({"number":number,"faces":[front,back]}));
    }
    let snapshot = rows
        .iter()
        .map(Value::to_string)
        .collect::<Vec<_>>()
        .join("\n");
    let (_, keywords, document) = board_fixtures::catalog_documents(&json!("1"));
    let mut document: Value = serde_json::from_str(&document).unwrap();
    for number in ["capacity-token-a", "capacity-token-b", "capacity-token-c"] {
        document["cards"][number] = document["cards"]["unit-follower"].clone();
    }
    if shared {
        for number in ["capacity-token-a", "capacity-token-b"] {
            document["cards"][number]["rules_name"] = json!("capacity-shared");
        }
        document["cards"]["capacity-token-b"]["token_template"] = json!(true);
    }
    let rules = rule_fixtures::rules(
        &snapshot,
        json!([rule_fixtures::title(
            "synthetic",
            "unit-follower",
            json!([])
        )]),
        json!([]),
    );
    let catalog = Arc::new(
        rule_fixtures::catalog(
            &snapshot,
            &keywords,
            &[("synthetic".into(), document.to_string())],
            &rules.to_string(),
        )
        .unwrap(),
    );
    let mut faces = vec![];
    for row in &rows {
        let number = row["number"].as_str().unwrap();
        let count = row["faces"].as_array().unwrap().len();
        for index in 0..count {
            faces.push(PrintingFaceIdentity {
                card_no: number.into(),
                native_face: u64::try_from(index).unwrap(),
                card: CardRef {
                    printing_id: format!("capacity-print-{number}"),
                    face: WireNat::try_from(if count == 2 {
                        1_usize.checked_sub(index).unwrap()
                    } else {
                        index
                    })
                    .unwrap(),
                },
            });
        }
    }
    let references = BattleReferences::new(
        &catalog,
        ReferenceInput {
            region: "jp".into(),
            snapshot_sha256: rule_fixtures::hash(snapshot.as_bytes()),
            faces,
            wording: None,
            rules: None,
        },
    )
    .unwrap();
    Fixture {
        catalog,
        references,
    }
}

pub(super) fn board(game: &Game, actor: Viewer) -> BattleObservation {
    board_with(game, actor, fixture())
}

pub(super) fn board_with(game: &Game, actor: Viewer, fixture: &Fixture) -> BattleObservation {
    // Processing is supplied by F02A; this seam tests the real board and native decision only.
    build_observation(
        &fixture.catalog,
        &fixture.references,
        &game.observe(actor).unwrap(),
        vec![],
    )
    .unwrap()
}

pub(super) fn project_game(
    game: &Game,
    actor: Viewer,
    id: &str,
) -> (player::Question, wire::QuestionShape, Binding) {
    project_with(game, actor, id, fixture())
}

pub(super) fn project_with(
    game: &Game,
    actor: Viewer,
    id: &str,
    fixture: &Fixture,
) -> (player::Question, wire::QuestionShape, Binding) {
    let native = question(game, actor).unwrap();
    let observation = board_with(game, actor, fixture);
    let (_, shape, binding) = project(
        domain(&native).unwrap(),
        Some(Context {
            id: &QuestionId::new(id.into()),
            actor: native.actor,
            purpose: native.purpose,
            observation: &observation,
            catalog: &fixture.catalog,
            references: &fixture.references,
        }),
    )
    .unwrap();
    (native, shape, binding)
}

pub(super) fn game(actor: Viewer, body: &Value, zones: Value) -> Game {
    game_with(actor, &program(body.clone()), zones, json!({}), fixture())
}

pub(super) fn game_with(
    actor: Viewer,
    program: &WellFormedCardProgram,
    zones: Value,
    mut other_zones: Value,
    fixture: &Fixture,
) -> Game {
    let (own, other) = seats(actor);
    let mut initial = board_fixtures::setup(actor.player().seat, json!([]));
    initial["turn"]["active"] = json!(own);
    initial["players"][own]["zones"] = zones;
    other_zones["hand"] = json!([{"id":"hidden","card":"unit-spell"}]);
    initial["players"][other]["zones"] = other_zones;
    let mut game = Game::new(
        Arc::clone(&fixture.catalog),
        &initial,
        &Value::Null,
        &Value::Null,
        "decision-token-capacity",
    )
    .unwrap();
    let empty = WellFormedCardProgram::check(CardProgram {
        abilities: vec![],
        card_play_costs: vec![],
    })
    .unwrap();
    for number in [
        "unit-spell",
        "unit-amulet",
        "capacity-token-a",
        "capacity-token-b",
        "capacity-token-c",
    ] {
        game.load_internal_card(number, &empty).unwrap();
    }
    game.load_internal_card("unit-follower", program).unwrap();
    game.activate_internal_card(
        "source",
        program.card().abilities.first().unwrap().node.id,
        actor.player(),
    )
    .unwrap();
    game.advance_player_input();
    game
}
