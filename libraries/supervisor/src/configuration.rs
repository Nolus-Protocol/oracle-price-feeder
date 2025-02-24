use std::time::Duration;

use anyhow::{Context as _, Error, Result};
use zeroize::Zeroizing;

use chain_ops::{
    key, node,
    signer::{GasAndFeeConfiguration, Signer},
};
use contract::{Address, Admin, CheckedContract, UncheckedContract};
use environment::ReadFromVar as _;

#[must_use]
pub struct Service {
    pub node_client: node::Client,
    pub signer: Signer,
    pub admin_contract: CheckedContract<Admin>,
    pub idle_duration: Duration,
    pub timeout_duration: Duration,
    pub balance_reporter_idle_duration: Duration,
    pub broadcast_delay_duration: Duration,
    pub broadcast_retry_delay_duration: Duration,
}

impl Service {
    pub async fn read_from_env() -> Result<Self> {
        let node_client = node::Client::connect(&Self::read_node_grpc_uri()?)
            .await
            .context("Failed to connect to node's gRPC!")?;

        let signer = Signer::new(
            node_client.clone(),
            Self::derive_signing_key()?,
            Self::read_fee_token_denominator()?,
            Self::read_gas_and_fee_configuration()?,
        )
        .await?;

        let (admin_contract, _) = UncheckedContract::admin(
            node_client.clone().query_wasm(),
            Address::new(Self::read_admin_contract_address()?),
        )
        .check()
        .await?;

        let idle_duration = Self::read_idle_duration()?;

        let timeout_duration = Self::read_timeout_duration()?;

        let balance_reporter_idle_duration =
            Self::read_balance_reporter_idle_duration()?;

        let broadcast_delay_duration = Self::read_broadcast_delay_duration()?;

        let broadcast_retry_delay_duration =
            Self::read_broadcast_retry_delay_duration()?;

        Ok(Self {
            node_client,
            signer,
            admin_contract,
            idle_duration,
            timeout_duration,
            balance_reporter_idle_duration,
            broadcast_delay_duration,
            broadcast_retry_delay_duration,
        })
    }

    pub const fn node_client(&self) -> &node::Client {
        &self.node_client
    }

    pub const fn signer(&self) -> &Signer {
        &self.signer
    }

    pub fn admin_contract(&self) -> &CheckedContract<Admin> {
        &self.admin_contract
    }

    #[must_use]
    pub fn idle_duration(&self) -> Duration {
        self.idle_duration
    }

    #[must_use]
    pub fn timeout_duration(&self) -> Duration {
        self.timeout_duration
    }

    #[must_use]
    pub fn balance_reporter_idle_duration(&self) -> Duration {
        self.balance_reporter_idle_duration
    }

    #[must_use]
    pub fn broadcast_delay_duration(&self) -> Duration {
        self.broadcast_delay_duration
    }

    #[must_use]
    pub fn broadcast_retry_delay_duration(&self) -> Duration {
        self.broadcast_retry_delay_duration
    }

    fn read_node_grpc_uri() -> Result<String> {
        String::read_from_var("NODE_GRPC_URI")
            .context("Failed to read node's gRPC URI!")
    }

    fn derive_signing_key() -> Result<key::Signing> {
        key::derive_from_mnemonic(&Self::read_signing_key_mnemonic()?, "")
            .context("Failed to derive signing key from mnemonic!")
    }

    fn read_signing_key_mnemonic() -> Result<Zeroizing<String>> {
        String::read_from_var("SIGNING_KEY_MNEMONIC")
            .context("Failed to read signing key's mnemonic!")
            .map(Zeroizing::new)
    }

    fn read_fee_token_denominator() -> Result<String> {
        String::read_from_var("FEE_TOKEN_DENOM")
            .context("Failed to read fee token's denominator!")
    }

    fn read_gas_and_fee_configuration() -> Result<GasAndFeeConfiguration> {
        GasAndFeeConfiguration::read_from_var("GAS_FEE_CONF")
            .context("Failed to read gas and fee configuration!")
    }

    fn read_admin_contract_address() -> Result<String> {
        String::read_from_var("ADMIN_CONTRACT_ADDRESS")
            .context("Failed to read admin contract's address")
    }

    fn read_idle_duration() -> Result<Duration> {
        u64::read_from_var("IDLE_DURATION_SECONDS")
            .map(Duration::from_secs)
            .context("Failed to read idle period duration!")
    }

    fn read_timeout_duration() -> Result<Duration> {
        u64::read_from_var("TIMEOUT_DURATION_SECONDS")
            .map(Duration::from_secs)
            .context("Failed to read timeout period duration!")
    }

    fn read_balance_reporter_idle_duration() -> Result<Duration, Error> {
        u64::read_from_var("BALANCE_REPORTER_IDLE_DURATION_SECONDS")
            .map(Duration::from_secs)
            .context("Failed to read between balance reporter idle delay period duration!")
    }

    fn read_broadcast_delay_duration() -> Result<Duration, Error> {
        u64::read_from_var("BROADCAST_DELAY_DURATION_SECONDS")
            .map(Duration::from_secs)
            .context("Failed to read between broadcast delay period duration!")
    }

    fn read_broadcast_retry_delay_duration() -> Result<Duration, Error> {
        u64::read_from_var("BROADCAST_RETRY_DELAY_DURATION_MILLISECONDS")
            .map(Duration::from_millis)
            .context("Failed to read between broadcast retries delay period duration!")
    }
}
