//! The whole tool on a synthetic public export: compose the inputs, run seeds, replay records.

#![expect(
    clippy::unwrap_used,
    reason = "A malformed synthetic export or an unwritable temporary folder invalidates these tests."
)]
#![expect(
    clippy::default_numeric_fallback,
    reason = "test fixtures are literal JSON"
)]

#[path = "../../battle-host/tests/support/fixture.rs"]
mod fixture;

use serde_json::{Value, json};
use std::env::temp_dir;
use std::fs::{create_dir_all, read_to_string, remove_dir_all, write};
use std::path::{Path, PathBuf};
use std::process::id;
use sve_selfplay::cli::command;
use sve_selfplay::play::{End, Limits, Record, play, replay};
use sve_selfplay::published::Published;

const CARDS: usize = 60;

fn number(index: usize) -> String {
    format!("t-{index:03}")
}

fn identifier(prefix: &str, index: usize) -> String {
    format!("{prefix}:{index:032x}")
}

/// Sixty vanilla neutral followers, each with an empty program: enough names for any deck.
/// One attack on a leader ends the match, which keeps these debug-build matches short.
fn export() -> (Value, String) {
    export_with(CARDS)
}

fn export_with(size: usize) -> (Value, String) {
    let snapshot = (0..size)
        .map(|index| {
            json!({"number":number(index),"faces":[{"name":number(index),
                "card_class":"ニュートラル","card_type":"フォロワー","cost":"1","power":"20",
                "hp":"1","traits":[],"text":null,"sections":[],"title":"synthetic"}]})
            .to_string()
        })
        .collect::<Vec<_>>()
        .join("\n");
    let mut rules: Value = serde_json::from_str(&fixture::rules(&snapshot, &number(0))).unwrap();
    *rules.pointer_mut("/input/kind").unwrap() = json!("resolved");
    *rules.pointer_mut("/titles/0/anchor").unwrap() =
        json!({"kind":"face-id","region":"jp","face_id":identifier("f", 0)});
    let mut references = fixture::references(&snapshot);
    references.wording = Some(
        serde_json::from_value(json!({
            "manifest_sha256": format!("sha256:{}", fixture::MANIFEST),
            "faces": (0..size).map(|index| json!({"card_no":number(index),"native_face":0_u8,
                "face_id":identifier("f", index),
                "revisions":[{"revision_id":format!("revision-{index}"),"fields":[]}]}))
                .collect::<Vec<_>>(),
        }))
        .unwrap(),
    );
    let programs = json!({
        "format_version":"4.2.0",
        "entries": (0..size).map(|index| json!({"id":number(index),"dsl_version":"1.0",
            "ast":{"card_id":identifier("c", index),"scope":"shared",
                "faces":{(identifier("f", index)):{"abilities":[]}}}})).collect::<Vec<_>>(),
        "printings": (0..size).map(|index| (identifier("c", index),
            json!({"representative":format!("test-{}", number(index)),"tokens":[]})))
            .collect::<serde_json::Map<_, _>>(),
    })
    .to_string();
    let projection = json!({
        "descriptor":{"key":"programs","path":"blobs/programs.json",
            "sha256":format!("sha256:{}", fixture::hex(programs.as_bytes()))},
        "engine_identity":{"Resolved":{"region":"jp","snapshot_sha256":references.snapshot_sha256,
            "faces":[{"card_no":number(0),"face_ordinal":0_u8,"face_id":identifier("f", 0),
                "rules_name_id":"synthetic-name","title_code":"synthetic_empty"}]}},
        "printings": (0..size).map(|index| json!({"printing_id":format!("test-{}", number(index)),
            "card_no":number(index),"card_id":identifier("c", index),
            "faces":[{"face_id":identifier("f", index),"revision_id":format!("revision-{index}"),
                "cost":1_u8,"rules_name":number(index)}]})).collect::<Vec<_>>(),
    });
    let reader = json!({"snapshot":snapshot,"rules":rules.to_string(),
        "references":references,"projection":projection});
    (reader, programs)
}

struct Folder(PathBuf);

impl Folder {
    fn new(name: &str) -> Self {
        let path = temp_dir().join(format!("sve-selfplay-{}-{name}", id()));
        create_dir_all(&path).unwrap();
        Self(path)
    }

    fn text(&self, name: &str) -> String {
        self.0.join(name).to_str().unwrap().to_owned()
    }
}

impl Drop for Folder {
    fn drop(&mut self) {
        drop(remove_dir_all(&self.0));
    }
}

/// The reader's output folder and the export root holding the program blob.
fn written(folder: &Folder) {
    let (reader, programs) = export();
    write_export(folder, &reader, &programs);
}

fn write_export(folder: &Folder, reader: &Value, programs: &str) {
    let text = |name: &str| reader.get(name).and_then(Value::as_str).unwrap().to_owned();
    let root = folder.0.join("reader");
    create_dir_all(&root).unwrap();
    write(root.join("snapshot.jsonl"), text("snapshot")).unwrap();
    write(root.join("rules.json"), text("rules")).unwrap();
    write(
        root.join("references.json"),
        reader["references"].to_string(),
    )
    .unwrap();
    write(
        root.join("program-projection.json"),
        reader["projection"].to_string(),
    )
    .unwrap();
    create_dir_all(folder.0.join("export/blobs")).unwrap();
    write(folder.0.join("export/blobs/programs.json"), programs).unwrap();
}

fn run(arguments: &[&str]) -> Result<String, String> {
    let mut log = Vec::new();
    let owned: Vec<String> = arguments.iter().map(|&text| text.to_owned()).collect();
    command(&owned, &mut log).map(|()| String::from_utf8(log).unwrap())
}

fn json_lines(path: &Path) -> Vec<Value> {
    read_to_string(path)
        .unwrap()
        .lines()
        .map(|line| serde_json::from_str(line).unwrap())
        .collect()
}

#[test]
fn seeds_run_from_a_composed_export_and_their_records_replay() {
    let folder = Folder::new("run");
    written(&folder);
    run(&[
        "prepare",
        &folder.text("reader"),
        &folder.text("export"),
        &folder.text("inputs"),
    ])
    .unwrap();
    let (inputs, programs) = (
        folder.text("inputs/inputs.json"),
        folder.text("inputs/programs.json"),
    );
    let log = run(&[
        "run",
        &inputs,
        &programs,
        &folder.text("out"),
        "7",
        "3",
        "2",
        "2",
    ])
    .unwrap();
    assert!(log.contains("60 eligible printings"), "{log}");
    assert!(log.contains("3/3 matches"), "{log}");

    let summary: Value =
        serde_json::from_str(&read_to_string(folder.0.join("out/summary.json")).unwrap()).unwrap();
    assert_eq!(summary["games"], 3);
    assert_eq!(summary["first_seed"], 7);
    assert_eq!(summary["failures"], json!([]), "{summary}");
    // Vanilla followers can only be played and attack: every match ends by the rules.
    let finished: u64 = ["p1", "p2", "draw"]
        .iter()
        .filter_map(|outcome| summary["outcomes"][outcome].as_u64())
        .sum();
    assert_eq!(finished, 3, "{summary}");
    assert_eq!(
        summary["step_engine_micros"]["count"],
        summary["match_decisions"]["total"]
    );
    assert_eq!(
        json_lines(&folder.0.join("out/failures.jsonl")),
        Vec::<Value>::new()
    );

    let records = folder.text("out/records.jsonl");
    assert_eq!(json_lines(Path::new(&records)).len(), 2);
    let timings = folder.text("out/timings.jsonl");
    let replay_log = run(&["replay", &inputs, &programs, &records, &timings]).unwrap();
    assert!(
        replay_log.contains(r#"replayed: {"finished":2}"#),
        "{replay_log}"
    );
    let replayed = json_lines(Path::new(&timings));
    assert_eq!(replayed.len(), 2);
    for (line, record) in replayed.iter().zip(json_lines(Path::new(&records))) {
        assert_eq!(line["end"], "finished");
        assert_eq!(line["seed"], record["seed"]);
        assert_eq!(
            line["engine_micros"].as_array().unwrap().len(),
            record["steps"].as_array().unwrap().len()
        );
    }

    // A failure line wraps its record; one whose receipts no longer hold fails the command.
    let mut kept = json_lines(Path::new(&records));
    let wrapped = json!({"seed": kept[0]["seed"], "failure": {}, "record": kept[0]});
    kept[1]["steps"].as_array_mut().unwrap().remove(0);
    let mixed = folder.text("out/mixed.jsonl");
    write(&mixed, format!("{wrapped}\n{}\n", kept[1])).unwrap();
    let diverged = run(&["replay", &inputs, &programs, &mixed, &timings]).unwrap_err();
    assert_eq!(diverged, "1 records did not replay to their receipts");
    let ends: Vec<String> = json_lines(Path::new(&timings))
        .iter()
        .map(|line| line["end"].as_str().unwrap().to_owned())
        .collect();
    assert_eq!(ends[0], "finished");
    assert!(
        ends[1].starts_with("replay: step 0 got rejected"),
        "{ends:?}"
    );
}

/// A listed deck of `size` cards, one copy each from `first` on.
fn listed(label: &str, weight: u64, first: usize, size: usize) -> Value {
    let main: serde_json::Map<String, Value> = (first..first.saturating_add(size))
        .map(|index| (number(index), json!(1)))
        .collect();
    json!({"label": label, "weight": weight,
        "construction": {"kind": "title", "value": "synthetic_empty"}, "main": main})
}

#[test]
fn listed_decks_are_drawn_by_weight_and_their_cards_counted() {
    let folder = Folder::new("listed");
    written(&folder);
    run(&[
        "prepare",
        &folder.text("reader"),
        &folder.text("export"),
        &folder.text("inputs"),
    ])
    .unwrap();
    let mut unknown = listed("unknown", 5, 0, 40);
    unknown["main"]["t-999"] = json!(1);
    unknown["main"].as_object_mut().unwrap().remove("t-000");
    let mut wrong_section = listed("wrong section", 5, 0, 41);
    wrong_section["main"]
        .as_object_mut()
        .unwrap()
        .remove("t-040");
    wrong_section["evolve"] = json!({"t-040": 1});
    let list = json!({"decks": [
        listed("low", 1, 0, 40),
        listed("high", 3, 20, 40),
        listed("short", 5, 0, 39),
        unknown,
        listed("idle", 0, 0, 40),
        wrong_section,
    ]});
    write(folder.0.join("decks.json"), list.to_string()).unwrap();
    let log = run(&[
        "run",
        "--decks",
        &folder.text("decks.json"),
        &folder.text("inputs/inputs.json"),
        &folder.text("inputs/programs.json"),
        &folder.text("out"),
        "0",
        "8",
        "2",
    ])
    .unwrap();
    assert!(log.contains("2 listed decks, 4 refused"), "{log}");
    let refused: Value =
        serde_json::from_str(&read_to_string(folder.0.join("out/refused-decks.json")).unwrap())
            .unwrap();
    assert_eq!(refused[0]["label"], "short");
    assert_eq!(refused[0]["reason"], "main deck has 39 cards");
    assert_eq!(
        refused[1],
        json!({"label": "unknown", "reason": "admission: MissingSnapshot"})
    );
    assert_eq!(
        refused[2],
        json!({"label": "idle", "reason": "weight is zero"})
    );
    assert_eq!(
        refused[3],
        json!({"label": "wrong section", "reason": "t-040 is in the wrong deck section"})
    );

    let summary: Value =
        serde_json::from_str(&read_to_string(folder.0.join("out/summary.json")).unwrap()).unwrap();
    assert_eq!(summary["failures"], json!([]), "{summary}");
    let lineup = summary["lineup"].as_array().unwrap();
    let seats = |label: &str| {
        lineup
            .iter()
            .find(|deck| deck["label"] == label)
            .map_or(0, |deck| deck["seats"].as_u64().unwrap())
    };
    assert_eq!(seats("low").saturating_add(seats("high")), 16, "{summary}");
    assert!(seats("low") > 0 && seats("high") > 0, "{summary}");
    let mirrors: u64 = lineup
        .iter()
        .map(|deck| deck["mirrors"].as_u64().unwrap())
        .sum();
    assert!(mirrors > 0, "{summary}");
    for deck in lineup {
        let results: u64 = ["wins", "losses", "draws", "failures"]
            .iter()
            .map(|field| deck[field].as_u64().unwrap())
            .sum();
        assert_eq!(results, deck["seats"].as_u64().unwrap(), "{deck}");
    }

    let cards: Value =
        serde_json::from_str(&read_to_string(folder.0.join("out/cards.json")).unwrap()).unwrap();
    // Either deck contains t-020, including in a mirror.
    assert_eq!(cards["t-020"]["built"], 8);
    assert!(cards.get("t-060").is_none());
    let total = |field: &str| -> u64 {
        cards
            .as_object()
            .unwrap()
            .values()
            .map(|card| card[field].as_u64().unwrap())
            .sum()
    };
    // Vanilla followers are played onto the field; nothing evolves and no ability exists.
    assert!(total("played") > 0, "{cards}");
    assert_eq!(total("played"), total("entered"), "{cards}");
    for field in ["abilities", "triggers", "evolved", "failed"] {
        assert_eq!(total(field), 0, "{field}");
    }
}

#[test]
fn a_deck_list_that_plays_nothing_is_refused() {
    let folder = Folder::new("unlisted");
    written(&folder);
    run(&[
        "prepare",
        &folder.text("reader"),
        &folder.text("export"),
        &folder.text("inputs"),
    ])
    .unwrap();
    let inputs = [
        folder.text("inputs/inputs.json"),
        folder.text("inputs/programs.json"),
    ];
    let attempt = |list: &str| {
        write(folder.0.join("decks.json"), list).unwrap();
        run(&[
            "run",
            "--decks",
            &folder.text("decks.json"),
            &inputs[0],
            &inputs[1],
            &folder.text("out"),
            "0",
            "1",
        ])
        .unwrap_err()
    };
    let none = json!({"decks": [listed("short", 1, 0, 39)]}).to_string();
    assert_eq!(
        attempt(&none),
        "deck list: no deck can be played (1 refused)"
    );
    let refused: Value =
        serde_json::from_str(&read_to_string(folder.0.join("out/refused-decks.json")).unwrap())
            .unwrap();
    assert_eq!(
        refused,
        json!([{"label": "short", "reason": "main deck has 39 cards"}])
    );
    assert!(attempt("{}").starts_with("deck list: missing field"));
}

#[test]
fn a_legal_listed_deck_can_play_without_a_random_pool() {
    let folder = Folder::new("small-pool");
    let (reader, programs) = export_with(40);
    write_export(&folder, &reader, &programs);
    run(&[
        "prepare",
        &folder.text("reader"),
        &folder.text("export"),
        &folder.text("inputs"),
    ])
    .unwrap();
    let list = json!({"decks": [listed("small", 1, 0, 40)]});
    write(folder.0.join("decks.json"), list.to_string()).unwrap();
    run(&[
        "run",
        "--decks",
        &folder.text("decks.json"),
        &folder.text("inputs/inputs.json"),
        &folder.text("inputs/programs.json"),
        &folder.text("out"),
        "0",
        "1",
    ])
    .unwrap();
    let summary: Value =
        serde_json::from_str(&read_to_string(folder.0.join("out/summary.json")).unwrap()).unwrap();
    assert_eq!(summary["failures"], json!([]), "{summary}");
    assert_eq!(summary["lineup"][0]["mirrors"], 1);
}

#[test]
fn a_seed_names_one_match_and_a_changed_record_fails_its_replay() {
    let (reader, programs) = export();
    let inputs = json!({
        "snapshot": reader["snapshot"], "rules": reader["rules"],
        "keywords": r#"{"format":1,"kind":"keyword_registry","rule_names":{}}"#,
        "references": reader["references"],
        "printings": reader["projection"]["printings"],
        "engine_identity": reader["projection"]["engine_identity"],
        "identity": fixture::identity(&programs, reader["rules"].as_str().unwrap()),
    });
    let published = Published::new(&inputs.to_string(), &programs).unwrap();
    assert_eq!(published.excluded(), []);
    assert_eq!(published.unprogrammed(), 0);

    let limits = Limits::default();
    let first = play(published.arena(), 11, limits);
    let again = play(published.arena(), 11, limits);
    assert!(matches!(first.end, End::Finished { .. }), "{:?}", first.end);
    assert_eq!(first.record, again.record);
    assert_ne!(first.record, play(published.arena(), 12, limits).record);
    assert_eq!(first.costs.len(), first.record.steps.len());
    for deck in &first.record.setup.decks {
        assert!((40..=50).contains(&deck.main.len()), "{deck:?}");
    }

    let replayed = replay(published.arena(), &first.record, limits);
    assert_eq!(replayed.end, first.end);
    assert_eq!(replayed.record, first.record);

    let failure_of = |record: &Record| match replay(published.arena(), record, limits).end {
        End::Failed(failure) => (failure.kind, failure.detail),
        End::Finished { .. } => panic!("a changed record must not replay to a result"),
    };
    // Dropping the first answer makes every later command name a question that no longer exists.
    let mut changed = first.record.clone();
    changed.steps.remove(0);
    assert_eq!(failure_of(&changed).0, "replay");
    // A match that ends while the record still has commands diverged just as much.
    let mut longer = first.record.clone();
    longer.steps.push(longer.steps[0].clone());
    assert_eq!(
        failure_of(&longer),
        ("replay", "the match ends before the record does".into())
    );
    // A record that stops early is not a divergence: it replays as far as it goes.
    let mut shorter = first.record;
    shorter.steps.pop();
    assert_eq!(failure_of(&shorter).0, "record_end");

    let stopped = play(
        published.arena(),
        11,
        Limits {
            decisions: 5,
            ..limits
        },
    );
    let End::Failed(limit) = stopped.end else {
        panic!("five submissions cannot finish a match");
    };
    assert_eq!(limit.kind, "decision_limit");
    assert_eq!(stopped.record.steps.len(), 5);
    assert_eq!(limit.board.as_array().map(Vec::len), Some(2));
}

#[test]
fn wrong_usage_and_missing_inputs_are_reported() {
    assert!(run(&[]).unwrap_err().starts_with("usage:"));
    assert!(run(&["run", "only-one"]).unwrap_err().starts_with("usage:"));
    assert!(run(&["replay"]).unwrap_err().starts_with("usage:"));
    let folder = Folder::new("missing");
    let missing = run(&[
        "prepare",
        &folder.text("reader"),
        &folder.text("export"),
        &folder.text("inputs"),
    ])
    .unwrap_err();
    assert!(missing.contains("references.json"), "{missing}");
}
