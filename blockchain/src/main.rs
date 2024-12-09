pub mod apiserver;
use crate::apiserver::ApiServer;
pub mod blockchain;
pub mod wallet;
use actix_web::middleware::Logger;
use std::thread;

/*
127.0.0.1:5000

127.0.0.1:5000 -> 127.0.0.2:5003

127.0.0.1:5000
127.0.0.1:5001
127.0.0.1:5002
127.0.0.1:5003

127.0.0.2:5000
127.0.0.2:5001
127.0.0.2:5002
127.0.0.2:5003

*/

fn main() {
    env_logger::init();

    let ports = vec![5000, 5001, 5002];
    let mut handles = vec![];
    for port in ports {
        let server = ApiServer::new(port);
        let handle = thread::spawn(move || {
            let runtime = tokio::runtime::Runtime::new().unwrap();
            runtime.block_on(server.run());
        });

        handles.push(handle);
    }

    for handle in handles {
        handle.join().unwrap();
    }
}
