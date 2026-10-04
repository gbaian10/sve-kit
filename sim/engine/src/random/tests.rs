use super::{Random, RandomAlgorithm};
use serde_json::json;

#[test]
fn numeric_legacy_state_keeps_its_wire_shape_and_words() {
    let mut random = Random::new("", RandomAlgorithm::Legacy);
    assert_eq!(
        serde_json::to_value(&random).unwrap(),
        0xcbf2_9ce4_8422_2325_u64
    );
    assert_eq!(random.next(), 14_087_677_454_934_409_008);
}

#[test]
fn versioned_state_restores_mid_buffer() {
    let mut original = Random::new("seed", RandomAlgorithm::ChaCha12V1);
    for _ in 0_u32..35 {
        original.next();
    }
    let encoded = serde_json::to_value(&original).unwrap();
    let mut restored: Random = serde_json::from_value(encoded).unwrap();
    for _ in 0_u32..100 {
        assert_eq!(restored.next(), original.next());
    }
}

#[test]
fn seed_expansion_is_exact_and_domain_separated() {
    let state = serde_json::to_value(Random::new("seed", RandomAlgorithm::ChaCha12V1)).unwrap();
    assert_eq!(state["algorithm"], "chacha12-sha256-rand09-v1");
    let expected_seed: [u8; 32] = [
        188, 137, 109, 6, 251, 50, 159, 159, 224, 37, 181, 94, 185, 223, 239, 1, 237, 113, 201,
        223, 196, 80, 191, 51, 84, 233, 209, 188, 6, 239, 114, 23,
    ];
    assert_eq!(state["state"]["seed"], json!(expected_seed));
    for different in ["seed ", "Seed", "種子", ""] {
        assert_ne!(
            state,
            serde_json::to_value(Random::new(different, RandomAlgorithm::ChaCha12V1)).unwrap()
        );
    }
}

#[test]
fn upstream_zero_seed_vector_and_full_width_cursor_survive_json() {
    let mut wire = serde_json::to_value(Random::new("zero", RandomAlgorithm::ChaCha12V1)).unwrap();
    wire["state"]["seed"] = json!([0_u8; 32].to_vec());
    let mut random: Random = serde_json::from_value(wire.clone()).unwrap();
    assert_eq!(random.next(), 0x53f9_5507_6a9a_f49b);
    wire["state"]["stream"] = json!(7_u64);
    wire["state"]["word_pos"] = json!([u64::MAX, 15_u64]);
    let mut last: Random = serde_json::from_value(wire.clone()).unwrap();
    assert_eq!(serde_json::to_value(&last).unwrap(), wire);
    last.next();
    let mut restored: Random =
        serde_json::from_str(&serde_json::to_string(&last).unwrap()).unwrap();
    for _ in 0_u32..100 {
        assert_eq!(last.next(), restored.next());
    }
    assert_eq!(
        serde_json::to_value(last).unwrap()["state"]["word_pos"],
        json!([201_u64, 0_u64])
    );
}

#[test]
fn unknown_or_malformed_versions_do_not_fall_back_to_legacy() {
    let valid = serde_json::to_value(Random::new("invalid", RandomAlgorithm::ChaCha12V1)).unwrap();
    let mut unknown = valid.clone();
    unknown["algorithm"] = json!("chacha12-sha256-rand09-v2");
    let mut missing = valid.clone();
    missing["state"].as_object_mut().unwrap().remove("word_pos");
    let mut bad_seed = valid.clone();
    bad_seed["state"]["seed"] = json!([0_u8; 31].to_vec());
    let mut extra = valid.clone();
    extra["unexpected"] = json!(true);
    let mut extra_state = valid.clone();
    extra_state["state"]["unexpected"] = json!(true);
    let mut bad_position = valid.clone();
    bad_position["state"]["word_pos"] = json!([-1_i64, 0_i64]);
    for bad in [
        unknown,
        missing,
        bad_seed,
        extra,
        extra_state,
        bad_position,
        json!(-1_i64),
        json!(1.5_f64),
        json!(null),
    ] {
        assert_eq!(
            serde_json::from_value::<Random>(bad)
                .unwrap_err()
                .to_string(),
            "data did not match any variant of untagged enum RandomWire"
        );
    }
    let mut overflow = valid;
    overflow["state"]["word_pos"] = json!([0_u64, 16_u64]);
    assert_eq!(
        serde_json::from_value::<Random>(overflow)
            .unwrap_err()
            .to_string(),
        "invalid: random word position exceeds the 68-bit ChaCha stream"
    );
}

#[test]
fn empty_operations_do_not_advance_and_ranges_remain_valid() {
    for algorithm in [RandomAlgorithm::Legacy, RandomAlgorithm::ChaCha12V1] {
        let mut random = Random::new("range", algorithm);
        let before = serde_json::to_value(&random).unwrap();
        assert_eq!(
            random.bounded(0).unwrap_err().to_string(),
            "invalid: random range must be nonempty"
        );
        let mut empty: [u8; 0] = [];
        random.shuffle(&mut empty);
        random.shuffle(&mut [0_u8]);
        assert_eq!(serde_json::to_value(&random).unwrap(), before);
        for bound in [1_u64, 6, 0x8000_0000_0000_0001, u64::MAX] {
            for _ in 0_u32..100 {
                assert!(random.bounded(bound).unwrap() < bound);
            }
        }
    }
}

#[test]
fn words_range_sampling_and_shuffle_have_fixed_version_vectors() {
    type Vector = (RandomAlgorithm, [u64; 4], [u64; 8], [u32; 8]);
    let cases: [Vector; 2] = [
        (
            RandomAlgorithm::Legacy,
            [
                12_181_685_342_030_963_830,
                18_173_694_064_748_430_173,
                4_852_065_545_794_103_970,
                10_924_606_611_213_250_296,
            ],
            [0, 5, 5, 5, 3, 3, 4, 1],
            [6, 1, 5, 3, 7, 0, 4, 2],
        ),
        (
            RandomAlgorithm::ChaCha12V1,
            [
                3_009_314_526_430_488_939,
                7_548_981_742_677_938_847,
                4_741_826_302_327_044_720,
                7_255_809_787_263_060_270,
            ],
            [5, 2, 0, 1, 5, 2, 0, 2],
            [6, 7, 4, 2, 1, 0, 5, 3],
        ),
    ];
    for (algorithm, words, rolls, shuffled) in cases {
        let mut random = Random::new("golden", algorithm);
        for word in words {
            assert_eq!(random.next(), word);
        }
        for roll in rolls {
            assert_eq!(random.bounded(6).unwrap(), roll);
        }
        let mut items = [0_u32, 1, 2, 3, 4, 5, 6, 7];
        random.shuffle(&mut items);
        assert_eq!(items, shuffled);
    }
}

#[test]
fn legacy_rejection_preserves_the_consumed_word_count() {
    let mut random: Random = serde_json::from_value(json!(0_u64)).unwrap();
    let bound = 0x8000_0000_0000_0001_u64;
    assert_eq!(random.bounded(bound).unwrap(), 7_070_836_379_803_831_726);
    assert_eq!(random.bounded(bound).unwrap(), 8_686_239_339_925_766_635);
    assert_eq!(
        serde_json::to_value(random).unwrap(),
        8_709_371_129_873_690_708_u64
    );
}
