use std::{
    borrow::Cow, collections::btree_map::Entry as BTreeMapEntry, sync::Arc,
};

use anyhow::{bail, Context as _, Result};

use chain_ops::{node, tx::ExecuteTemplate};
use contract::{ProtocolContracts, ProtocolDex};
use dex::providers::{astroport::Astroport, osmosis::Osmosis};
use environment::ReadFromVar as _;
use service::{
    supervisor::configuration,
    task::{application_defined, TimeBasedExpiration, TxPackage},
};

use crate::oracle::Oracle;

use super::{context, Task, TaskWithProvider};

#[derive(Debug, Clone, PartialEq, Eq, PartialOrd, Ord)]
pub struct Id {
    protocol: Arc<str>,
}

impl Id {
    #[must_use]
    pub const fn new(protocol: Arc<str>) -> Self {
        Self { protocol }
    }

    fn dex_node_grpc_var(mut network: String) -> Result<String> {
        const VAR_SUFFIX: &str = "__NODE_GRPC";

        if network.is_empty() {
            bail!("Protocol's network identifier is zero-length!");
        }

        let mut search_index = 1;

        while let Some(index) = network[search_index..]
            .find(|ch: char| ch.is_ascii_uppercase())
            .map(|index| search_index + index)
        {
            network.insert(index, '_');

            search_index = index + 2;
        }

        network = network.to_ascii_uppercase().replace('-', "_");

        network.reserve_exact(VAR_SUFFIX.len());

        network.push_str(VAR_SUFFIX);

        Ok(network)
    }
}

impl application_defined::Id for Id {
    type ServiceConfiguration = configuration::Service;

    type TaskCreationContext = context::ApplicationDefined;

    type Task = Task;

    #[inline]
    fn protocol(&self) -> Option<&Arc<str>> {
        Some(&self.protocol)
    }

    #[inline]
    fn name(&self) -> Cow<'static, str> {
        Cow::Owned(self.protocol.to_string())
    }

    async fn into_task<'r>(
        self,
        service_configuration: &'r mut Self::ServiceConfiguration,
        task_creation_context: &'r mut Self::TaskCreationContext,
        transaction_tx: &'r channel::unbounded::Sender<
            TxPackage<TimeBasedExpiration>,
        >,
    ) -> Result<Task> {
        let contract::Protocol { network, dex } = service_configuration
            .admin_contract()
            .clone()
            .protocol(&self.protocol)
            .await
            .with_context(|| {
                format!(
                    "Failed to query protocol's information! Protocol={}",
                    self.protocol
                )
            })?;

        let node_client = service_configuration.node_client().clone();

        let dex_node_client = {
            let entry = task_creation_context
                .dex_node_clients
                .entry(network.clone());

            match entry {
                BTreeMapEntry::Vacant(entry) => entry.insert(
                    node::Client::connect(
                        &Self::dex_node_grpc_var(network.clone())
                            .and_then(String::read_from_var)?,
                    )
                    .await?,
                ),
                BTreeMapEntry::Occupied(entry) => entry.into_mut(),
            }
            .clone()
        };

        task_creation_context
            .dex_node_clients
            .insert(network, dex_node_client.clone());

        Ok(match dex {
            ProtocolDex::Astroport {
                contracts: ProtocolContracts { oracle },
                router_address,
            } => {
                let oracle = oracle::Oracle::new(oracle).await?;

                Task::Astroport(TaskWithProvider {
                    protocol: self.protocol.clone(),
                    source: format!("Astroport; Protocol={}", self.protocol)
                        .into(),
                    node_client,
                    dex_node_client,
                    duration_before_start: task_creation_context
                        .duration_before_start,
                    execute_template: ExecuteTemplate::new(
                        service_configuration.signer().address().into(),
                        oracle.address().into(),
                    ),
                    idle_duration: service_configuration.idle_duration(),
                    timeout_duration: service_configuration.timeout_duration(),
                    hard_gas_limit: task_creation_context.gas_limit,
                    oracle: Oracle::new(
                        oracle,
                        task_creation_context.update_currencies_interval,
                    )
                    .await?,
                    provider: Astroport::new(router_address),
                    transaction_tx: transaction_tx.clone(),
                })
            },
            ProtocolDex::Osmosis {
                contracts: ProtocolContracts { oracle },
            } => {
                let oracle = oracle::Oracle::new(oracle).await?;

                Task::Osmosis(TaskWithProvider {
                    protocol: self.protocol.clone(),
                    source: format!("Osmosis; Protocol={}", self.protocol)
                        .into(),
                    node_client,
                    dex_node_client,
                    duration_before_start: task_creation_context
                        .duration_before_start,
                    execute_template: ExecuteTemplate::new(
                        service_configuration.signer().address().into(),
                        oracle.address().into(),
                    ),
                    idle_duration: service_configuration.idle_duration(),
                    timeout_duration: service_configuration.timeout_duration(),
                    hard_gas_limit: task_creation_context.gas_limit,
                    oracle: Oracle::new(
                        oracle,
                        task_creation_context.update_currencies_interval,
                    )
                    .await?,
                    provider: Osmosis::new(),
                    transaction_tx: transaction_tx.clone(),
                })
            },
        })

        // Oracle::new(
        //     node_client.clone().query_wasm(),
        //     oracle_address.clone(),
        //     task_creation_context.update_currencies_interval,
        // )
        // .await
        // .map(|oracle| Base {
        //     protocol: self.protocol.clone(),
        //     node_client,
        //     oracle,
        //     dex_node_client,
        //     source: format!(
        //         "{}; Protocol={}",
        //         Self::dex_name(&dex),
        //         self.protocol,
        //     )
        //     .into(),
        //     duration_before_start: task_creation_context.duration_before_start,
        //     execute_template: ExecuteTemplate::new(
        //         service_configuration.signer().address().into(),
        //         oracle_address,
        //     ),
        //     idle_duration: service_configuration.idle_duration(),
        //     timeout_duration: service_configuration.timeout_duration(),
        //     hard_gas_limit: task_creation_context.gas_limit,
        //     transaction_tx: transaction_tx.clone(),
        // })
        // .map(|base| Task {
        //     base,
        //     provider: Self::construct_provider(dex),
        // })
    }
}
