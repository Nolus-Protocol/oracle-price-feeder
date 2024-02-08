use tracing::{error, info, info_span};

use chain_comms::{
    decode,
    reexport::cosmrs::tendermint::{abci::types::ExecTxResult, Hash},
};

use crate::messages::DispatchResponse;

pub fn tx_response(
    contract_type: &str,
    contract_address: &str,
    hash: &Hash,
    tx_result: &ExecTxResult,
) -> Option<DispatchResponse> {
    info_span!("Tx Response")
        .in_scope(|| {
            info!("Contract type: {}", contract_type);

            info!("Contract: {}", contract_address);

            info!("Hash: {hash}");

            let mut maybe_dispatch_response = None;

            broadcast::log::on_error(tx_result.code, &tx_result.log);

            if tx_result.code.is_ok() {
                match decode::exec_tx_data(tx_result) {
                    Ok(dispatch_response) => {
                        match serde_json_wasm::from_slice::<DispatchResponse>(&dispatch_response) {
                            Ok(dispatch_response) => {
                                info!(
                                    "Dispatched {} alarms.",
                                    dispatch_response.dispatched_alarms()
                                );

                                maybe_dispatch_response = Some(dispatch_response);
                            }
                            Err(error) => error!(
                                error = ?error,
                                response_data = String::from_utf8_lossy(&tx_result.data).as_ref(),
                                "Failed to deserialize transaction response from the JSON format! Cause: {error}",
                            ),
                        }
                    }
                    Err(error) => error!(
                        error = ?error,
                        "Failed to decode transaction response from the Protobuf format! Cause: {error}",
                    ),
                }
            }

            info!("Gas limit for transacion: {}", tx_result.gas_wanted);
            info!("Gas used: {}", tx_result.gas_used);

            maybe_dispatch_response
        })
}
