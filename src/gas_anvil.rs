use std::sync::{Arc, Mutex};

use alloy_network::{Network, TransactionBuilder};
use alloy_primitives::{B256, U256};
use alloy_provider::{ext::{AnvilApi, TxPoolApi}, Provider, ProviderBuilder, WalletProvider};
use alloy_rpc_types::{TransactionRequest, TransactionTrait};
use alloy_transport::{RpcError, Transport, TransportResult};

#[derive(Clone)]
pub struct Gas1559Config {
    pub max_fee_per_gas: u128,
    pub max_priority_fee_per_gas: u128,
}

#[derive(Clone, Default)]
pub struct GasAnvil {
    config: Arc<Mutex<Option<Gas1559Config>>>,
}

#[allow(dead_code)]
impl GasAnvil {
    /// Creates a new GasAnvil instance.
    pub fn new() -> Self {
        Self {
            config: Arc::new(Mutex::new(None)),
        }
    }

    pub fn set_1559_config(&self, max_fee_per_gas: u128, max_priority_fee_per_gas: u128) {
        let mut config = self.config.lock().unwrap();
        *config = Some(Gas1559Config {
            max_fee_per_gas,
            max_priority_fee_per_gas,
        });
    }

    pub async fn mine<P, T, N>(&self, provider: &P, tx_hash: B256) -> TransportResult<()>
    where
        P: Provider<T, N> + AnvilApi<N, T> + WalletProvider,
        T: Transport + Clone + Send + Sync,
        N: Network,
    {
        let config = {
            let config_guard = self.config.lock().unwrap();
            config_guard.clone()
        };

        if let Some(cfg) = config {
            let tx = provider.get_transaction_by_hash(tx_hash).await.unwrap();
            let tx = tx.ok_or(RpcError::LocalUsageError(Box::new(std::io::Error::new(
                std::io::ErrorKind::NotFound,
                "Transaction not found",
            ))))?;
            
            if tx.max_fee_per_gas() >= cfg.max_fee_per_gas && 
                tx.max_priority_fee_per_gas().unwrap_or(0) >= cfg.max_priority_fee_per_gas {
                // Mine the block including this transaction
                // provider.evm_mine(None).await?;
                provider.anvil_mine(Some(U256::from(1)), None).await.unwrap();
                println!("Mined transaction {:?}", tx_hash);
            } else {
                // provider.raw_request::<(U256,), bool>("anvil_setBlockNumber".into(), (U256::from(1),)).await.unwrap();
                provider.anvil_mine(Some(U256::from(1)), None).await?;
                // let block = provider.get_block_by_number(BlockNumberOrTag::Latest, BlockTransactionsKind::Hashes)
                // .await?
                // .unwrap();
                // let current_timestamp = block.header.timestamp;
                // provider.anvil_set_next_block_timestamp(current_timestamp + 1).await?;
                // // Mine empty block to increase block number
                // provider.evm_mine(Some(MineOptions::Options {
                //     timestamp: None,
                //     blocks: Some(1),
                // })).await?;
                println!("Transaction {:?} does not meet the gas requirements.", tx_hash);
            }
           
        } else {
            println!("EIP-1559 config not set. Skipping mining.");
        }

        Ok(())
    }
}