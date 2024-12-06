use std::{collections::HashMap, future::IntoFuture, sync::{Arc, Mutex}};
use alloy_primitives::B256;
use alloy_rpc_types::TransactionTrait;
use alloy_network::{Network, TransactionBuilder, TransactionResponse};
use alloy_provider::{ext::TxPoolApi,  fillers::{FillerControlFlow, GasFillable, TxFiller}, utils::Eip1559Estimation, Provider, SendableTx};
use alloy_transport::{RpcError, Transport, TransportResult};
use futures::FutureExt;
use derive_new::new; 

mod gas_anvil;


#[derive(Clone, Debug, Default, new)]
pub struct LinearEscalator {
    start_bid: u128,             // Starting bid in wei
    increment: u128,             // Increment per block in wei
    max_bid: u128,               // Maximum bid in wei
    start_block: u64,           // Block number to start escalation
    valid_length: u64,          // Duration in blocks
    current_bid: Arc<Mutex<HashMap<B256, u128>>>, // Tracks current bid per transaction
}

impl LinearEscalator {
    pub fn update_bid(&self, tx_id: B256, current_block: u64) -> u128 {
        let mut bids = self.current_bid.lock().unwrap();
        let bid = bids.entry(tx_id).or_insert(self.start_bid);

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
    escalator: LinearEscalator,
}

impl GasEscalatorFiller {
    pub fn with_escalator(escalator: LinearEscalator) -> Self {
        Self {
            escalator,
        }
    }

    pub fn escalator(&self) -> &LinearEscalator {
        &self.escalator
    }

    // async fn find
    async fn get_transaction<P, T, N>(
        &self,
        provider: &P,
        tx: &N::TransactionRequest,
    ) -> TransportResult<Option<B256>>
    where
        P: Provider<T, N>,
        T: Transport + Clone,
        N: Network,
    {
        let from = tx.from().ok_or(RpcError::LocalUsageError(Box::new(std::io::Error::new(std::io::ErrorKind::InvalidInput, "TransactionRequest missing 'from' field"))))?;
        let nonce = tx.nonce().ok_or(RpcError::LocalUsageError(Box::new(std::io::Error::new(std::io::ErrorKind::InvalidInput, "TransactionRequest missing 'nonce' field"))))?;

        let txpool_content = provider.txpool_content().await?;

        for (sender, txs) in txpool_content.pending {
            if sender != from {
                continue;
            }

            for (_, pending_tx) in txs {
                if pending_tx.nonce() == nonce {
                    return Ok(Some(pending_tx.tx_hash()));
                }
            }
        }

        // check for queued


        Ok(None)
    }

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

        let eip1559_fees_fut = if let (Some(max_fee_per_gas), Some(max_priority_fee_per_gas)) =
            (tx.max_fee_per_gas(), tx.max_priority_fee_per_gas())
        {
            async move { Ok(Eip1559Estimation { max_fee_per_gas, max_priority_fee_per_gas }) }
                .left_future()
        } else {
            provider.estimate_eip1559_fees(None).right_future()
        };


        let gas_limit_fut = tx.gas_limit().map_or_else(
            || provider.estimate_gas(tx).into_future().right_future(),
            |gas_limit| async move { Ok(gas_limit) }.left_future(),
        );

        let (gas_limit, default_estimate) = futures::try_join!(gas_limit_fut, eip1559_fees_fut)?;
        let base_fee = default_estimate.max_fee_per_gas - default_estimate.max_priority_fee_per_gas;
        // 10% increase minimum recommended by RPC providers
        let replacement_fee = (default_estimate.max_fee_per_gas * 110) / 100;
        let replacement_priority_fee = replacement_fee - base_fee;

        let estimate = if let Some(tx_hash) = self.get_transaction(provider, tx).await? {
            let current_block = provider.get_block_number().await?;
            let current_bid = {
                let bids = self.escalator.current_bid.lock().unwrap();
                bids.get(&tx_hash).copied()
            };

            let new_bid = if let Some(existing_bid) = current_bid {
                existing_bid  // Don't update if we already have a bid
            } else {
                self.escalator.update_bid(tx_hash, current_block)
            };
            
            let max_priority_fee_per_gas = std::cmp::max(new_bid, replacement_priority_fee);
            let max_fee_per_gas = base_fee + max_priority_fee_per_gas;

            println!(
                "Retrying transaction {} with increased bid: {} wei",
                tx_hash, new_bid
            );

            Eip1559Estimation {
                max_fee_per_gas,
                max_priority_fee_per_gas,
            }
        } else {
            default_estimate
        };

        println!("🚀 Gas Estimate: {:?}", estimate);

        Ok(GasFillable::Eip1559 { gas_limit, estimate })
    }
}

impl<N: Network> TxFiller<N> for GasEscalatorFiller {
    type Fillable = GasFillable;

    // from gas.rs
    fn status(&self, tx: &<N as Network>::TransactionRequest) -> FillerControlFlow {
        // legacy and eip2930 tx
        if tx.gas_price().is_some() && tx.gas_limit().is_some() {
            return FillerControlFlow::Finished;
        }

        // eip1559
        if tx.max_fee_per_gas().is_some()
            && tx.max_priority_fee_per_gas().is_some()
            && tx.gas_limit().is_some()
        {
            return FillerControlFlow::Finished;
        }

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
    mod esclator_tests;
}

