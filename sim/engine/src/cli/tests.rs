use clap::{CommandFactory as _, Parser as _, error::ErrorKind};

use super::{Cli, Mode};

#[test]
fn positional_defaults_are_unchanged() {
    Cli::command().debug_assert();
    let args = Cli::try_parse_from(["sve-prototype", "synthetic.jsonl"]).unwrap();
    assert_eq!(args.snapshot.to_str(), Some("synthetic.jsonl"));
    assert_eq!(args.root.to_str(), Some("."));
    assert_eq!(args.mode, Mode::All);
    assert_eq!(args.output.to_str(), Some("target/prototype"));
    assert_eq!(args.selection_or_known, None);
}

#[test]
fn every_existing_mode_preserves_positional_paths() {
    for (name, mode) in [
        ("all", Mode::All),
        ("g1", Mode::G1),
        ("ai", Mode::Ai),
        ("replay", Mode::Replay),
        ("assist", Mode::Assist),
        ("validate", Mode::Validate),
        ("rules", Mode::Rules),
        ("gate", Mode::Gate),
    ] {
        let args = Cli::try_parse_from([
            "sve-prototype",
            "snapshot with spaces.jsonl",
            "project root",
            name,
            "reports with spaces",
            "selection or known.yaml",
        ])
        .unwrap();
        assert_eq!(args.snapshot.to_str(), Some("snapshot with spaces.jsonl"));
        assert_eq!(args.root.to_str(), Some("project root"));
        assert_eq!(args.mode, mode);
        assert_eq!(args.output.to_str(), Some("reports with spaces"));
        assert_eq!(
            args.selection_or_known
                .as_deref()
                .and_then(|path| path.to_str()),
            Some("selection or known.yaml")
        );
    }
}

#[test]
fn optional_paths_remain_optional() {
    let gate_args = Cli::try_parse_from(["sve-prototype", "snapshot", "root", "gate"]).unwrap();
    assert_eq!(gate_args.mode, Mode::Gate);
    assert_eq!(gate_args.output.to_str(), Some("target/prototype"));
    assert_eq!(gate_args.selection_or_known, None);
    let rules_args =
        Cli::try_parse_from(["sve-prototype", "snapshot", "root", "rules", "out"]).unwrap();
    assert_eq!(rules_args.mode, Mode::Rules);
    assert_eq!(rules_args.output.to_str(), Some("out"));
    assert_eq!(rules_args.selection_or_known, None);
}

#[test]
fn clap_rejects_missing_snapshot_unknown_mode_options_and_extra_values() {
    for (arguments, kind) in [
        (vec!["sve-prototype"], ErrorKind::MissingRequiredArgument),
        (
            vec!["sve-prototype", "snapshot", ".", "invalid"],
            ErrorKind::InvalidValue,
        ),
        (
            vec!["sve-prototype", "snapshot", "--unknown"],
            ErrorKind::UnknownArgument,
        ),
        (
            vec![
                "sve-prototype",
                "snapshot",
                ".",
                "rules",
                "out",
                "selection",
                "extra",
            ],
            ErrorKind::UnknownArgument,
        ),
    ] {
        let error = Cli::try_parse_from(arguments).unwrap_err();
        assert_eq!(error.kind(), kind);
    }
}

#[test]
fn help_and_version_do_not_need_a_snapshot() {
    for flag in ["--help", "-h"] {
        let error = Cli::try_parse_from(["sve-prototype", flag]).unwrap_err();
        assert_eq!(error.kind(), ErrorKind::DisplayHelp);
    }
    for flag in ["--version", "-V"] {
        let error = Cli::try_parse_from(["sve-prototype", flag]).unwrap_err();
        assert_eq!(error.kind(), ErrorKind::DisplayVersion);
    }
}

#[test]
fn end_of_options_allows_paths_starting_with_a_dash() {
    let args = Cli::try_parse_from([
        "sve-prototype",
        "--",
        "-snapshot.jsonl",
        "-root",
        "rules",
        "-reports",
        "-selection.yaml",
    ])
    .unwrap();
    assert_eq!(args.snapshot.to_str(), Some("-snapshot.jsonl"));
    assert_eq!(args.root.to_str(), Some("-root"));
    assert_eq!(args.mode, Mode::Rules);
    assert_eq!(args.output.to_str(), Some("-reports"));
    assert_eq!(
        args.selection_or_known
            .as_deref()
            .and_then(|path| path.to_str()),
        Some("-selection.yaml")
    );
}
