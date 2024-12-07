use core::time;
use std::sync::{Arc, Mutex};
use alloy_consensus::BlockHeader;

use alloy_network::{BlockResponse, Network, TransactionBuilder};
use alloy_primitives::{B256, U256};
use alloy_provider::{ext::{AnvilApi, TxPoolApi}, Provider, ProviderBuilder, WalletProvider};
use alloy_rpc_types::{BlockNumberOrTag, BlockTransactionsKind, TransactionRequest, TransactionTrait};
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

    pub async fn mine<P, T, N>(&self, provider: &P, tx_hash: B256, transaction_request: TransactionRequest) -> TransportResult<()>
    where
        P: Provider<T, N> + AnvilApi<N, T> + WalletProvider,
        T: Transport + Clone + Send + Sync,
        N: Network,
        <N as Network>::TransactionRequest: From<TransactionRequest>,
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
                provider.anvil_drop_transaction(tx_hash).await.unwrap();
                provider.anvil_mine(None, None).await.unwrap();

                // provider.
                let _tx = provider.eth_send_unsigned_transaction(transaction_request.into()).await.unwrap();

                println!("Transaction {:?} does not meet the gas requirements.", tx_hash);
            }
           
        } else {
            println!("EIP-1559 config not set. Skipping mining.");
        }

        Ok(())
    }
}