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
