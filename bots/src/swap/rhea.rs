use std::sync::Arc;

use super::{QuoteOutput, Swap};
use crate::{
    near::{get_access_key_data, send_tx, view, RpcResult},
    Network,
};
use near_crypto::InMemorySigner;
use near_jsonrpc_client::JsonRpcClient;
use near_primitives::{
    transaction::{Transaction, TransactionV0},
    views::FinalExecutionStatus,
};
use near_sdk::{json_types::U128, near, serde_json, AccountId};
use templar_common::asset::{FromAsset, FungibleAsset, ToAsset};

#[derive(Debug, Clone)]
pub struct RheaSwap {
    pub contract: AccountId,
    pub client: JsonRpcClient,
    pub signer: Arc<InMemorySigner>,
}

impl RheaSwap {
    #[allow(
        clippy::unwrap_used,
        reason = "We know the contract IDs are valid NEAR account IDs."
    )]
    pub fn new(network: Network, client: JsonRpcClient, signer: Arc<InMemorySigner>) -> Self {
        Self {
            contract: match network {
                Network::Mainnet => "dclv2.ref-labs.near".parse().unwrap(),
                Network::Testnet => "dclv2.ref-dev.testnet".parse().unwrap(),
            },
            client,
            signer,
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq, PartialOrd, Ord)]
#[near(serializers = [json, borsh])]
struct QuoteRequest {
    pool_ids: Vec<String>,
    input_token: AccountId,
    output_token: AccountId,
    output_amount: U128,
    tag: String,
}

impl QuoteRequest {
    pub fn new(input_token: AccountId, output_token: AccountId, output_amount: U128) -> Self {
        Self {
            pool_ids: vec![format!("{}|{}|100", input_token, output_token)],
            tag: format!("{}|100|{}", input_token, output_amount.0),
            input_token,
            output_token,
            output_amount,
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq, PartialOrd, Ord)]
#[near(serializers = [json, borsh])]
pub struct QuoteResponse {
    amount: U128,
    tag: String,
}

impl QuoteOutput for QuoteResponse {
    fn to_u128(&self) -> U128 {
        self.amount
    }
}

#[derive(Debug, Clone, PartialEq, Eq, PartialOrd, Ord)]
#[near(serializers = [json, borsh])]
enum SwapRequestMsg {
    SwapByOutput {
        pool_ids: Vec<String>,
        output_token: AccountId,
        output_amount: U128,
        client_id: String,
    },
}

impl SwapRequestMsg {
    pub fn new(input_token: &AccountId, output_token: AccountId, output_amount: U128) -> Self {
        Self::SwapByOutput {
            pool_ids: vec![format!("{}|{}|100", input_token, output_token)],
            output_token,
            output_amount,
            client_id: format!("{}|100|{}", input_token, output_amount.0),
        }
    }
}

#[async_trait::async_trait]
#[allow(
    clippy::expect_used,
    reason = "Rhea was mostly implemented for testing purposes, and don't expect it to be used in production."
)]
impl Swap for RheaSwap {
    type QuoteOutput = QuoteResponse;
    type SwapOutput = FinalExecutionStatus;

    async fn quote(
        &self,
        from: FungibleAsset<FromAsset>,
        to: FungibleAsset<ToAsset>,
        amount: U128,
    ) -> RpcResult<Self::QuoteOutput> {
        let response: QuoteResponse = view(
            &self.client,
            self.contract.clone(),
            "quote_by_output",
            &QuoteRequest::new(
                from.into_nep141()
                    .expect("MT not yet supported on Rhea `from` assets"),
                to.into_nep141()
                    .expect("MT not yet supported on Rhea `to` assets"),
                amount,
            ),
        )
        .await?;
        Ok(response)
    }

    async fn swap(
        &self,
        from: FungibleAsset<FromAsset>,
        to: FungibleAsset<ToAsset>,
        amount: U128,
    ) -> RpcResult<Self::SwapOutput> {
        let msg = SwapRequestMsg::new(
            &from
                .clone()
                .into_nep141()
                .expect("MT not yet supported on Rhea `from` assets"),
            to.into_nep141()
                .expect("MT not yet supported on Rhea `to` assets"),
            amount,
        );

        let (nonce, block_hash) = get_access_key_data(&self.client, &self.signer).await?;

        let function_call =
            from.transfer_call_action(&self.contract, amount.into(), &serde_json::to_string(&msg)?);
        let tx = Transaction::V0(TransactionV0 {
            nonce,
            receiver_id: from.contract_id(),
            block_hash,
            signer_id: self.signer.account_id.clone(),
            public_key: self.signer.public_key().clone(),
            actions: vec![function_call.into()],
        });

        send_tx(&self.client, &self.signer, 10, tx).await
    }
}
