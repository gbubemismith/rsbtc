use std::{env, fs::File, process::exit};

use chrono::Utc;
use lib::{
    crypto::PrivateKey,
    sha256::Hash,
    types::{Block, BlockHeader, Transaction, TransactionOutput},
    util::{MerkleRoot, Savable},
};
use uuid::Uuid;

fn main() {
    let path = if let Some(arg) = env::args().nth(1) {
        arg
    } else {
        eprintln!("Usage: block_gen <block_file>");
        exit(1)
    };

    let private_key = PrivateKey::new_key();
    let transactions = vec![Transaction::new(
        vec![],
        vec![TransactionOutput {
            unique_id: Uuid::new_v4(),
            value: lib::INITIAL_REWARD * 10u64.pow(8),
            pubkey: private_key.public_key(),
        }],
    )];
    let merkle_root = MerkleRoot::calculate(&transactions);
    let block = Block::new(
        BlockHeader::new(Utc::now(), 0, Hash::zero(), merkle_root, lib::MIN_TARGET),
        transactions,
    );
    block.save_to_file(path).expect("Failed to save block");
}
