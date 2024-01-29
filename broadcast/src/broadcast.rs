use std::{collections::BTreeMap, time::Duration};

use tokio::{
    spawn,
    time::{sleep, Instant},
};

use chain_comms::{client::Client as NodeClient, interact::commit};

use crate::generators::{CommitError, CommitErrorType, CommitResultSender};
use crate::preprocess::TxRequest;
use crate::{impl_variant, log, ApiAndConfiguration};

pub(crate) struct BroadcastAndSendBackTxHash {
    pub(crate) broadcast_timestamp: Instant,
    pub(crate) channel_closed: Option<usize>,
}

#[inline]
pub(crate) async fn sleep_and_broadcast_tx<Impl: impl_variant::Impl>(
    api_and_configuration: &mut ApiAndConfiguration,
    between_tx_margin_time: Duration,
    tx_request: TxRequest<Impl>,
    tx_result_senders: &BTreeMap<usize, CommitResultSender>,
    last_signing_timestamp: Instant,
) -> Result<BroadcastAndSendBackTxHash, TxRequest<Impl>> {
    sleep_between_txs(between_tx_margin_time, last_signing_timestamp).await;

    broadcast_and_send_back_tx_hash::<Impl>(
        api_and_configuration,
        tx_result_senders,
        tx_request.sender_id,
        tx_request.signed_tx_bytes,
    )
    .await
    .map_err(|signed_tx_bytes| TxRequest {
        signed_tx_bytes,
        ..tx_request
    })
}

#[inline]
async fn sleep_between_txs(between_tx_margin_time: Duration, last_signing_timestamp: Instant) {
    let time_left_since_last_signing: Duration =
        between_tx_margin_time.saturating_sub(last_signing_timestamp.elapsed());

    if !time_left_since_last_signing.is_zero() {
        sleep(time_left_since_last_signing).await;
    }
}

enum SendBackTxHashResult {
    Ok,
    ChannelClosed,
}

#[inline]
async fn broadcast_and_send_back_tx_hash<Impl: impl_variant::Impl>(
    &mut ApiAndConfiguration {
        ref node_client,
        ref mut signer,
        tick_time,
        poll_time,
        ..
    }: &mut ApiAndConfiguration,
    tx_result_senders: &BTreeMap<usize, CommitResultSender>,
    sender_id: usize,
    signed_tx_bytes: Vec<u8>,
) -> Result<BroadcastAndSendBackTxHash, Vec<u8>> {
    let tx_response: commit::Response =
        Impl::broadcast_commit(node_client, signer, signed_tx_bytes).await?;

    let broadcast_timestamp: Instant = Instant::now();

    log::commit_response(&tx_response);

    let channel_closed: bool = matches!(
        send_back_tx_hash(
            node_client,
            tick_time,
            poll_time,
            tx_result_senders,
            sender_id,
            tx_response,
        ),
        SendBackTxHashResult::ChannelClosed
    );

    Ok(BroadcastAndSendBackTxHash {
        broadcast_timestamp,
        channel_closed: channel_closed.then_some(sender_id),
    })
}

#[inline]
fn send_back_tx_hash(
    node_client: &NodeClient,
    tick_time: Duration,
    poll_time: Duration,
    tx_result_senders: &BTreeMap<usize, CommitResultSender>,
    sender_id: usize,
    tx_response: commit::Response,
) -> SendBackTxHashResult {
    let hash = tx_response.hash;

    let channel_closed = if let Some(sender) = tx_result_senders.get(&sender_id) {
        if sender
            .send(if tx_response.code.is_ok() {
                Ok(tx_response.hash)
            } else {
                Err(CommitError {
                    r#type: if tx_response.code.value() == 32 {
                        CommitErrorType::InvalidAccountSequence
                    } else {
                        CommitErrorType::Unknown
                    },
                    tx_response,
                })
            })
            .is_ok()
        {
            return SendBackTxHashResult::Ok;
        }

        SendBackTxHashResult::ChannelClosed
    } else {
        SendBackTxHashResult::Ok
    };

    drop(spawn({
        let node_client = node_client.clone();

        async move {
            crate::poll_delivered_tx(&node_client, tick_time, poll_time, hash).await;
        }
    }));

    channel_closed
}
