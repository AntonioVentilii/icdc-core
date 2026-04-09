use candid::CandidType;
use serde::{Deserialize, Serialize};

use crate::{
    constants::{MAX_ICON_URL_LEN, MAX_REWARD_DESCRIPTION_LEN, MAX_REWARD_TITLE_LEN},
    types::{
        asset::{errors::AssetError, Asset},
        series::SeriesError,
    },
};

#[derive(
    CandidType, Serialize, Deserialize, Clone, Debug, PartialEq, Eq, PartialOrd, Ord, Hash,
)]
pub enum PayoutUnit {
    Fiat(FiatUnit),
    Asset(Asset),
    NonMonetary(NonMonetaryUnit),
}

impl PayoutUnit {
    #[must_use]
    pub fn as_id_bytes(&self) -> Vec<u8> {
        match self {
            PayoutUnit::Fiat(f) => format!("FIAT-{}", f.as_str()).into_bytes(),
            PayoutUnit::Asset(a) => format!("ASSET-{}", a.to_string().to_uppercase()).into_bytes(),
            PayoutUnit::NonMonetary(n) => match n {
                NonMonetaryUnit::Points => b"POINTS".to_vec(),
                NonMonetaryUnit::Social(s) => {
                    format!("SOCIAL-{}-{:?}-{:?}", s.title, s.description, s.icon_url).into_bytes()
                }
            },
        }
    }

    #[must_use]
    pub fn usd() -> Self {
        PayoutUnit::Fiat(FiatUnit::Usd)
    }

    /// Converts the economic unit to its canonical token rail.
    pub fn to_asset(&self) -> Result<Asset, AssetError> {
        match self {
            PayoutUnit::Asset(a) => Ok(a.clone()),
            _ => Err(AssetError::UnsupportedAsset),
        }
    }
}

#[derive(
    CandidType, Serialize, Deserialize, Clone, Debug, PartialEq, Eq, PartialOrd, Ord, Hash,
)]
pub enum FiatUnit {
    Usd,
    Eur,
    Gbp,
    Chf,
}

impl FiatUnit {
    #[must_use]
    pub fn as_str(&self) -> &'static str {
        match self {
            FiatUnit::Usd => "USD",
            FiatUnit::Eur => "EUR",
            FiatUnit::Gbp => "GBP",
            FiatUnit::Chf => "CHF",
        }
    }
}

#[derive(
    CandidType, Serialize, Deserialize, Clone, Debug, PartialEq, Eq, PartialOrd, Ord, Hash,
)]
pub struct SocialReward {
    /// A short title for the reward (e.g., "Pizza 🍕").
    pub title: String,
    /// An optional detailed description of the reward.
    pub description: Option<String>,
    /// An optional icon URL for the reward.
    pub icon_url: Option<String>,
}

impl SocialReward {
    /// Validates the lengths of the reward fields.
    pub fn validate(&self) -> Result<(), SeriesError> {
        if self.title.chars().count() > MAX_REWARD_TITLE_LEN {
            return Err(SeriesError::RewardTitleTooLong);
        }
        if let Some(desc) = &self.description {
            if desc.chars().count() > MAX_REWARD_DESCRIPTION_LEN {
                return Err(SeriesError::RewardDescriptionTooLong);
            }
        }
        if let Some(icon) = &self.icon_url {
            if icon.chars().count() > MAX_ICON_URL_LEN {
                return Err(SeriesError::RewardIconUrlTooLong);
            }
        }
        Ok(())
    }
}

#[derive(
    CandidType, Serialize, Deserialize, Clone, Debug, PartialEq, Eq, PartialOrd, Ord, Hash,
)]
pub enum NonMonetaryUnit {
    Points,
    Social(SocialReward),
}

impl NonMonetaryUnit {
    #[must_use]
    pub fn as_str(&self) -> &str {
        match self {
            NonMonetaryUnit::Points => "POINTS",
            NonMonetaryUnit::Social(s) => s.title.as_str(),
        }
    }
}
