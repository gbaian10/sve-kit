//! Messages between the host and one seat's client.

use crate::common::{ContentRef, Format, RequestId, Seat};
use core::error::Error;
use core::fmt;
use schemars::JsonSchema;
use serde::de::{self, Deserializer};
use serde::{Deserialize, Serialize};

/// Card text region of the pinned snapshot.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "snake_case")]
pub enum Region {
    /// Japanese edition.
    Jp,
    /// English edition.
    En,
}

/// Exact content this game is pinned to.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct Identity {
    /// Public card snapshot.
    pub snapshot: ContentRef,
    /// Whole public program package, never a subset revealing a private deck.
    pub programs: ContentRef,
    /// Engine build.
    pub engine: ContentRef,
    /// Rules content.
    pub rules: ContentRef,
    /// Card text region.
    pub region: Region,
}

/// Delivered once per created game, before the first update.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct Ready {
    /// Wire format.
    pub format: Format,
    /// Seat bound to this channel.
    pub viewer: Seat,
    /// Content identity of the game.
    pub identity: Identity,
}

/// Host duty that failed.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "snake_case")]
pub enum HostErrorReason {
    /// A message is malformed or of another wire format.
    Format,
    /// A number is not an integer within the safe range.
    Range,
    /// A required board, question or presentation source cannot be projected safely.
    Projection,
    /// A resource budget was exceeded.
    Budget,
}

/// The host can no longer provide a safe interaction; the client stops interacting.
///
/// It carries no payload, secret, card text or stack trace, and is never a rules result.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct HostError {
    /// Wire format.
    pub format: Format,
    /// Failed duty.
    pub reason: HostErrorReason,
}

impl HostError {
    /// A resource budget was exceeded.
    pub const BUDGET: Self = Self::new(HostErrorReason::Budget);
    /// A message is malformed or of another wire format.
    pub const FORMAT: Self = Self::new(HostErrorReason::Format);
    /// A required projection is unavailable.
    pub const PROJECTION: Self = Self::new(HostErrorReason::Projection);
    /// A number is not an integer within the safe range.
    pub const RANGE: Self = Self::new(HostErrorReason::Range);

    /// An error for one failed duty.
    #[must_use]
    pub const fn new(reason: HostErrorReason) -> Self {
        Self {
            format: Format,
            reason,
        }
    }
}

impl fmt::Display for HostError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(match self.reason {
            HostErrorReason::Format => "host error: format",
            HostErrorReason::Range => "host error: range",
            HostErrorReason::Projection => "host error: projection",
            HostErrorReason::Budget => "host error: budget",
        })
    }
}

impl Error for HostError {}

/// Native outcome of one accepted or refused submission.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "snake_case")]
pub enum ReceiptResult {
    /// The input was accepted and the procedure advanced.
    Applied,
    /// The input was refused; the question, state and randomness are unchanged.
    Rejected,
    /// A validated play was illegal and returned to its origin.
    Reverted,
    /// The rules cancelled a pending occurrence.
    Cancelled,
    /// Rules execution faulted.
    EngineFault,
}

/// Why a submission was refused, without naming the private condition that failed.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "snake_case")]
pub enum Rejection {
    /// Wrong seat, retired question, unknown option or invalid answer.
    InvalidSubmission,
}

/// Outcome of one submission, sent before that seat's related update.
///
/// The rejection reason follows from the result, so the two can never disagree.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct Receipt {
    /// Label of the submission.
    request_id: RequestId,
    /// Native outcome.
    result: ReceiptResult,
    /// Present exactly when the result is a rejection.
    rejection: Option<Rejection>,
}

impl Receipt {
    /// The receipt of one submission with its native outcome.
    #[must_use]
    pub const fn new(request_id: RequestId, result: ReceiptResult) -> Self {
        let rejection = match result {
            ReceiptResult::Rejected => Some(Rejection::InvalidSubmission),
            ReceiptResult::Applied
            | ReceiptResult::Reverted
            | ReceiptResult::Cancelled
            | ReceiptResult::EngineFault => None,
        };
        Self {
            request_id,
            result,
            rejection,
        }
    }

    /// Why the submission was refused; present exactly for a rejection.
    #[must_use]
    pub const fn rejection(&self) -> Option<Rejection> {
        self.rejection
    }

    /// Label of the submission.
    #[must_use]
    pub const fn request_id(&self) -> &RequestId {
        &self.request_id
    }

    /// Native outcome.
    #[must_use]
    pub const fn result(&self) -> ReceiptResult {
        self.result
    }
}

impl<'de> Deserialize<'de> for Receipt {
    fn deserialize<D>(deserializer: D) -> Result<Self, D::Error>
    where
        D: Deserializer<'de>,
    {
        let stated = ReceiptFields::deserialize(deserializer)?;
        let receipt = Self::new(stated.request_id, stated.result);
        if receipt.rejection == stated.rejection {
            Ok(receipt)
        } else {
            Err(de::Error::custom("rejection contradicts the result"))
        }
    }
}

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct ReceiptFields {
    request_id: RequestId,
    result: ReceiptResult,
    rejection: Option<Rejection>,
}

/// Everything the host sends to a client.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
#[serde(tag = "kind", rename_all = "snake_case")]
pub enum HostMessage {
    /// A game was created for this channel.
    #[schemars(title = "Ready")]
    Ready(Ready),
    /// Outcome of a submission.
    #[schemars(title = "Receipt")]
    Receipt(Receipt),
    /// The host stopped.
    #[schemars(title = "HostError")]
    HostError(HostError),
}
