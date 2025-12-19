use std::time::SystemTime;

use serde::{Deserialize, Deserializer, Serialize, Serializer, de::Error, ser::*};
use serde_json;
use serde_json::Value;
use serde_with::{TimestampSeconds, serde_as};

#[derive(Debug, Clone)]
pub struct Bounced {
    pub games: Option<Vec<String>>,
    pub slots: Option<Vec<i64>>,
    pub tags: Vec<String>,
    pub data: BounceData,
}

/// An internal representation of the [Bounced] struct, used as an intermediate
/// state to determine how to decode the [BounceData].
#[derive(Debug, Clone, Deserialize, Serialize)]
struct InternalBounced {
    pub games: Option<Vec<String>>,
    pub slots: Option<Vec<i64>>,
    #[serde(default)]
    pub tags: Vec<String>,
    pub data: Value,
}

// Deserialize Bounced based on its tags.
impl<'de> Deserialize<'de> for Bounced {
    fn deserialize<D>(deserializer: D) -> Result<Bounced, D::Error>
    where
        D: Deserializer<'de>,
    {
        let internal = InternalBounced::deserialize(deserializer)?;
        if internal.tags.iter().any(|t| t == "DeathLink") {
            Ok(Bounced {
                games: internal.games,
                slots: internal.slots,
                tags: internal.tags,
                data: BounceData::DeathLink(match serde_json::from_value(internal.data) {
                    Ok(data) => data,
                    Err(err) => return Err(D::Error::custom(err)),
                }),
            })
        } else {
            Ok(Bounced {
                games: internal.games,
                slots: internal.slots,
                tags: internal.tags,
                data: BounceData::Generic(internal.data),
            })
        }
    }
}

#[derive(Debug, Clone)]
pub enum BounceData {
    DeathLink(DeathLink),
    Generic(Value),
}

#[serde_as]
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct DeathLink {
    #[serde_as(as = "TimestampSeconds<f64>")]
    pub time: SystemTime,
    pub cause: Option<String>,
    pub source: String,
}

#[derive(Debug, Clone)]
pub struct Bounce {
    pub games: Option<Vec<String>>,
    pub slots: Option<Vec<String>>,
    pub tags: Vec<String>,
    pub data: BounceData,
}

impl Serialize for Bounce {
    fn serialize<S>(&self, serializer: S) -> Result<S::Ok, S::Error>
    where
        S: Serializer,
    {
        let mut state = serializer.serialize_struct(
            "Bounce",
            2 + self.games.iter().count() + self.slots.iter().count(),
        )?;

        if let Some(games) = &self.games {
            state.serialize_field("games", games)?;
        }

        if let Some(slots) = &self.slots {
            state.serialize_field("slots", slots)?;
        }

        match &self.data {
            BounceData::DeathLink(death_link) => {
                let mut tags = self.tags.clone();
                if !tags.iter().any(|t| t == "DeathLink") {
                    tags.push("DeathLink".to_string());
                }

                state.serialize_field("tags", &tags)?;
                state.serialize_field("data", &death_link)?;
            }
            BounceData::Generic(value) => {
                if !self.tags.is_empty() {
                    state.serialize_field("tags", &self.tags)?;
                }
                state.serialize_field("data", &value)?;
            }
        }

        state.end()
    }
}
