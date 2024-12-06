One of great inovation of blockchain is its decentralize. There are countless node running on the same blockchain in any corner of the world and there is a protocol to help all distributed nodes to act in a consistency way. For eample 
if a node in US add a new block to its chain, then it will sync the info of the node to every node around the way then just for a monent all nodes around the world can have the newly added block.

From this section, let's see how block chain nodes archive this. First we need to run multiple servers on multiple threads to simulate nodes running around the world, we need to change our code as following. First we need to add the tokio
crate, and we chage the cargo.toml as following:

```rs
[dependencies]
...
tokio = { version = "1", features = ["full"] }
```

tokio crate can create a virtual environment to run aysnc code, since our api server need to run in async mode, and commond thread like std::thread is not allow any async task to run inside it, then we need to utilize the runtime object
from tokio to generate an virtual environment to let the api_server to run as async task inside it, the runtime object will block on the given async task until its end. Therefore we will change code in main.rs as following:

```rs

pub mod apiserver;
pub mod blockchain;
pub mod wallet;
use crate::apiserver::ApiServer;
use actix_web::middleware::Logger;
use std::thread;

fn main() {
    // Initialize the logger with default settings
    env_logger::init();
    let ports = vec![5000, 5001, 5002];
    let mut handles = vec![];
    for port in ports {
        let server = ApiServer::new(port);
        let handle = thread::spawn(move || {
            let runtime = tokio::runtime::Runtime::new().unwrap();
            //runtime of tokio can handle async function
            runtime.block_on(server.run());
        });
        handles.push(handle);
    }

  
    for handle in handles {
        handle.join().unwrap();
    }
}

```
In above code, we create 3 threads to run three server instance, each instance running on port from 5000 to 5002. As we know server.run is an async function, then we use runtime from tokio crate to create an environment to run the async 
function and block on that function until its end. Then we can create 3 threads, and each thread can run each server instance, eventhoung ApiServer is running asynchronously, but we put it inside the block_on method of runtime, then for
the thread object, the server is just like running itself in a synchronous way.

Run above code and make sure there are three server instances, and you can get wallet from each server by using http://localhost:5000/wallet, http://localhost:5002/wallet, http://localhost:5002/wallet respectively.

When the blockchain server up and running, it needs to find whether there are any peers, it dose this by constructing an array of candidate ips, and then send query message for each ip, if it can get positive response, then it will know the
given ip is the same blockchain peer and it will coorperate with that peer for data consitency, let's see how we can construct the candidate ips. For a server with ip as 127.0.0.1:5000, it will construct an array of candidate ips by using 
the last ip digit with port range. 

For example if we fix the last ip digit range from 0 to 2, then ip of 127.0.0.1, 127.0.0.2 (1+2), 127.0.0.3 (1+2) will be candidate ip. If we fix the range of port from 5000 t0 5002, then ip candidate would be 127.0.0.1:5000, 127.0.0.1:5001,
127.0.0.1:5002, 127.0.0.1:5000, 127.0.0.2:5001, 127.0.0.2:5002, 127.0.0.3:5000, 127.0.0.3:5001, 127.0.0.3:5002 are all candidate ips, let's see how we use code to implement it, first we need to add dependency as following in cargo.toml:

```rs
[dependencies]
regex = "1.9"
```

Then in mod.rs of ApiServer:

```rs
use regex::Regex;

#[derive(Clone, Debug)]
pub struct ApiServer {
    port: u16,
    /*
    clone the api server, we will only increase the reference count of Arc,
    and the mutex will remain only one
    */
    cache: Arc<Mutex<HashMap<String, BlockChain>>>,

    neighbors: Arc<Mutex<Vec<String>>>,
}
```

In above code, we add a new field to ApiServer which is an vector to save candidate ips. 

```rs
impl ApiServer {
    const BLOCKCHAIN_PORT_RANGE_START: u16 = 5000;
    const BLOCKCHAIN_PORT_RANGE_END: u16 = 5003;
    const NEIGHBOR_IP_RANGE_START: u8 = 0;
    const NEIGHBOR_IP_RANGE_END: u8 = 1;
    const NEIGHBOR_IP_SYNC_TIME: u8 = 20;
        pub fn new(port: u16) -> Self {
        let cache = Arc::new(Mutex::new(HashMap::new()));
        let neighbors = Arc::new(Mutex::new(vec![]));
        let mut api_server = ApiServer {
            port,
            cache,
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
}
```
In above code, we fix the last ip digit range from 0 to 1, and the port range from 5000 to 5003, and in the new constructor, we initialized a an empty vector to save possible ips. Let's add a method for constructing candiate ip as 
following:

```rs
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
        let mut neighbors = self.neighbors.lock().unwrap();
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
```
In above code, we first construct the ip and port for current server instance. Why should we use the regular expression to extract "127.0.0.1"? Actually we can ignore the regex and using "127.0.0.1" directly. But if we want to get the
real ip address, then we will need the regular expression to extract the ip4 address out. When we get the ip address, we get the last ip digit and change this digit in the given ip digit range, and go over the port range to get each
port number and construct the target peer ip.

In main.rs we can test above code as following:

```rs
fn main() {
    env_logger::init();
    let server = ApiServer::new(5000);
    server.get_neighbors();
}

```

Run above code we will get following result:

```rs
find neighbors: ["127.0.0.2:5000", "127.0.0.1:5001", "127.0.0.2:5001", "127.0.0.1:5002", "127.0.0.2:5002", "127.0.0.1:5003", "127.0.0.2:5003"]
```
