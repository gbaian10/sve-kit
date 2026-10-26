//! Admission of complete current printed declarations, without executing their rules.
use super::{Ability, Game, KeywordId, Located, Result, invalid, unsupported};
use crate::ir::lowering::PrintedKeyword;
use alloc::collections::BTreeSet;
use serde_json::Value;

pub(super) fn check_coordinate(
    origins: &mut BTreeSet<(usize, u64, Option<KeywordId>)>,
    ordinal: usize,
    line: u64,
    keyword: Option<KeywordId>,
) -> Result<()> {
    if line == 0
        || !origins.insert((ordinal, line, keyword))
        || origins
            .iter()
            .any(|(other_face, other_line, other_keyword)| {
                *other_face == ordinal
                    && *other_line == line
                    && (keyword.is_none() != other_keyword.is_none())
            })
    {
        return Err(invalid("invalid or duplicate printed ability coordinate"));
    }
    Ok(())
}

pub(super) fn check(
    game: &Game,
    card: &str,
    located: &Located<Ability>,
    printed: &Value,
    keyword: KeywordId,
) -> Result<()> {
    if !matches!(
        printed["card_type"].as_str(),
        Some("フォロワー" | "フォロワー・エボルヴ" | "フォロワー・アドバンス")
    ) {
        return Err(unsupported("printed keyword requires a follower face"));
    }
    let current = game
        .card_runtime
        .current
        .get(card)
        .ok_or_else(|| unsupported("printed keyword requires current declaration proof"))?;
    let proof = current
        .candidate
        .declaration_at(&located.source.author_path)
        .ok_or_else(|| invalid("missing printed keyword reference"))?;
    let index = proof
        .keywords()
        .iter()
        .position(|item| {
            matches!(
                (item, keyword),
                (PrintedKeyword::Ward, KeywordId::Ward) | (PrintedKeyword::Aura, KeywordId::Aura)
            )
        })
        .ok_or_else(|| invalid("foreign printed keyword"))?;
    if located.source.author_path != format!("{}/keywords/{index}", proof.source.author_path) {
        return Err(invalid("printed keyword element source mismatch"));
    }
    Ok(())
}
