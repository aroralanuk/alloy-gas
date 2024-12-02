use alloy_gas::GasEscalatorFiller;
use alloy_provider::{Provider, ProviderBuilder};
use alloy_rpc_types::{request::TransactionRequest, TransactionTrait};
use alloy_network::TransactionBuilder;
use alloy::{
    // network::TransactionBuilder,
    primitives::{address, U256},
};
use eyre::Result;

#[tokio::main]
async fn main() -> Result<()> {
 // Spin up a local Anvil node.
    // Ensure `anvil` is available in $PATH.
    let provider = ProviderBuilder::new()
        // Add the `GasFiller` to the provider.
        // It is generally recommended to use the `.with_recommended_fillers()` method, which
        // includes the `GasFiller`.
        .with_gas_estimation()
        .filler(GasEscalatorFiller::default())
        .on_anvil_with_wallet();

    // Build an EIP-1559 type transaction to send 125 wei to Vitalik.
    let vitalik = address!("d8dA6BF26964aF9D7eEd9e03E53415D37aA96045");
    let tx = TransactionRequest::default()
        .with_to(vitalik)
        .with_value(U256::from(125))
        // Notice that without the `NonceFiller`, you need to set `nonce` field.
        .with_nonce(0)
        // Notice that without the `ChainIdFiller`, you need to set the `chain_id` field.
        .with_chain_id(provider.get_chain_id().await?);

    // Send the transaction, the nonce (0) is automatically managed by the provider.
    let builder = provider.send_transaction(tx.clone()).await?;
    let node_hash = *builder.tx_hash();
    let pending_tx =
        provider.get_transaction_by_hash(node_hash).await?.expect("Pending transaction not found");
    assert_eq!(pending_tx.nonce(), 0);

 
    println!("Transaction sent with nonce: {}", pending_tx.nonce());

    // check vitalik balance
    let balance = provider.get_balance(vitalik).await?;
    println!("Vitalik's balance: {}", balance);

    Ok(())
}
