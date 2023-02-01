use std::time::Duration;

use cosmrs::{
    bip32::{Language, Mnemonic},
    crypto::secp256k1::SigningKey,
    proto::{
        cosmos::{
            base::abci::v1beta1::GasInfo,
            tx::v1beta1::{service_client::ServiceClient, SimulateRequest},
        },
        cosmwasm::wasm::v1::{
            query_client::QueryClient as WasmQueryClient, QuerySmartContractStateRequest,
        },
    },
    tendermint::Hash,
    tx::Fee,
};
use tokio::{
    io::{AsyncBufReadExt, BufReader as AsyncBufReader},
    time::sleep,
};
use tracing::{debug, error, info, info_span, Dispatch};

use alarms_dispatcher::{
    account::{account_data, account_id},
    client::Client,
    configuration::{read_config, Config, Node},
    messages::{DispatchResponse, ExecuteMsg, QueryMsg, StatusResponse},
    signer::Signer,
    tx::{ContractTx, TxResponse},
};

pub mod error;
pub mod log;

pub const DEFAULT_COSMOS_HD_PATH: &str = "m/44'/118'/0'/0/0";

pub const MAX_CONSEQUENT_ERRORS_COUNT: usize = 5;

#[tokio::main]
async fn main() -> Result<(), error::Application> {
    let log_writer = tracing_appender::rolling::hourly("./dispatcher-logs", "log");

    let (log_writer, _guard) =
        tracing_appender::non_blocking(log::CombinedWriter::new(std::io::stdout(), log_writer));

    setup_logging(log_writer)?;

    info!(concat!(
        "Running version built on: ",
        env!("BUILD_START_TIME_DATE", "No build time provided!")
    ));

    let result = dispatch_alarms(prepare_rpc().await?).await;

    if let Err(error) = &result {
        error!("{error}");
    }

    info!("Shutting down...");

    result.map_err(Into::into)
}

fn setup_logging<W>(writer: W) -> Result<(), tracing::dispatcher::SetGlobalDefaultError>
where
    W: for<'r> tracing_subscriber::fmt::MakeWriter<'r> + Send + Sync + 'static,
{
    tracing::dispatcher::set_global_default(Dispatch::new(
        tracing_subscriber::fmt()
            .with_level(true)
            .with_ansi(true)
            .with_file(false)
            .with_line_number(false)
            .with_writer(writer)
            .with_max_level({
                #[cfg(debug_assertions)]
                {
                    tracing::level_filters::LevelFilter::DEBUG
                }
                #[cfg(not(debug_assertions))]
                {
                    use std::{env::var_os, ffi::OsStr};

                    if var_os("ALARMS_DISPATCHER_DEBUG")
                        .map(|value| {
                            [OsStr::new("1"), OsStr::new("y"), OsStr::new("Y")]
                                .contains(&value.as_os_str())
                        })
                        .unwrap_or_default()
                    {
                        tracing::level_filters::LevelFilter::DEBUG
                    } else {
                        tracing::level_filters::LevelFilter::INFO
                    }
                }
            })
            .finish(),
    ))
}

pub async fn signing_key(
    derivation_path: &str,
    password: &str,
) -> Result<SigningKey, error::SigningKey> {
    use error::SigningKey as Error;

    println!("Enter dispatcher's account secret: ");

    let mut secret = String::new();

    // Returns number of read bytes, which is meaningless for current case.
    let _ = AsyncBufReader::new(tokio::io::stdin())
        .read_line(&mut secret)
        .await?;

    SigningKey::derive_from_path(
        Mnemonic::new(secret.trim(), Language::English)
            .map_err(Error::ParsingMnemonic)?
            .to_seed(password),
        &derivation_path
            .parse()
            .map_err(Error::ParsingDerivationPath)?,
    )
    .map_err(Error::DerivingKey)
}

pub struct RpcSetup {
    signer: Signer,
    config: Config,
    client: Client,
}

async fn prepare_rpc() -> Result<RpcSetup, error::RpcSetup> {
    let signing_key = signing_key(DEFAULT_COSMOS_HD_PATH, "").await?;

    info!("Successfully derived private key.");

    let config = read_config().await?;

    info!("Successfully read configuration file.");

    let client = Client::new(config.node()).await?;

    info!("Fetching account data from network...");

    let account_id = account_id(&signing_key, config.node())?;

    let account_data = account_data(account_id.clone(), &client).await?;

    info!("Successfully fetched account data from network.");

    Ok(RpcSetup {
        signer: Signer::new(
            account_id.to_string(),
            signing_key,
            config.node().chain_id().clone(),
            account_data,
        ),
        config,
        client,
    })
}

async fn dispatch_alarms(
    RpcSetup {
        mut signer,
        config,
        client,
    }: RpcSetup,
) -> Result<(), error::DispatchAlarms> {
    let poll_period = Duration::from_secs(config.poll_period_seconds());

    let query = serde_json_wasm::to_vec(&QueryMsg::AlarmsStatus {})?;

    loop {
        for (contract, type_name, to_error) in [
            (
                config.market_price_oracle(),
                "market price",
                error::DispatchAlarms::DispatchPriceAlarm
                    as fn(error::DispatchAlarm) -> error::DispatchAlarms,
            ),
            (
                config.time_alarms(),
                "time",
                error::DispatchAlarms::DispatchTimeAlarm
                    as fn(error::DispatchAlarm) -> error::DispatchAlarms,
            ),
        ] {
            dispatch_alarm(
                &mut signer,
                &client,
                config.node(),
                contract.address(),
                contract.max_alarms_group(),
                &query,
                type_name,
            )
            .await
            .map_err(to_error)?;
        }

        sleep(poll_period).await;
    }
}

async fn dispatch_alarm<'r>(
    signer: &'r mut Signer,
    client: &'r Client,
    config: &'r Node,
    address: &'r str,
    max_alarms: u32,
    query: &'r [u8],
    alarm_type: &'static str,
) -> Result<(), error::DispatchAlarm> {
    loop {
        let response: StatusResponse = query_status(client, address, query).await?;

        if response.remaining_for_dispatch() {
            let result = commit_tx(signer, client, config, address, max_alarms).await?;

            info!(
                "Dispatched {} {} alarms.",
                result.dispatched_alarms(),
                alarm_type
            );

            if result.dispatched_alarms() == max_alarms {
                continue;
            }
        }

        return Ok(());
    }
}

async fn query_status(
    client: &Client,
    address: &str,
    query: &[u8],
) -> Result<StatusResponse, error::StatusQuery> {
    serde_json_wasm::from_slice(&{
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
            "gRPC status response from {address} returned successfully!",
        );

        data
    })
    .map_err(Into::into)
}

async fn commit_tx(
    signer: &mut Signer,
    client: &Client,
    config: &Node,
    address: &str,
    max_count: u32,
) -> Result<DispatchResponse, error::TxCommit> {
    let unsigned_tx = ContractTx::new(address.into()).add_message(
        serde_json_wasm::to_vec(&ExecuteMsg::DispatchAlarms { max_count })?,
        Vec::new(),
    );

    let gas_info =
        simulation_gas_info(signer, client, config, max_count, unsigned_tx.clone()).await?;

    let signed_tx = unsigned_tx.commit(
        signer,
        Fee::from_amount_and_gas(
            config.fee().clone(),
            gas_info
                .gas_used
                .checked_mul(11)
                .and_then(|result| result.checked_div(10))
                .unwrap_or(gas_info.gas_used),
        ),
        None,
        None,
    )?;

    let tx_commit_response = client
        .with_json_rpc(|rpc| async move { signed_tx.broadcast_commit(&rpc).await })
        .await?;

    signer.tx_confirmed();

    let response = serde_json_wasm::from_slice(&tx_commit_response.deliver_tx.data)?;

    info_span!("Tx").in_scope(|| {
        log_commit_response(
            tx_commit_response.hash,
            &[
                ("Check", &tx_commit_response.check_tx as &dyn TxResponse),
                ("Deliver", &tx_commit_response.deliver_tx as &dyn TxResponse),
            ],
            &response
        )
    });

    Ok(response)
}

async fn simulation_gas_info(
    signer: &mut Signer,
    client: &Client,
    config: &Node,
    max_count: u32,
    unsigned_tx: ContractTx,
) -> Result<GasInfo, error::TxCommit> {
    let simulation_tx = unsigned_tx
        .commit(
            signer,
            Fee::from_amount_and_gas(
                config.fee().clone(),
                config
                    .gas_limit_per_alarm()
                    .saturating_mul(max_count.into()),
            ),
            None,
            None,
        )?
        .to_bytes()?;

    client
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
        .ok_or(error::TxCommit::MissingSimulationGasInto)
}

fn log_commit_response(hash: Hash, results: &[(&str, &dyn TxResponse)], dispatch_response: &DispatchResponse) {
    info!("Hash: {}", hash);

    info!("Dispatched {} alarms in total.", dispatch_response.dispatched_alarms());

    for &(tx_name, tx_result) in results {
        {
            let (code, log) = (tx_result.code(), tx_result.log());

            if code.is_ok() {
                debug!("[{}] Log: {}", tx_name, log);
            } else {
                error!(
                    log = %log,
                    "[{}] Error with code {} has occurred!",
                    tx_name,
                    code.value(),
                );
            }
        }

        {
            let (gas_wanted, gas_used) = (tx_result.gas_wanted(), tx_result.gas_used());

            if gas_wanted < gas_used {
                error!(
                    wanted = %gas_wanted,
                    used = %gas_used,
                    "[{}] Out of gas!",
                    tx_name,
                );
            } else {
                info!("[{}] Gas used: {}", tx_name, gas_used);
            }
        }
    }
}
