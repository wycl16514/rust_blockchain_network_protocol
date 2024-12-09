use crate::blockchain::{transaction::Transaction as BlockchainTransaction, BlockChain};
use crate::wallet::{Transaction as WalletTranaction, Wallet};
use actix_web::{web, App, HttpResponse, HttpServer};
use log::{debug, error, info, trace, warn};

use rand::Rng;
use rand_core::block;
use regex::Regex;
use reqwest::Error;
use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use std::sync::{Arc, Mutex};
use std::{thread, time::Duration};
/*
concensus -> all nodes to have the same copy of blockchain

s:5000,    s:5001,   s:5002
tx:5000    tx:5000    tx:5000

action: mine

s:5000 -> mine -> s:5001, s:5002 to remove all the tx in their pool

s:5000 -> after mining, -> add a block to  its chain -> len of its own chain to change,
may break the data consistency amount all nodes

s:5000 -> concensus msg: 5001, 5002.

s:5000 -msg of concensus (lock the blockchain) -> s:5001 (return immediately, use random timer to)

 <- (blocked: require access to chain of s:5000)  msg of chain     <-

5001 receive concensus msg:
1, send chain msg to 5000, to get the chain
2, send chain msg to 5002, to get the chain
3, if it receive chains from 5000, 5002,
4, if compare the len of its own chain with chain from 5002, 5000, 3 chains
5, if there is the longest chain is from 5000 or 5002
6, replace its own chain with the longest chain

its the same for 5002

on distributed alogrithm totally a new topic which pass the score of this course
*/

#[derive(Serialize, Deserialize, Debug)]
struct PingResponse {
    pong: String,
}

#[derive(Serialize, Debug)]
pub struct TransactionsInBlockChain {
    transaction_count: usize,
    transactions: Vec<BlockchainTransaction>,
}

//http:://localhost:5000/amount/0x12345
#[derive(Serialize)]
struct QueryAmount {
    amount: f64,
}

#[derive(Clone, Debug)]
pub struct ApiServer {
    port: u16,
    /*
    clone the api server, we will only increase the reference count of Arc,
    and the mutex will remain only one
    */
    cache: Arc<Mutex<HashMap<String, BlockChain>>>,

    candidates: Arc<Mutex<Vec<String>>>,

    neighbors: Arc<Mutex<Vec<String>>>,
}

#[derive(Deserialize, Debug)]
pub struct Transaction {
    pub private_key: String,
    pub public_key: String,
    pub blockchain_address: String,
    pub recipient_address: String,
    pub amount: String,
}

impl ApiServer {
    const BLOCKCHAIN_PORT_RANGE_START: u16 = 5000;
    const BLOCKCHAIN_PORT_RANGE_END: u16 = 5003;
    const NEIGHBOR_IP_RANGE_START: u8 = 0;
    const NEIGHBOR_IP_RANGE_END: u8 = 1;
    const NEIGHBOR_IP_SYNC_TIME: u64 = 20;

    pub fn new(port: u16) -> Self {
        let cache = Arc::new(Mutex::new(HashMap::new()));
        let neighbors = Arc::new(Mutex::new(vec![]));
        let candidates = Arc::new(Mutex::new(vec![]));
        let mut api_server = ApiServer {
            port,
            cache,
            candidates,
            neighbors,
        };

        let wallet_miner = Wallet::new();
        {
            /*
            get the lock of Mutex will return Result<ApiServer, None>, if we get lock ok,
            we need to use unwrap to get the ApiServer from Result

            The lock will cause reference to mutex, reference to the ApiServer,
            need to release the lock before returning the api_server,
            no unlock method, only way to unlock the mutex is let it go out of its scope
            */
            let mut unlock_cache = api_server.cache.lock().unwrap();
            unlock_cache.insert(
                "blockchain".to_string(),
                BlockChain::new(wallet_miner.get_address()),
            );
        }

        return api_server;
    }

    pub async fn handle_ping() -> HttpResponse {
        info!("receiving ping request....");
        let response = PingResponse {
            pong: "pong".to_string(),
        };
        HttpResponse::Ok().json(response)
    }

    pub async fn get_amount(
        data: web::Data<Arc<ApiServer>>,
        path: web::Path<String>,
    ) -> HttpResponse {
        let address = path.into_inner();
        let api_server = data.get_ref();
        let unlock_cache = api_server.cache.lock().unwrap();
        let block_chain = unlock_cache.get("blockchain").unwrap();
        let amount = block_chain.calculate_total_amount(address);
        let amount_return = QueryAmount { amount };

        HttpResponse::Ok().json(amount_return)
    }

    pub async fn mining(data: web::Data<Arc<ApiServer>>) -> HttpResponse {
        let api_server = data.get_ref();
        let mut unlock_cache = api_server.cache.lock().unwrap();
        let block_chain = unlock_cache.get_mut("blockchain").unwrap();
        let is_mined = block_chain.mining();
        if !is_mined {
            return HttpResponse::InternalServerError().json("mining fail");
        }

        //1. notify peers to remove txs in their pool
        Self::delete_neighbors_transaction_in_pool(api_server).await;

        //2.send concensus msg to peers to ask them update their own chain
        Self::build_concensus(api_server).await;

        HttpResponse::Ok().json("mining ok")
    }

    pub async fn build_concensus(api_server: &ApiServer) -> Result<(), reqwest::Error> {
        let neighbors = api_server.neighbors.lock().unwrap();
        let client = reqwest::Client::builder()
            .no_proxy()
            .timeout(Duration::from_secs(5))
            .build()?;

        for neighbor in &*neighbors {
            let url = format!("http://{}/concensus", neighbor);
            client.get(url.clone()).send().await?;
        }

        Ok(())
    }

    pub async fn handle_concensus(data: web::Data<Arc<ApiServer>>) -> HttpResponse {
        let api_server = data.get_ref();
        /*
        need to spaw thread to do concensus, since it is triggered by resolve_conflic
        by peer, which means the blockchain object is still locked by peer. When
        handling concensus, we need to request /chain from the same peer, which will
        cause the peer to access the blockchain object. But the object is still in
        lock state, then doing it directly will cause deadlock
        */
        let api_server_clone = api_server.clone();
        thread::spawn(move || {
            thread::sleep(Duration::from_secs(2));
            let runtime = tokio::runtime::Runtime::new().unwrap();
            runtime.block_on(Self::do_concensus(&api_server_clone));
        });

        HttpResponse::Ok().json("ok")
    }

    pub async fn do_concensus(api_server: &ApiServer) {
        let result = Self::resolve_conflicts(api_server).await;
        if let Ok(res) = result {
            if res {
                info!(
                    "blockchain is replaced by concensus for server of port: {}",
                    api_server.port
                );
            } else {
                info!(
                    "blockchain is not replaced for server of port: {}",
                    api_server.port
                );
            }
        }

        if let Err(err) = result {
            info!(
                "server with port:{} do concensus fail with err:{:?}",
                api_server.port, err
            );
        }
    }

    pub async fn resolve_conflicts(api_server: &ApiServer) -> Result<bool, reqwest::Error> {
        info!("in resolve conflicts with port:{}", api_server.port);

        let mut unlock_cache = api_server.cache.lock().unwrap();
        let block_chain = unlock_cache.get_mut("blockchain").unwrap();
        let neighbors = api_server.neighbors.lock().unwrap();
        let mut change_chain = false;
        let mut max_length = block_chain.chain.len();

        info!(
            "in resolve conflicts with port: {} begin...",
            api_server.port
        );

        let client = reqwest::Client::builder()
            .no_proxy()
            .timeout(Duration::from_secs(5))
            .build()?;

        for neighbor in &*neighbors {
            let mut rng = rand::thread_rng();
            let random_secs: u64 = rng.gen_range(1..=5);
            thread::sleep(Duration::from_secs(random_secs));

            info!(
                "begin ask chain from neighbor:{}, from server with port:{}",
                neighbor, api_server.port
            );
            let url = format!("http://{}/chain", neighbor);
            let neighbor_block_chain: BlockChain =
                client.get(url.clone()).send().await?.json().await?;

            info!(
                "server:{} get chain with len:{} for url:{}",
                api_server.port,
                neighbor_block_chain.chain.len(),
                url.clone()
            );

            if neighbor_block_chain.chain.len() > max_length
                && BlockChain::valid_chain(&neighbor_block_chain.chain)
            {
                max_length = neighbor_block_chain.chain.len();
                block_chain.chain = neighbor_block_chain.chain.clone();
                change_chain = true;
                info!(
                    "server with port:{}, find the longest chain with neighbor:{}",
                    api_server.port, neighbor
                );
            }
        }

        Ok(change_chain)
    }

    pub async fn handle_chain(data: web::Data<Arc<ApiServer>>) -> HttpResponse {
        let api_server = data.get_ref();
        let mut unlock_cache = api_server.cache.lock().unwrap();
        let block_chain = unlock_cache.get("blockchain").unwrap();
        HttpResponse::Ok().json(block_chain)
    }

    pub async fn delete_neighbors_transaction_in_pool(
        api_server: &ApiServer,
    ) -> Result<(), reqwest::Error> {
        let client = reqwest::Client::builder()
            .no_proxy()
            .timeout(Duration::from_secs(5))
            .build()?;

        let neighbors = api_server.neighbors.lock().unwrap();
        for neighbor in &*neighbors {
            let url = format!("http://{}/delete_transactions", neighbor);
            client.get(url).send().await?;
        }

        Ok(())
    }

    pub async fn delete_transacion_pool(data: web::Data<Arc<ApiServer>>) -> HttpResponse {
        let api_server = data.get_ref();
        let mut unlock_cache = api_server.cache.lock().unwrap();
        let block_chain = unlock_cache.get_mut("blockchain").unwrap();
        block_chain.transaction_pool.clear();
        HttpResponse::Ok().json("transaction pool clear")
    }

    async fn get_wallet() -> HttpResponse {
        HttpResponse::Ok().content_type("text/html").body(
            r#"
            <!DOCTYPE html>
            <html lang="en">
            <head>
               <meta charset="UTF-8"/>
               <title>Wallet</title>
               <script src="https://ajax.googleapis.com/ajax/libs/jquery/3.7.1/jquery.min.js"></script>
               <script>
                  $(function(){
                    $.ajax({
                        url: "/get_wallet",
                        method: "GET",
                        success: function(response) {
                            console.log(response);
                            $("\#public_key").val(response['public_key'])
                            $("\#private_key").val(response['private_key'])
                            $("\#blockchain_address").val(response['blockchain_address'])
                        },
                        error: function(error) {
                            console.log(error)
                        }
                    })

                    $("\#send_money").click(function(){
                        let confirm_text = 'Are you ready to send the given amount?'
                        let confirm_result = confirm(confirm_text)
                        if (!confirm_result) {
                            alert('Transaction cancelled')
                            return
                        }

                        let transaction = {
                            'private_key': $("\#private_key").val(),
                            'blockchain_address': $("\#blockchain_address").val(),
                            'public_key': $("\#public_key").val(),
                            'recipient_address': $("\#recipient_address").val(),
                            'amount': $("\#send_amount").val(),
                        }

                        $.ajax({
                            url: "/transaction",
                            method: "POST",
                            contentType: "application/json",
                            data: JSON.stringify(transaction),
                            success: function(response) {
                                console.log(response)
                                alert('success')
                            },
                            error: function(error) {
                                console.error(error)
                                alert('error')
                            }
                        })
                    })

                    function reload_amount() {
                        const address = $("\#blockchain_address").val()
                        console.log("get amount for address:", address)
                        const url = `/amount/${address}`
                        console.log("query amount url: ", url)
                        $.ajax({
                            url: url,
                            type: "GET",
                            success: function(response) {
                                let amount = response["amount"]
                                $("\#input_amount").text(amount)
                                console.log(amount)
                            },
                            error: function(error) {
                                console.error(error)
                            }
                        })
                    }

                    $("\#refresh_wallet").click(function(){
                        reload_amount()
                    })

                    setInterval(reload_amount, 3000)
                  })
               </script>
            </head>
            <body>
                <div>
                  <h1>Wallet</h1>
                   <div id="input_amount">0</div>
                   <button id="refresh_wallet">Refresh Wallet</button>
                   <p>Publick Key</p>
                   <textarea id="public_key" row="2" cols="100">
                   </textarea>

                   <p>Private Key</p>
                   <textarea id="private_key" row="1" cols="100">
                   </textarea>

                    <p>Blockchain address</p>
                   <textarea id="blockchain_address" row="1" cols="100">
                   </textarea>
                </div>

                <div>
                    <h1>Send Money</h1>
                    <div>
                        Address: <input id="recipient_address" size="100" type="text"></input>
                        <br>
                        Amount: <input id="send_amount" type="text"/>
                        <br>
                        <button id="send_money">Send</button>
                    </div>
                </div>

            </body>
            </html>
            "#
        )
    }

    pub async fn handle_sync_transaction(
        data: web::Data<Arc<ApiServer>>,
        transaction: web::Json<WalletTranaction>,
    ) -> HttpResponse {
        info!("handle sync transaction...");
        let wallet_tx = transaction.into_inner();
        let api_server = data.get_ref();
        let mut unlock_cache = api_server.cache.lock().unwrap();
        let block_chain = unlock_cache.get_mut("blockchain").unwrap();
        let add_result = block_chain.add_transaction(&wallet_tx);
        if !add_result {
            info!("sync transaction to blockchain fail");
            return HttpResponse::InternalServerError().json("sync transaction to blockchain fail");
        }
        info!("sync transaction to blockchain ok");
        return HttpResponse::Ok().json("sync transaction to blockchain ok");
    }

    pub async fn sync_transaction_with_neighbors(
        api_server: &ApiServer,
        transaction: &WalletTranaction,
    ) -> Result<(), reqwest::Error> {
        //info!("begin to sync tx with neighbors...");
        //Mutex<Vec<String>>
        let neighbors = api_server.neighbors.lock().unwrap();
        let client = reqwest::Client::builder()
            .no_proxy()
            .timeout(Duration::from_secs(5))
            .build()?;

        /*
        Vec<String> is wrapped by Mutex, *neighbors => get the Vec<String> out from
        Mutex, &*neighbors => &Vec<String>, withour &, it will move all the ip string
        out from the vector
        */
        for neighbor in &*neighbors {
            let url = format!("http://{}/sync_transaction", neighbor);
            info!("sync neighbor with url : {}", url);
            let response = client.post(url).json(transaction).send().await?;
            info!(
                "sync tx with neighbor: {} and result is: {}",
                neighbor,
                response.text().await?
            );
        }

        Ok(())
    }

    pub async fn get_transaction_handler(
        data: web::Data<Arc<ApiServer>>,
        transaction: web::Json<Transaction>,
    ) -> HttpResponse {
        let tx = transaction.into_inner();
        debug!("receive json info: {:?}", tx);
        //parse return Result
        let amount = tx.amount.parse::<f64>().unwrap();
        //need to create wallet instance from the transaction
        let wallet = Wallet::new_from(&tx.public_key, &tx.private_key, &tx.blockchain_address);
        let wallet_tx = wallet.sign_transaction(&tx.recipient_address, amount);
        let api_server = data.get_ref();
        let mut unlock_cache = api_server.cache.lock().unwrap();
        let block_chain = unlock_cache.get_mut("blockchain").unwrap();
        let add_result = block_chain.add_transaction(&wallet_tx);
        if !add_result {
            info!("add transaction to blockchain fail");
            return HttpResponse::InternalServerError().json("add transaction to blockchain fail");
        }
        info!("add transaction to blockchain ok");
        //Return a Future object
        let _ = Self::sync_transaction_with_neighbors(api_server, &wallet_tx).await;
        return HttpResponse::Ok().json("add transaction to blockchain ok");
    }

    pub async fn show_transaction(data: web::Data<Arc<ApiServer>>) -> HttpResponse {
        let api_server = data.get_ref();
        let unlock_cache = api_server.cache.lock().unwrap();
        let block_chain = unlock_cache.get("blockchain").unwrap();
        let mut get_transactions = TransactionsInBlockChain {
            transaction_count: 0,
            transactions: Vec::<BlockchainTransaction>::new(),
        };
        get_transactions.transactions = block_chain.get_transactions();
        get_transactions.transaction_count = get_transactions.transactions.len();
        debug!("show transactions in chain:{:?}", get_transactions);
        HttpResponse::Ok().json(get_transactions)
    }

    async fn get_wallet_handler() -> HttpResponse {
        let wallet_user = Wallet::new();
        let wallet_data = wallet_user.get_wallet_data();
        HttpResponse::Ok().json(wallet_data)
    }

    async fn get_index(&self) -> HttpResponse {
        let unlock_cache = self.cache.lock().unwrap();
        let blockchain = unlock_cache.get("blockchain").unwrap();
        let blocks = &blockchain.chain;
        HttpResponse::Ok().json(blocks)
    }

    pub async fn get_index_handler(data: web::Data<Arc<ApiServer>>) -> HttpResponse {
        info!("Receiving request at '/' endpoint");
        debug!("Handler received ApiServer data: {:?}", data);

        data.get_ref().get_index().await
    }

    pub async fn run(&self) {
        /*
        the server will create 4 worker threads to handle requests,
        share data,
        use Arc automatic resource counter to wrap the api server
        */
        let api = Arc::new(self.clone());
        api.sync_neighbors();
        Self::resolve_conflicts(&api).await;

        let server = HttpServer::new(move || {
            App::new()
                .app_data(web::Data::new(api.clone()))
                .wrap(actix_web::middleware::Logger::default())
                .route("/", web::get().to(Self::get_index_handler))
                .route("/wallet", web::get().to(Self::get_wallet))
                .route("/get_wallet", web::get().to(Self::get_wallet_handler))
                .route(
                    "/transaction",
                    web::post().to(Self::get_transaction_handler),
                )
                .route("/show_transactions", web::get().to(Self::show_transaction))
                .route("/mining", web::get().to(Self::mining))
                .route("/amount/{address}", web::get().to(Self::get_amount))
                .route("/ping", web::get().to(Self::handle_ping))
                .route(
                    "/sync_transaction",
                    web::post().to(Self::handle_sync_transaction),
                )
                .route(
                    "/delete_transactions",
                    web::get().to(Self::delete_transacion_pool),
                )
                .route("/chain", web::get().to(Self::handle_chain))
                .route("/concensus", web::get().to(Self::handle_concensus))
        });

        println!("Server running on port:{}", self.port);
        server
            .bind(("0.0.0.0", self.port))
            .unwrap()
            .run()
            .await
            .expect("Error running the server");
    }

    pub fn sync_neighbors(&self) {
        info!("run sync neighbors....");
        self.get_neighbors();
        let api_clone = self.clone();
        thread::spawn(move || loop {
            info!("timer thred looping...");
            api_clone.register_neighbors();
            thread::sleep(Duration::from_secs(Self::NEIGHBOR_IP_SYNC_TIME));
        });
    }

    pub fn register_neighbors(&self) {
        let candidates = self.candidates.lock().unwrap();
        let mut neighbors = self.neighbors.lock().unwrap();
        let candidates_vec = candidates.clone();

        for candidate in &candidates_vec {
            let contains = neighbors.iter().any(|s| s == candidate);
            if contains {
                info!(
                    "candidate: {} already synced by server with port: {}",
                    candidate, self.port
                );
                continue;
            }

            info!("ping candidate: {}", candidate);
            let runtime = tokio::runtime::Runtime::new().unwrap();
            runtime.block_on(self.ping_neighbor(candidate, &mut neighbors));
        }
    }

    pub async fn ping_neighbor(
        &self,
        candidate: &String,
        neighbors: &mut Vec<String>,
    ) -> Result<(), reqwest::Error> {
        let url = format!("http://{}/ping", candidate);
        //127.0.0.2:5000
        let client = reqwest::Client::builder()
            .timeout(Duration::from_secs(5))
            .no_proxy()
            .build()?;

        let response = client.get(url.clone()).send().await?;

        if response.status().is_success() {
            let json: PingResponse = response.json().await.expect("fail to get pong back");
            if json.pong == "pong" {
                info!(
                    "current server with port:{}, ping neighbor with url:{} success",
                    self.port, url
                );
                neighbors.push(candidate.clone());
            }
        }

        Ok(())
    }

    pub fn get_neighbors(&self) {
        let ip = "127.0.0.1".to_string() + &":".to_string() + &self.port.to_string();
        let ipv4_regex = Regex::new(
        r"\b((25[0-5]|2[0-4][0-9]|1[0-9]{2}|[1-9]?[0-9])\.){3}(25[0-5]|2[0-4][0-9]|1[0-9]{2}|[1-9]?[0-9])\b"
    ).unwrap();
        let ips: Vec<String> = ipv4_regex
            .find_iter(ip.as_str()) //String -> &str
            .map(|mat| mat.as_str().to_string())
            .collect();
        let current_ip = ip.clone();
        let ip_parts: Vec<&str> = ips[0].split('.').collect();
        let last_ip = ip_parts[3].parse::<u8>().unwrap();
        let mut neighbors = self.candidates.lock().unwrap();
        for port in Self::BLOCKCHAIN_PORT_RANGE_START..=Self::BLOCKCHAIN_PORT_RANGE_END {
            for ip in Self::NEIGHBOR_IP_RANGE_START..=Self::NEIGHBOR_IP_RANGE_END {
                let guess_host =
                    ip_parts[0..3].join(".").to_string() + "." + &(last_ip + ip).to_string();
                let guess_target = guess_host + ":" + &port.to_string();
                if guess_target != current_ip {
                    neighbors.push(guess_target);
                }
            }
        }

        info!("find neighbors: {:?}", neighbors);
    }
}
