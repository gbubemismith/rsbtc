use std::{env, fs::File, process::exit};

use lib::{types::Transaction, util::Savable};

fn main() {
    let path = if let Some(arg) = env::args().nth(1) {
        arg
    } else {
        eprintln!("Usage: tx_print <tx_file>");
        exit(1)
    };

    if let Ok(file) = File::open(path) {
        let block = Transaction::load(file).expect("Failed to load block");
        println!("{:#?}", block);
    }
}
