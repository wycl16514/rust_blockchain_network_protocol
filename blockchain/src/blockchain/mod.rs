use sha2::{Digest, Sha256};
use std::slice::RSplit;
use std::time::SystemTime;
use transaction::*;
pub mod transaction;
use crate::wallet::{Transaction as WalletTransaction, Wallet};
use std::cmp::PartialEq;
use std::ops::AddAssign;
use std::ops::Index;
use std::time::Instant;
//fix bug
use serde::{Deserialize, Serialize};
use serde_json;

pub trait Serialization<T> {
    fn serialization(&self) -> Vec<u8>;
    //static
    fn deserialization(bytes: Vec<u8>) -> T;
}

pub enum BlockSearch {
    //tag value
    SearchByIndex(usize),
    SearchByPreviousHash(Vec<u8>),
    SearchByBlockHash(Vec<u8>),
    SearchByNonce(i32),
    SearchByTimeStamp(u128),
    SearchByTransaction(Vec<u8>),
}

pub enum BlockSearchResult<'a> {
    //indicate the block reference attaching to the tag value
    //has the same life time as the block on the chain
    Success(&'a Block),
    FailOfEmptyBlocks,
    FailOfIndex(usize),
    FailOfPreviousHash(Vec<u8>),
    FailOfBlockHash(Vec<u8>),
    FailOfNonce(i32),
    FailOfTimeStamp(u128),
    FailOfTransaction(Vec<u8>),
}

//fix bug here
#[derive(Debug, Clone, Deserialize, Serialize)]
pub struct Block {
    nonce: i32,
    previous_hash: Vec<u8>,
    time_stamp: u128,
    transactions: Vec<Vec<u8>>,
}

impl AddAssign<i32> for Block {
    fn add_assign(&mut self, rhs: i32) {
        self.nonce += rhs;
    }
}

/*
trait PartialEq<Rhs = Self> where Rhs : ?Sized {
    fn eq(&self, other :Rhs) -> bool;
    fn nq(&self, other :Rhs) -> bool {
        !self->eq(self, other)
    }
}
*/

impl PartialEq for Block {
    fn eq(&self, other: &Self) -> bool {
        let self_hash = self.hash();
        let other_hash = other.hash();
        self_hash == other_hash
    }
}
/*
CamelCase for name of struct, snake_case for name of fields
*/

impl Block {
    //methods for the struct, class methods,
    //Two kinds of methods, one kind static method which not reading or
    //writing into fields of the block
    //Self is alias name for object, if we change the name of the struct
    //then we don't need to change the name inside here
    pub fn new(nonce: i32, previous_hash: Vec<u8>) -> Self {
        //the method will take control of the input of previous_hash
        let time_now = SystemTime::now()
            .duration_since(SystemTime::UNIX_EPOCH)
            .unwrap();
        Block {
            nonce: nonce,
            previous_hash: previous_hash,
            time_stamp: time_now.as_nanos(),
            transactions: Vec::<Vec<u8>>::new(),
        } //don't add semicolon because we want to return this object
    }

    //struct method which is need to read or write to fields of the struct
    //self which reference to the struct instance
    pub fn print(&self) {
        //format value as hex
        println!("timestamp: {:x}", self.time_stamp);
        //format integer
        println!("nonce: {}", self.nonce);
        //print vecotr, ask the compiler to do it
        println!("previous_hash: {:?}", self.previous_hash);
        for (idx, tx) in self.transactions.iter().enumerate() {
            let transaction = Transaction::deserialization(tx.to_vec());
            println!("the {}'th transaction is: {}", idx, transaction);
        }
    }

    pub fn hash(&self) -> Vec<u8> {
        let mut bin = Vec::<u8>::new();
        bin.extend(self.nonce.to_be_bytes());
        bin.extend(self.previous_hash.clone());
        bin.extend(self.time_stamp.to_be_bytes());
        for tx in self.transactions.iter() {
            bin.extend(tx.clone());
        }

        let mut hasher = Sha256::new();
        hasher.update(bin);
        hasher.finalize().to_vec()
    }
}

#[derive(Debug, Serialize, Deserialize, Clone)]
pub struct BlockChain {
    pub transaction_pool: Vec<Vec<u8>>,
    pub chain: Vec<Block>,
    pub blockchain_address: String,
}
/*
trait Index<idx> {
    type Output: ?Sized
    fn index(&self, index: idx) -> &Self.Output;
}
*/

impl Index<usize> for BlockChain {
    type Output = Block; //u32
    fn index(&self, index: usize) -> &Self::Output {
        let res = self.chain.get(index);
        match res {
            Some(block) => {
                return block;
            }
            None => {
                panic!("index out of range for the chain")
            }
        }
    }
}

impl BlockChain {
    const DIFFICULTY: usize = 3;
    const MINING_SENDER: &str = "THE BLOCKCHAIN";
    const MINING_REWARD: f64 = 1.0;

    pub fn new(address: String) -> Self {
        let mut bc = BlockChain {
            transaction_pool: Vec::<Vec<u8>>::new(),
            chain: Vec::<Block>::new(),
            blockchain_address: address,
        };

        let b = Block::new(0, vec![0 as u8; 32]);
        bc.chain.push(b);
        bc.mining();

        bc //no semicolon
    }

    pub fn valid_chain(chain: &Vec<Block>) -> bool {
        let mut pre_block = &chain[0];
        for current_index in 1..chain.len() {
            let b = &chain[current_index];
            if b.previous_hash != pre_block.hash() {
                return false;
            }

            let hash = b.hash();
            let hash_str = hex::encode(&hash);
            if hash_str[0..BlockChain::DIFFICULTY] != "0".repeat(BlockChain::DIFFICULTY) {
                return false;
            }

            pre_block = b;
        }

        true
    }

    pub fn create_block(&mut self, nonce: i32, previous_hash: Vec<u8>) {
        let mut b = Block::new(nonce, previous_hash);
        for tx in self.transaction_pool.iter() {
            b.transactions.push(tx.clone());
        }
        self.transaction_pool.clear();
        let now = Instant::now();
        let proof_hash = BlockChain::do_proof_of_work(&mut b);
        let elapsed = now.elapsed();
        println!(
            "compute time: {:?}\nproof for the current block is :{:?}",
            elapsed, proof_hash
        );
        self.chain.push(b);
    }

    pub fn print(&self) {
        /*
        using ieterator to loop over the vector, it is complicate and powerful
        */
        for (i, block) in self.chain.iter().enumerate() {
            println!("{} Chain {} {}", "=".repeat(25), i, "=".repeat(25));
            block.print();
        }
        println!("{}", "*".repeat(25));
    }

    pub fn last_block(&self) -> &Block {
        if self.chain.len() > 1 {
            return &self.chain[self.chain.len() - 1];
        }

        &self.chain[0]
    }

    pub fn search_block(&self, search: BlockSearch) -> BlockSearchResult {
        for (idx, block) in self.chain.iter().enumerate() {
            match search {
                BlockSearch::SearchByIndex(index) => {
                    if idx == index {
                        return BlockSearchResult::Success(block);
                    }

                    if index >= self.chain.len() {
                        return BlockSearchResult::FailOfIndex(index);
                    }
                }
                BlockSearch::SearchByPreviousHash(ref hash) => {
                    /*
                    enum matching can cause data ownership transfer, the hash value
                    attach to search is transfer to the local variable of hash here,
                    when we go to the end of the code block here, the hash will be
                    dropped, then in the next round, we will have not value to get.
                    */
                    if block.previous_hash == *hash {
                        return BlockSearchResult::Success(block);
                    }

                    if idx >= self.chain.len() {
                        //the data type for FailOfPreviousHash is
                        //Vec<u8>, but the hash is &Vec<u8>
                        //to_vec will cause the &Vec<u8> to clone its Vec<u8> data
                        return BlockSearchResult::FailOfPreviousHash(hash.to_vec());
                    }
                }

                BlockSearch::SearchByBlockHash(ref hash) => {
                    if block.hash() == *hash {
                        return BlockSearchResult::Success(block);
                    }

                    if idx >= self.chain.len() {
                        return BlockSearchResult::FailOfBlockHash(hash.to_vec());
                    }
                }

                BlockSearch::SearchByNonce(nonce) => {
                    if block.nonce == nonce {
                        return BlockSearchResult::Success(block);
                    }

                    if idx >= self.chain.len() {
                        return BlockSearchResult::FailOfNonce(nonce);
                    }
                }

                BlockSearch::SearchByTimeStamp(time_stamp) => {
                    if block.time_stamp == time_stamp {
                        return BlockSearchResult::Success(block);
                    }

                    if idx >= self.chain.len() {
                        return BlockSearchResult::FailOfTimeStamp(time_stamp);
                    }
                }

                BlockSearch::SearchByTransaction(ref transaction) => {
                    for tx in block.transactions.iter() {
                        if tx == transaction {
                            return BlockSearchResult::Success(block);
                        }

                        if idx >= self.chain.len() {
                            return BlockSearchResult::FailOfTransaction(transaction.to_vec());
                        }
                    }
                }
            }
        }

        return BlockSearchResult::FailOfEmptyBlocks;
    }

    pub fn get_transactions(&self) -> Vec<Transaction> {
        let mut transactions = Vec::<Transaction>::new();
        for tx_in_pool in self.transaction_pool.iter() {
            transactions.push(Transaction::deserialization(tx_in_pool.to_vec()));
        }

        transactions
    }

    pub fn add_transaction(&mut self, tx: &WalletTransaction) -> bool {
        if tx.sender == self.blockchain_address {
            println!("minner cannot send money to himself");
            return false;
        }

        if tx.sender != BlockChain::MINING_SENDER && !Wallet::verify_transaction(tx) {
            println!("invalid transaction");
            return false;
        }

        // if tx.sender != BlockChain::MINING_SENDER
        //     && self.calculate_total_amount(tx.sender.clone()) < tx.amount as i64
        // {
        //     println!("sender dose not have enough balance");
        //     return false;
        // }

        let transaction = Transaction::new(
            tx.sender.as_bytes().to_vec(),
            tx.recipient.as_bytes().to_vec(),
            tx.amount,
        );

        for tx_in_pool in self.transaction_pool.iter() {
            if *tx_in_pool == transaction.serialization() {
                break;
            }
        }

        self.transaction_pool.push(transaction.serialization());
        true
    }

    fn do_proof_of_work(block: &mut Block) -> String {
        loop {
            let hash = block.hash();
            let hash_str = hex::encode(&hash);
            if hash_str[0..BlockChain::DIFFICULTY] == "0".repeat(BlockChain::DIFFICULTY) {
                return hash_str;
            }

            *block += 1;
        }
    }

    pub fn mining(&mut self) -> bool {
        /*
        when a block is mined, a transaction need to created to record the value that
        the blockchain send to the miner
        */
        let tx = WalletTransaction {
            sender: BlockChain::MINING_SENDER.clone().into(),
            recipient: self.blockchain_address.clone().into(),
            amount: BlockChain::MINING_REWARD,
            public_key: "".to_string(),
            signature: "".to_string(),
        };

        self.add_transaction(&tx);
        self.create_block(0, self.last_block().hash());

        true
    }

    pub fn calculate_total_amount(&self, address: String) -> f64 {
        let mut total_amount: f64 = 0.0;
        for i in 0..self.chain.len() {
            let block = &self[i];
            for t in block.transactions.iter() {
                let tx = Transaction::deserialization(t.clone());
                let value = tx.value;

                /*
                into is a trait used for converting one type into another,
                String implement many type of into trait, such as into<str>, into<i32>,
                into<u64> ..., into<Vec<u8>>,
                we need to tell the compiler which trait we should use that is
                into<Vec<u8>>
                */
                if <String as Into<Vec<u8>>>::into(address.clone()) == tx.recipient_address {
                    total_amount += value as f64;
                }

                if <String as Into<Vec<u8>>>::into(address.clone()) == tx.sender_address {
                    total_amount -= value as f64;
                }
            }
        }

        total_amount
    }
}
