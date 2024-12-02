use std::{collections::HashMap, future::IntoFuture, sync::{Arc, Mutex}};
use alloy_network::{Network, TransactionBuilder};
use alloy_provider::{fillers::{FillerControlFlow, GasFillable, TxFiller}, utils::Eip1559Estimation, Provider, SendableTx};
use alloy_transport::{Transport, TransportResult};
use futures::FutureExt;
use derive_new::new; 


#[derive(Clone, Debug, new)]
pub struct LinearEscalator {
    start_bid: u128,             // Starting bid in wei
    increment: u128,             // Increment per block in wei
    max_bid: u128,               // Maximum bid in wei
    start_block: u64,           // Block number to start escalation
    valid_length: u64,          // Duration in blocks
    current_bid: Arc<Mutex<HashMap<String, u128>>>, // Tracks current bid per transaction
}

impl LinearEscalator {
    pub fn update_bid(&self, tx_id: &str, current_block: u64) -> u128 {
        let mut bids = self.current_bid.lock().unwrap();
        let bid = bids.entry(tx_id.to_string()).or_insert(self.start_bid);

        if current_block >= self.start_block + self.valid_length {
            // Transaction has expired
            *bid = 0;
        } else {
            *bid = std::cmp::min(*bid + self.increment, self.max_bid);
        }

        *bid
    }
}

#[derive(Clone, Debug, Default)]
pub struct GasEscalatorFiller {
    escalator: Option<LinearEscalator>,
}

impl GasEscalatorFiller {
    pub fn with_escalator(escalator: LinearEscalator) -> Self {
        Self {
            escalator: Some(escalator),
        }
    }

    // async fn find

    async fn prepare_1559<P, T, N>(
        &self,
        provider: &P,
        tx: &N::TransactionRequest,
    ) -> TransportResult<GasFillable>
    where
        P: Provider<T, N>,
        T: Transport + Clone,
        N: Network,
    {
        let gas_limit_fut = tx.gas_limit().map_or_else(
            || provider.estimate_gas(tx).into_future().right_future(),
            |gas_limit| async move { Ok(gas_limit) }.left_future(),
        );

        Ok(GasFillable::Eip1559 { gas_limit: gas_limit_fut.await?, estimate: Eip1559Estimation { max_fee_per_gas: 10_000_000_000, max_priority_fee_per_gas: 7_200_000_000 } })
    }
}

impl<N: Network> TxFiller<N> for GasEscalatorFiller {
    type Fillable = GasFillable;

    fn status(&self, tx: &<N as Network>::TransactionRequest) -> FillerControlFlow {
        FillerControlFlow::Ready
    }

    fn fill_sync(&self, _tx: &mut SendableTx<N>) {}

    async fn prepare<P, T>(
        &self,
        provider: &P,
        tx: &<N as Network>::TransactionRequest,
    ) -> TransportResult<Self::Fillable>
    where
        P: Provider<T, N>,
        T: Transport + Clone,
    {
        self.prepare_1559(provider, tx).await
    }

    async fn fill(
        &self,
        fillable: Self::Fillable,
        mut tx: SendableTx<N>,
    ) -> TransportResult<SendableTx<N>> {
        if let Some(builder) = tx.as_mut_builder() {
            match fillable {
                GasFillable::Legacy { gas_limit, gas_price } => {
                    builder.set_gas_limit(gas_limit);
                    builder.set_gas_price(gas_price);
                }
                GasFillable::Eip1559 { gas_limit, estimate } => {
                    builder.set_gas_limit(gas_limit);
                    builder.set_max_fee_per_gas(estimate.max_fee_per_gas);
                    builder.set_max_priority_fee_per_gas(estimate.max_priority_fee_per_gas);
                }
            }
        };
        Ok(tx)
    }
}

#[cfg(test)]
mod tests {
    use alloy::primitives::{address, U256};
    use alloy_provider::ProviderBuilder;
    use alloy_rpc_types::TransactionRequest;
    use alloy_network::TransactionBuilder;

    use super::*;

    #[tokio::test]  
    async fn test_gas_escalator_filler() {
        let filler = GasEscalatorFiller::default();
        let provider = ProviderBuilder::new().filler(filler).on_anvil_with_wallet();

        let vitalik = address!("d8dA6BF26964aF9D7eEd9e03E53415D37aA96045");
        let tx = TransactionRequest::default()
            .with_to(vitalik)
            .with_value(U256::from(125))
            // Notice that without the `NonceFiller`, you need to set `nonce` field.
            .with_nonce(0)
            // Notice that without the `ChainIdFiller`, you need to set the `chain_id` field.
            .with_chain_id(provider.get_chain_id().await.unwrap());


        // send transaction
        let tx = provider.send_transaction(tx).await.unwrap();
        let receipt = tx.get_receipt().await.unwrap();

        println!("Receipt: {:?}, {:?}", receipt.effective_gas_price, receipt.gas_used);
    }
}