use std::num::NonZeroU32;

use cosmrs::{
    proto::{
        cosmos::{
            auth::v1beta1::{
                query_client::QueryClient as AuthQueryClient, BaseAccount, QueryAccountRequest,
            },
            base::abci::v1beta1::GasInfo,
            tx::v1beta1::{service_client::ServiceClient, SimulateRequest},
        },
        cosmwasm::wasm::v1::{
            query_client::QueryClient as WasmQueryClient, QuerySmartContractStateRequest,
        },
    },
    tendermint::abci::Code,
    tx::Fee,
    Coin,
};
use serde::de::DeserializeOwned;
use tracing::{debug, error};

use crate::{build_tx::ContractTx, client::Client, config::Node, signer::Signer};

pub mod error;

pub type CommitResponse = cosmrs::rpc::endpoint::broadcast::tx_commit::Response;

pub async fn query_account_data(
    client: &Client,
    address: &str,
) -> Result<BaseAccount, error::AccountQuery> {
    prost::Message::decode(
        {
            let data = client
                .with_grpc(move |rpc| async move {
                    AuthQueryClient::new(rpc)
                        .account(QueryAccountRequest {
                            address: address.into(),
                        })
                        .await
                })
                .await?
                .into_inner()
                .account
                .ok_or(error::AccountQuery::NoAccountData)?
                .value;

            debug!("gRPC query response from {address} returned successfully!");

            data
        }
        .as_slice(),
    )
    .map_err(Into::into)
}

pub async fn query_wasm<R>(
    client: &Client,
    address: &str,
    query: &[u8],
) -> Result<R, error::WasmQuery>
where
    R: DeserializeOwned,
{
    serde_json_wasm::from_slice::<R>(&{
        let data = client
            .with_grpc({
                let query_data = query.to_vec();

                move |rpc| async move {
                    WasmQueryClient::new(rpc)
                        .smart_contract_state(QuerySmartContractStateRequest {
                            address: address.into(),
                            query_data,
                        })
                        .await
                }
            })
            .await?
            .into_inner()
            .data;

        debug!(
            data = %String::from_utf8_lossy(&data),
            "gRPC query response from {address} returned successfully!",
        );

        data
    })
    .map_err(Into::into)
}

pub async fn simulate_tx(
    signer: &mut Signer,
    client: &Client,
    config: &Node,
    gas_limit: u64,
    unsigned_tx: ContractTx,
) -> Result<GasInfo, error::SimulateTx> {
    let simulation_tx = unsigned_tx
        .commit(signer, calculate_fee(config, gas_limit)?, None, None)?
        .to_bytes()?;

    let gas_info: GasInfo = client
        .with_grpc(move |channel| async move {
            ServiceClient::new(channel)
                .simulate(SimulateRequest {
                    tx_bytes: simulation_tx,
                    ..Default::default()
                })
                .await
        })
        .await?
        .into_inner()
        .gas_info
        .ok_or(error::SimulateTx::MissingSimulationGasInto)?;

    if gas_limit < gas_info.gas_used {
        return Err(error::SimulateTx::SimulationGasExceedsLimit {
            used: gas_info.gas_used,
        });
    }

    Ok(gas_info)
}

pub async fn commit_tx(
    signer: &mut Signer,
    client: &Client,
    node_config: &Node,
    unsigned_tx: ContractTx,
    gas_limit: u64,
) -> Result<CommitResponse, error::CommitTx> {
    const ERROR_CODE: Code = Code::Err(if let Some(n) = NonZeroU32::new(13) {
        n
    } else {
        panic!()
    });

    let signed_tx =
        unsigned_tx.commit(signer, calculate_fee(node_config, gas_limit)?, None, None)?;

    let tx_commit_response = client
        .with_json_rpc(|rpc| async move { signed_tx.broadcast_commit(&rpc).await })
        .await?;

    if !(tx_commit_response.deliver_tx.code == ERROR_CODE
        && tx_commit_response.deliver_tx.gas_used == 0
        && tx_commit_response.deliver_tx.gas_wanted == 0)
    {
        signer.tx_confirmed();
    }

    Ok(tx_commit_response)
}

pub async fn commit_tx_with_gas_estimation(
    signer: &mut Signer,
    client: &Client,
    node_config: &Node,
    gas_limit: u64,
    unsigned_tx: ContractTx,
    fallback_gas_limit: u64,
) -> Result<CommitResponse, error::GasEstimatingTxCommit> {
    let tx_gas_limit: u64 = match simulate_tx(
        signer,
        client,
        node_config,
        gas_limit,
        unsigned_tx.clone(),
    )
    .await
    {
        Ok(gas_info) => gas_info.gas_used,
        Err(error) => {
            error!(
                error = %error,
                "Failed to simulate transaction! Falling back to provided gas limit. Fallback gas limit: {gas_limit}.",
                gas_limit = fallback_gas_limit
            );

            fallback_gas_limit
        }
    };

    let adjusted_gas_limit: u64 = u128::from(tx_gas_limit)
        .checked_mul(node_config.gas_adjustment_numerator().get().into())
        .and_then(|result| {
            result.checked_div(node_config.gas_adjustment_denominator().get().into())
        })
        .map(|result| u64::try_from(result).unwrap_or(u64::MAX))
        .unwrap_or(tx_gas_limit);

    commit_tx(signer, client, node_config, unsigned_tx, adjusted_gas_limit)
        .await
        .map_err(Into::into)
}

fn calculate_fee(config: &Node, gas_limit: u64) -> Result<Fee, error::FeeCalculation> {
    Ok(Fee::from_amount_and_gas(
        Coin::new(
            u128::from(gas_limit)
                .saturating_mul(config.gas_price_numerator().get().into())
                .saturating_div(config.gas_price_denominator().get().into()),
            config.fee_denom(),
        )?,
        gas_limit,
    ))
}
