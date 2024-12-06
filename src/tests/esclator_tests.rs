use alloy::primitives::{address, U256};
use alloy_provider::{ext::AnvilApi, Provider, ProviderBuilder, WalletProvider};
use alloy_rpc_types::TransactionRequest;
use alloy_network::TransactionBuilder;
use std::collections::HashMap;
use std::sync::{Arc, Mutex};

use crate::{GasEscalatorFiller, LinearEscalator};

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
    let filler = GasEscalatorFiller::with_escalator(LinearEscalator {
        start_bid: 1_000_000_000,
        increment: 100_000_000,
        max_bid: 10_000_000_000,
        start_block: 0,
        valid_length: 10,
        current_bid: Arc::new(Mutex::new(HashMap::new())),
    });
    let filler_clone = filler.clone();
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
    let expected_tx_hash = *pending_tx1.tx_hash();

    let receipt1 = provider.get_transaction_receipt(expected_tx_hash).await.unwrap();
    assert!(
        receipt1.is_none(),
        "Transaction1 should be pending and not yet mined"
    );

    let actual_tx_hash = filler_clone.get_transaction(&provider, &tx1).await.unwrap().unwrap();
    assert_eq!(expected_tx_hash, actual_tx_hash);

    let _pending_tx2 = provider.send_transaction(tx1.clone()).await.unwrap();
}