use candid::CandidType;
use serde::Deserialize;
use std::cell::RefCell;

const SECONDS_PER_DAY: u64 = 86_400;
const DETERMINISTIC_RATE: u64 = 4_000_000_000;
const XRC_DECIMALS: u32 = 9;
const REQUIRED_SOURCE_COUNT: u64 = 4;

#[derive(Clone, Debug, PartialEq, Eq, CandidType, Deserialize)]
pub enum AssetClass {
    Cryptocurrency,
    FiatCurrency,
}

#[derive(Clone, Debug, PartialEq, Eq, CandidType, Deserialize)]
pub struct Asset {
    pub symbol: String,
    pub class: AssetClass,
}

#[derive(Clone, Debug, PartialEq, Eq, CandidType, Deserialize)]
pub struct GetExchangeRateRequest {
    pub base_asset: Asset,
    pub quote_asset: Asset,
    pub timestamp: Option<u64>,
}

#[derive(Clone, Debug, PartialEq, Eq, CandidType, Deserialize)]
pub struct ExchangeRateMetadata {
    pub decimals: u32,
    pub base_asset_num_received_rates: u64,
    pub base_asset_num_queried_sources: u64,
    pub quote_asset_num_received_rates: u64,
    pub quote_asset_num_queried_sources: u64,
    pub standard_deviation: u64,
    pub forex_timestamp: Option<u64>,
}

#[derive(Clone, Debug, PartialEq, Eq, CandidType, Deserialize)]
pub struct ExchangeRate {
    pub base_asset: Asset,
    pub quote_asset: Asset,
    pub timestamp: u64,
    pub rate: u64,
    pub metadata: ExchangeRateMetadata,
}

#[derive(Clone, Debug, PartialEq, Eq, CandidType, Deserialize)]
pub struct OtherError {
    pub code: u32,
    pub description: String,
}

#[derive(Clone, Debug, PartialEq, Eq, CandidType, Deserialize)]
pub enum ExchangeRateError {
    AnonymousPrincipalNotAllowed,
    Pending,
    CryptoBaseAssetNotFound,
    CryptoQuoteAssetNotFound,
    StablecoinRateNotFound,
    StablecoinRateTooFewRates,
    StablecoinRateZeroRate,
    ForexInvalidTimestamp,
    ForexBaseAssetNotFound,
    ForexQuoteAssetNotFound,
    ForexAssetsNotFound,
    RateLimited,
    NotEnoughCycles,
    InconsistentRatesReceived,
    Other(OtherError),
}

thread_local! {
    static OBSERVED_TIMESTAMPS: RefCell<Vec<u64>> = const { RefCell::new(Vec::new()) };
}

fn validate_request(request: &GetExchangeRateRequest) -> Result<u64, ExchangeRateError> {
    if request.base_asset
        != (Asset {
            symbol: "ICP".to_string(),
            class: AssetClass::Cryptocurrency,
        })
        || request.quote_asset
            != (Asset {
                symbol: "CXDR".to_string(),
                class: AssetClass::FiatCurrency,
            })
    {
        return Err(ExchangeRateError::Other(OtherError {
            code: 1,
            description: "expected the source-shaped ICP/CXDR request".to_string(),
        }));
    }
    let timestamp = request
        .timestamp
        .ok_or(ExchangeRateError::ForexInvalidTimestamp)?;
    if !timestamp.is_multiple_of(SECONDS_PER_DAY) {
        return Err(ExchangeRateError::ForexInvalidTimestamp);
    }
    Ok(timestamp)
}

#[cfg_attr(target_family = "wasm", ic_cdk::update)]
pub fn get_exchange_rate(
    request: GetExchangeRateRequest,
) -> Result<ExchangeRate, ExchangeRateError> {
    let timestamp = validate_request(&request)?;
    OBSERVED_TIMESTAMPS.with(|observed| observed.borrow_mut().push(timestamp));
    Ok(ExchangeRate {
        base_asset: request.base_asset,
        quote_asset: request.quote_asset,
        timestamp,
        rate: DETERMINISTIC_RATE,
        metadata: ExchangeRateMetadata {
            decimals: XRC_DECIMALS,
            base_asset_num_received_rates: REQUIRED_SOURCE_COUNT,
            base_asset_num_queried_sources: REQUIRED_SOURCE_COUNT,
            quote_asset_num_received_rates: REQUIRED_SOURCE_COUNT,
            quote_asset_num_queried_sources: REQUIRED_SOURCE_COUNT,
            standard_deviation: 0,
            forex_timestamp: Some(timestamp),
        },
    })
}

#[cfg_attr(target_family = "wasm", ic_cdk::query)]
pub fn observed_timestamps() -> Vec<u64> {
    OBSERVED_TIMESTAMPS.with(|observed| observed.borrow().clone())
}

#[cfg(test)]
mod tests {
    use super::*;

    fn request(timestamp: Option<u64>) -> GetExchangeRateRequest {
        GetExchangeRateRequest {
            base_asset: Asset {
                symbol: "ICP".to_string(),
                class: AssetClass::Cryptocurrency,
            },
            quote_asset: Asset {
                symbol: "CXDR".to_string(),
                class: AssetClass::FiatCurrency,
            },
            timestamp,
        }
    }

    #[test]
    fn source_shaped_request_is_exact_and_response_is_deterministic() {
        let first = get_exchange_rate(request(Some(10 * SECONDS_PER_DAY))).unwrap();
        let second = get_exchange_rate(request(Some(10 * SECONDS_PER_DAY))).unwrap();
        assert_eq!(first, second);
        assert_eq!(first.rate, DETERMINISTIC_RATE);
        assert_eq!(first.metadata.base_asset_num_received_rates, 4);
        assert_eq!(first.metadata.quote_asset_num_received_rates, 4);
    }

    #[test]
    fn malformed_pair_and_timestamp_are_rejected() {
        let mut wrong_pair = request(Some(SECONDS_PER_DAY));
        wrong_pair.quote_asset.symbol = "XDR".to_string();
        assert!(matches!(
            get_exchange_rate(wrong_pair),
            Err(ExchangeRateError::Other(_))
        ));
        assert_eq!(
            get_exchange_rate(request(None)),
            Err(ExchangeRateError::ForexInvalidTimestamp)
        );
        assert_eq!(
            get_exchange_rate(request(Some(SECONDS_PER_DAY + 1))),
            Err(ExchangeRateError::ForexInvalidTimestamp)
        );
    }
}
