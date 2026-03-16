use lib::types::Block;
use lib::util::Savable;
use std::{env, fs::File, process::exit};

fn main() {
    let path = if let Some(arg) = env::args().nth(1) {
        arg
    } else {
        eprintln!("Usage: block_print <block_file>");
        exit(1)
    };

    if let Ok(file) = File::open(path) {
        let block = Block::load(file).expect("Failed to load block");
        println!("{:#?}", block);
    }
}
