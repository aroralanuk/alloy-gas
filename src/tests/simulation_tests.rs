use std::sync::{Arc, Mutex};
use std::{fs::File, io::BufReader};
use std::collections::{HashMap, VecDeque};

use alloy_primitives::{address, utils, Address, U256};
use alloy_provider::fillers::{GasFiller, TxFiller};
use alloy_provider::{Provider, ProviderBuilder, SendableTx, WalletProvider};
use alloy_network::TransactionBuilder;
use alloy_rpc_types::{BlockNumberOrTag, TransactionRequest};
use serde::Deserialize;
use serde_json::Result;

use crate::{GasEscalatorFiller, LinearEscalator};

#[derive(Debug, Copy, Clone, Deserialize)]
struct BlockFeeData {
    block_number: u64,
    base_fee_per_gas: u128,
}

#[derive(Debug, Copy, Clone, Deserialize)]
struct TransactionData {
    block_number: u64,
    transaction_index: u64,
    from_address: Address,
    gas_limit: u64,
    gas_used: u64,
    gas_price: u128,
}

fn load_transaction_data(path: &str) -> Vec<TransactionData> {
    let file = File::open(path).expect("Failed to open transaction data file");
    let reader = BufReader::new(file);
    serde_json::from_reader(reader).expect("Failed to parse transaction data")
}

fn load_block_fee_data(path: &str) -> Vec<BlockFeeData> {
    let file = File::open(path).expect("Failed to open block fee data file");
    let reader = BufReader::new(file);
    serde_json::from_reader(reader).expect("Failed to parse block fee data")
}

#[tokio::test]
async fn simulate_gas_escalator() {
    // Load block data from JSON file
    let raw_transactions = load_transaction_data("data/transactions.json");
    let block_fee_data = load_block_fee_data("data/blocks.json");
    
    

    let mut blocks:HashMap<u64, (u128, Vec<TransactionData>)> = HashMap::new();
    for &block in &block_fee_data {
        blocks.entry(block.block_number).or_insert((block.base_fee_per_gas, Vec::new()));
    }
    println!("blocks: {:?}", blocks);


    for tx in raw_transactions {
        if let Some((base_fee, transactions)) = blocks.get_mut(&tx.block_number) {
            if tx.gas_price >= *base_fee {
                transactions.push(tx);
            }
        }
        continue;
        println!("block_number: {:?} {:?} {:?} {:?} {:?} {:?}", tx.block_number, tx.from_address, tx.transaction_index, tx.gas_limit, tx.gas_price, tx.gas_used);
    }

    let mut priority_fee_data: HashMap<u64, u128> = HashMap::new();
    let mut sorted_block_numbers: Vec<u64> = blocks.keys().cloned().collect();
    sorted_block_numbers.sort();

    for &block_number in &sorted_block_numbers {
        let block_info = &blocks.get(&block_number).unwrap();
        println!("block_info len: {:?} {:?}", block_info.1.len(), block_info.0);
        let mut tips: Vec<u128> = block_info.1.iter().filter_map(|tx| {
            if tx.gas_price >= block_info.0 {
                Some(tx.gas_price - block_info.0)
            } else {
                None
            }
        }).collect();

        if tips.is_empty() {
            tips.push(1_000_000_000u128); // Fallback if no valid tips are found
        }

        tips.sort_unstable();
        let index = usize::min(20 * tips.len() / 100, tips.len() - 1);
        let tip = tips[index];
        priority_fee_data.insert(block_number, tip);
    }
    println!("priority_fee_data: {:?}", priority_fee_data);

    let start_bid = 1_000_000_000u128;
    let increment = 1_000_000_000u128;
    let max_bid = 10_000_000_000u128;

    let simple_filler = GasFiller::default();
    let simple_provider = ProviderBuilder::new().filler(simple_filler).on_anvil_with_wallet();

    let escalator_filler = GasEscalatorFiller::with_escalator(LinearEscalator {
        start_bid: 5_000_000_000,
        increment: 5_000_000_000,
        max_bid: 50_000_000_000,
        start_block: 12349000,
        valid_length: 120,
        current_bid: Arc::new(Mutex::new(5_000_000_000)),
    });


    let mut block_numbers: Vec<u64> = blocks.keys().cloned().collect();
    block_numbers.sort();

    let mut pending_regular = VecDeque::new();
    // let mut pending_escalator = VecDeque::new();

    for &block_number in &block_numbers {
        let (base_fee, block_txs) = &blocks.get(&block_number).unwrap();
        if block_txs.len() == 0 {
            continue;
        }

        if pending_regular.is_empty()  {
            // let fee_history = simple_provider
            // .get_fee_history(
            //     10,
            //     BlockNumberOrTag::Latest,
            //     &[20.0],
            // )
            // .await.unwrap();

            
            let max_fee_per_gas = 2 * base_fee;
            let max_priority_fee_per_gas = max_fee_per_gas + priority_fee_data.get(&block_number).unwrap();
            println!("max_fee_per_gas: {:?} {:?} {:?}", max_fee_per_gas, max_priority_fee_per_gas, priority_fee_data.get(&block_number).unwrap());

            // Create the transaction request
            let tx_request = create_test_tx(&simple_provider, max_fee_per_gas, max_priority_fee_per_gas).await;
            let fillable = simple_filler.prepare(&simple_provider, &tx_request).await.unwrap();
            println!("fillable: {:?}", fillable);
            pending_regular.push_back((tx_request, block_number));
        }
        // if pending_escalator.is_empty() {
    }

    

        

}

async fn create_test_tx<P: Provider + WalletProvider>(provider: &P, max_fee_per_gas: u128, max_priority_fee_per_gas: u128) -> TransactionRequest {
    TransactionRequest::default()
        .with_from(provider.default_signer_address())
        .with_to(address!("d8dA6BF26964aF9D7eEd9e03E53415D37aA96045"))
        .with_value(U256::from(125))
        .with_max_fee_per_gas(max_fee_per_gas)
        .with_max_priority_fee_per_gas(max_priority_fee_per_gas)
        .with_nonce(0)
        .with_chain_id(provider.get_chain_id().await.unwrap())
}