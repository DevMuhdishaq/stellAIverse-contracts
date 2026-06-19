#![no_std]
pub mod types;

#[cfg(test)]
mod test;

use soroban_sdk::{contract, contractimpl, Env, Address, Symbol, Bytes, BytesN, vec, xdr::ToXdr};
use types::{OracleData, PricingRule, MarketplaceCircuitBreaker};

#[contract]
pub struct MarketplaceContract;

const DATA_EXPIRATION_WINDOW_SECONDS: u64 = 3600; 
const BPS_DENOMINATOR: u128 = 10_000;

#[contractimpl]
impl MarketplaceContract {
    
    pub fn authorize_oracle(env: Env, admin: Address, oracle: Address) {
        admin.require_auth();
        env.storage().instance().set(&Symbol::new(&env, "oracle"), &oracle);
    }

    pub fn set_circuit_breaker(env: Env, admin: Address, status: MarketplaceCircuitBreaker) {
        admin.require_auth();
        env.storage().instance().set(&Symbol::new(&env, "breaker"), &status);
    }

    pub fn verify_and_get_oracle_value(
        env: Env, 
        oracle_data: OracleData, 
        _signature: BytesN<64>
    ) -> u128 {
        let breaker: MarketplaceCircuitBreaker = env.storage().instance()
            .get(&Symbol::new(&env, "breaker"))
            .unwrap_or(MarketplaceCircuitBreaker::Active);
            
        if let MarketplaceCircuitBreaker::Terminated = breaker {
            panic!("Marketplace core operations are locked via circuit breaker");
        }

        let authorized_oracle: Address = env.storage().instance()
            .get(&Symbol::new(&env, "oracle"))
            .unwrap_or_else(|| panic!("Oracle reference not configured"));

        let mut check_payload = Bytes::new(&env);
        check_payload.append(&oracle_data.metric_id.to_xdr(&env));
        check_payload.append(&oracle_data.value.to_xdr(&env));
        check_payload.append(&oracle_data.timestamp.to_xdr(&env));

        // Fix: Cast explicitly into a generic Soroban Val mapping
        authorized_oracle.require_auth_for_args(vec![&env, check_payload.into()]);

        let current_ledger_time = env.ledger().timestamp();
        if current_ledger_time > oracle_data.timestamp + DATA_EXPIRATION_WINDOW_SECONDS {
            panic!("Oracle data attestation has expired");
        }

        oracle_data.value
    }

    pub fn calculate_dynamic_price(
        _env: Env, 
        rule: PricingRule, 
        verified_metric_value: u128
    ) -> u128 {
        if verified_metric_value == 0 {
            return rule.base_price;
        }

        let adjustment = verified_metric_value
            .checked_mul(rule.scale_factor_bps as u128)
            .unwrap_or(0) / BPS_DENOMINATOR;

        if rule.inverse {
            rule.base_price.checked_sub(adjustment).unwrap_or(0)
        } else {
            rule.base_price.checked_add(adjustment).unwrap_or(u128::MAX)
        }
    }
}
