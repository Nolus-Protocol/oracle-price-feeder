use cosmrs::{
    proto::{
        cosmos::{base::v1beta1::Coin, tx::v1beta1::TxRaw as RawTx},
        cosmwasm::wasm::v1::MsgExecuteContract,
    },
    tendermint::abci::{
        response::{CheckTx, DeliverTx},
        Code,
    },
    tx::{Body, Fee},
    Any as ProtobufAny,
};

use crate::signer::Signer;

use self::error::Result;

pub mod error;

#[derive(Clone)]
struct Msg {
    message: Vec<u8>,
    funds: Vec<Coin>,
}

#[derive(Clone)]
#[must_use]
pub struct ContractTx {
    contract: String,
    messages: Vec<Msg>,
}

impl ContractTx {
    pub const fn new(contract: String) -> Self {
        Self {
            contract,
            messages: Vec::new(),
        }
    }

    #[must_use]
    pub fn is_empty(&self) -> bool {
        self.messages.is_empty()
    }

    pub fn add_message(mut self, message: Vec<u8>, funds: Vec<Coin>) -> Self {
        self.messages.push(Msg { message, funds });

        self
    }

    pub fn serialize(self, signer: &Signer) -> Result<Vec<ProtobufAny>> {
        let buf = Vec::with_capacity(self.messages.len());

        self.messages
            .into_iter()
            .map(|msg| {
                ProtobufAny::from_msg(&MsgExecuteContract {
                    sender: signer.signer_address().into(),
                    contract: self.contract.clone(),
                    msg: msg.message,
                    funds: msg.funds,
                })
            })
            .try_fold(buf, |mut acc, msg| -> Result<Vec<ProtobufAny>> {
                acc.push(msg?);

                Ok(acc)
            })
    }

    pub fn commit(
        self,
        signer: &Signer,
        fee: Fee,
        memo: Option<&str>,
        timeout: Option<u32>,
    ) -> Result<RawTx> {
        self.serialize(signer).and_then(|messages| {
            signer
                .sign(
                    Body::new(
                        messages,
                        memo.unwrap_or_default(),
                        timeout.unwrap_or_default(),
                    ),
                    fee,
                )
                .map_err(Into::into)
        })
    }
}

pub trait TxResponse {
    fn code(&self) -> Code;

    fn log(&self) -> &str;

    fn gas_wanted(&self) -> i64;

    fn gas_used(&self) -> i64;
}

impl TxResponse for CheckTx {
    fn code(&self) -> Code {
        self.code
    }

    fn log(&self) -> &str {
        &self.log
    }

    fn gas_wanted(&self) -> i64 {
        self.gas_wanted
    }

    fn gas_used(&self) -> i64 {
        self.gas_used
    }
}

impl TxResponse for DeliverTx {
    fn code(&self) -> Code {
        self.code
    }

    fn log(&self) -> &str {
        &self.log
    }

    fn gas_wanted(&self) -> i64 {
        self.gas_wanted
    }

    fn gas_used(&self) -> i64 {
        self.gas_used
    }
}
