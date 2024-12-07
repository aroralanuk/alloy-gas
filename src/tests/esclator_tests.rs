use alloy::primitives::{address, U256};
use alloy_provider::{ext::{AnvilApi,TxPoolApi}, Provider, ProviderBuilder, WalletProvider};
use alloy_rpc_types::TransactionRequest;
use alloy_network::TransactionBuilder;
use std::collections::HashMap;
use std::sync::{Arc, Mutex};

use crate::{GasEscalatorFiller, LinearEscalator};
use crate::gas_anvil::GasAnvil;

#[tokio::test]  
async fn test_gas_escalator_filler() {
    let filler = GasEscalatorFiller::default();
    let provider = ProviderBuilder::new().filler(filler).on_anvil_with_wallet();

    let vitalik = address!("d8dA6BF26964aF9D7eEd9e03E53415D37aA96045");
    let tx = TransactionRequest::default()
        .with_from(provider.default_signer_address())
        .with_to(vitalik)
        .with_value(U256::from(125))
        .with_nonce(0)
        .with_chain_id(provider.get_chain_id().await.unwrap());

    let tx = provider.send_transaction(tx).await.unwrap();
    let receipt = tx.get_receipt().await.unwrap();

    println!("Receipt: {:?}, {:?}", receipt.effective_gas_price, receipt.gas_used);
}

#[tokio::test]
async fn test_underpriced_stuck_in_txpool() {
    let gas_anvil = GasAnvil::new();
    gas_anvil.set_1559_config(2_500_000_000, 1_150_000_000);

    let filler = GasEscalatorFiller::with_escalator(LinearEscalator {
        start_bid: 1_000_000_000,
        increment: 100_000_000,
        max_bid: 10_000_000_000,
        start_block: 0,
        valid_length: 10,
        current_bid: Arc::new(Mutex::new(HashMap::new())),
    });
    let provider = ProviderBuilder::new().filler(filler).on_anvil_with_wallet();
    provider.anvil_set_auto_mine(false).await.unwrap();

    let sender = provider.default_signer_address();
    let receiver = address!("d8dA6BF26964aF9D7eEd9e03E53415D37aA96045");

    let initial_balance = U256::from(1e18 as u64);
    provider.anvil_set_balance(sender, initial_balance).await.unwrap();

    let tx1 = TransactionRequest::default()
        .from(sender)
        .with_to(receiver)
        .with_value(U256::from(125))
        .with_nonce(0)
        .with_chain_id(provider.get_chain_id().await.unwrap());

    let pending_tx1 = provider.send_transaction(tx1.clone()).await.unwrap();
    let tx_hash1 = *pending_tx1.tx_hash();

    // "underpriced"
    gas_anvil.mine(&provider, tx_hash1).await.unwrap();

    let receipt1 = provider.get_transaction_receipt(tx_hash1).await.unwrap();
    assert!(
        receipt1.is_none(),
        "tx1 should be pending and not yet mined - unmet max fee per gas"
    );

    let pending_tx2 = provider.send_transaction(tx1.clone()).await.unwrap();
    let tx2_hash = *pending_tx2.tx_hash();

    gas_anvil.mine(&provider, tx2_hash).await.unwrap();
    let receipt2 = provider.get_transaction_receipt(tx2_hash).await.unwrap();
    assert!(receipt2.is_none(), "tx2 should be pending and not yet mined - unmet max priority fee per gas");

    let pool_content = provider.txpool_content().await.unwrap();
    println!("Mempool after tx2: {:?}", pool_content);
    // provider.anvil_mine_one().await.unwrap();
    let tx2 = tx1.clone();

    let pending_tx3 = provider.send_transaction(tx2.clone()).await.unwrap();
    // let tx3_hash = *pending_tx3.tx_hash();

    // gas_anvil.mine(&provider, tx3_hash).await.unwrap();
    // let receipt3 = provider.get_transaction_receipt(tx3_hash).await.unwrap();
    // assert!(receipt3.is_some(), "tx3 should be mined");

    
}