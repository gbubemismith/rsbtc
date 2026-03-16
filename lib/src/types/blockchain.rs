use std::collections::{HashMap, HashSet};

use super::{Block, Transaction, TransactionOutput};
use crate::{
    error::{BtcError, Result},
    sha256::Hash,
    util::{MerkleRoot, Savable},
};

use bigdecimal::BigDecimal;
use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};
use std::io::{Error as IoError, ErrorKind as IoErrorKind, Read, Result as IoResult, Write};

use crate::U256;

#[derive(Serialize, Deserialize, Clone, Debug)]
pub struct Blockchain {
    utxos: HashMap<Hash, (bool, TransactionOutput)>,
    target: U256,
    blocks: Vec<Block>,
    #[serde(default, skip_serializing)]
    mempool: Vec<(DateTime<Utc>, Transaction)>,
}

impl Savable for Blockchain {
    fn load<I: Read>(reader: I) -> IoResult<Self> {
        ciborium::de::from_reader(reader).map_err(|_| {
            IoError::new(
                IoErrorKind::InvalidData,
                "Failed to deserialize Transaction",
            )
        })
    }

    fn save<O: Write>(&self, writer: O) -> IoResult<()> {
        ciborium::ser::into_writer(self, writer)
            .map_err(|_| IoError::new(IoErrorKind::InvalidData, "Failed to serialize Transaction"))
    }
}

impl Blockchain {
    pub fn new() -> Self {
        Self {
            utxos: HashMap::new(),
            target: crate::MIN_TARGET,
            blocks: vec![],
            mempool: vec![],
        }
    }

    pub fn utxos(&self) -> &HashMap<Hash, (bool, TransactionOutput)> {
        &self.utxos
    }

    pub fn target(&self) -> U256 {
        self.target
    }

    pub fn blocks(&self) -> impl Iterator<Item = &Block> {
        self.blocks.iter()
    }

    pub fn block_height(&self) -> u64 {
        self.blocks.len() as u64
    }

    /// Adds a new block to the blockchain.
    pub fn add_block(&mut self, block: Block) -> Result<()> {
        // check if the block is valid
        if self.blocks.is_empty() {
            // if this is the first block, it must have a zero hash
            if block.header.prev_block_hash != Hash::zero() {
                println!("zero hash");
                return Err(BtcError::InvalidBlock);
            }
        } else {
            // if this is not the first block, it must have a valid previous block hash
            let last_block = self.blocks.last().unwrap();

            if block.header.prev_block_hash != last_block.hash() {
                println!("prev hash is wrong");
                return Err(BtcError::InvalidBlock);
            }

            // check if the block's hash is less than the target
            if !block.header.hash().matches_target(block.header.target) {
                println!("does not match target");
                return Err(BtcError::InvalidBlock);
            }

            // check if the block's merkel root is correct
            let calculated_merkle_root = MerkleRoot::calculate(&block.transactions);
            if calculated_merkle_root != block.header.merkle_root {
                println!("invalid merkle root");
                return Err(BtcError::InvalidBlock);
            }

            // check if the block's timestamp is after the last block's timestamp
            if block.header.timestamp <= last_block.header.timestamp {
                println!("timestamp is not greater than previous block's timestamp");
                return Err(BtcError::InvalidBlock);
            }

            // Verify all transactions is the block
            block.verify_transactions(self.block_height(), &self.utxos)?;
        }

        let block_transactions: HashSet<_> =
            block.transactions.iter().map(|tx| tx.hash()).collect();

        self.mempool
            .retain(|tx| !block_transactions.contains(&tx.1.hash()));
        self.blocks.push(block);
        self.try_adjust_block();
        Ok(())
    }

    pub fn rebuild_utxos(&mut self) {
        for block in &self.blocks {
            for transaction in &block.transactions {
                for input in &transaction.inputs {
                    self.utxos.remove(&input.prev_tx_output_hash);
                }

                for output in transaction.outputs.iter() {
                    self.utxos
                        .insert(transaction.hash(), (false, output.clone()));
                }
            }
        }
    }

    pub fn try_adjust_block(&mut self) {
        if self.blocks.is_empty() {
            return;
        }

        if self.blocks.len() & crate::DIFFICULTY_UPDATE_INTERVAL as usize != 0 {
            return;
        }

        let start_time = self.blocks
            [self.blocks.len() - crate::DIFFICULTY_UPDATE_INTERVAL as usize]
            .header
            .timestamp;
        let end_time = self.blocks.last().unwrap().header.timestamp;

        let time_diff = end_time - start_time;
        let time_diff_seconds = time_diff.num_seconds();

        let target_seconds = crate::IDEAL_BLOCK_TIME * crate::DIFFICULTY_UPDATE_INTERVAL;
        let new_target = BigDecimal::parse_bytes(&self.target.to_string().as_bytes(), 10)
            .expect("BUG: impossible")
            * (BigDecimal::from(time_diff_seconds) / BigDecimal::from(target_seconds));

        let new_target_str = new_target
            .to_string()
            .split('.')
            .next()
            .expect("BUG: Expected a decimal point")
            .to_owned();
        let new_target: U256 = U256::from_str_radix(&new_target_str, 10).expect("BUG: impossible");

        let new_target = if new_target < self.target / 4 {
            self.target / 4
        } else if new_target > self.target * 4 {
            self.target * 4
        } else {
            new_target
        };

        self.target = new_target.min(crate::MIN_TARGET);
    }

    pub fn add_to_mempool(&mut self, transaction: Transaction) -> Result<()> {
        let mut known_inputs = HashSet::new();

        for input in &transaction.inputs {
            if !self.utxos.contains_key(&input.prev_tx_output_hash) {
                return Err(BtcError::InvalidTransaction);
            }

            if known_inputs.contains(&input.prev_tx_output_hash) {
                return Err(BtcError::InvalidTransaction);
            }

            known_inputs.insert(input.prev_tx_output_hash);
        }

        for input in &transaction.inputs {
            if let Some((true, _)) = self.utxos.get(&input.prev_tx_output_hash) {
                let referencing_transaction =
                    self.mempool
                        .iter()
                        .enumerate()
                        .find(|(_, (_, transaction))| {
                            transaction
                                .outputs
                                .iter()
                                .any(|output| output.hash() == input.prev_tx_output_hash)
                        });

                // If we have found one, unmark all of its UTXOs
                if let Some((idx, (_, referencing_transaction))) = referencing_transaction {
                    for input in &referencing_transaction.inputs {
                        // set all utxos from this transaction to false
                        self.utxos
                            .entry(input.prev_tx_output_hash)
                            .and_modify(|(marked, _)| {
                                *marked = false;
                            });
                    }
                    // remove the transaction from the mempool
                    self.mempool.remove(idx);
                } else {
                    // if, somehow, there is no matching transaction,
                    // set this utxo to false
                    self.utxos
                        .entry(input.prev_tx_output_hash)
                        .and_modify(|(marked, _)| {
                            *marked = false;
                        });
                }
            }
        }
        let all_inputs = transaction
            .inputs
            .iter()
            .map(|input| {
                self.utxos
                    .get(&input.prev_tx_output_hash)
                    .expect("BUG: impossible")
                    .1
                    .value
            })
            .sum::<u64>();
        let all_outputs = transaction.outputs.iter().map(|output| output.value).sum();
        if all_inputs < all_outputs {
            return Err(BtcError::InvalidTransaction);
        }

        // Mark the UTXOs as used
        for input in &transaction.inputs {
            self.utxos
                .entry(input.prev_tx_output_hash)
                .and_modify(|(marked, _)| *marked = true);
        }

        self.mempool.push((Utc::now(), transaction));
        // sort by miner fee
        self.mempool.sort_by_key(|(_, transaction)| {
            let all_inputs = transaction
                .inputs
                .iter()
                .map(|input| {
                    self.utxos
                        .get(&input.prev_tx_output_hash)
                        .expect("BUG: impossible")
                        .1
                        .value
                })
                .sum::<u64>();
            let all_outputs: u64 = transaction.outputs.iter().map(|output| output.value).sum();
            let miner_fee = all_inputs - all_outputs;
            miner_fee
        });
        Ok(())
    }

    pub fn mempool(&self) -> &[(DateTime<Utc>, Transaction)] {
        &self.mempool
    }

    pub fn cleanup_mempool(&mut self) {
        let now = Utc::now();
        let mut utxo_hashes_to_unmark: Vec<Hash> = vec![];
        self.mempool.retain(|(timestamp, transaction)| {
            if now - *timestamp
                > chrono::Duration::seconds(crate::MAX_MEMPOOL_TRANSACTION_AGE as i64)
            {
                utxo_hashes_to_unmark.extend(
                    transaction
                        .inputs
                        .iter()
                        .map(|input| input.prev_tx_output_hash),
                );
                false
            } else {
                true
            }
        });

        // unmark all of the UTXOs
        for hash in utxo_hashes_to_unmark {
            self.utxos.entry(hash).and_modify(|(marked, _)| {
                *marked = false;
            });
        }
    }
}
