use std::collections::{HashMap, HashSet, VecDeque};
use async_std::sync::Arc;
use tokio::runtime::Runtime;
use tokio::sync::RwLock;

use garb_sync_aptos::Pool;

fn main() -> Result<(), Box<dyn std::error::Error>> {
    let rt = Runtime::new()?;
    rt.block_on(async_main())?;
    Ok(())
}
pub async fn async_main() -> anyhow::Result<()> {
    // pools: will be loaded initially by sync module
    // after loading the pools we will poll every n seconds for new events on the loaded pools
    let pools = Arc::new(RwLock::new(HashMap::<String, Pool>::new()));

    // update_q: is the queue that holds the updated pools.
    // we find an updated pool by polling events on chain
    // aptos doesn't have websocket service to listen to on-chain events when they are committed
    // so we have to query events iteratively
    // when there is an event triggered by a pool that pool is added to update_q
    // then we find all the routes that go through the updated pool and try them if they are profitable
    let update_q = Arc::new(RwLock::new(VecDeque::<Pool>::new()));

    // routes holds the routes that pass through an updated pool
    // this will be populated by the graph module when there is an updated pool
    let routes = Arc::new(RwLock::new(HashSet::<Vec<Pool>>::new()));

    // we start the sync service first to load the pools first
    garb_sync_aptos::start(pools.clone(), update_q.clone())
        .await
        .unwrap();
    
    let (_, _) = tokio::join!(
        garb_graph_aptos::start(pools.clone(), update_q.clone(), routes.clone()),
        transactor(routes.clone())
    );
    Ok(())
}
// Transactor
// routes: the updated paths compiled by the graph task
//         it is updated every time there is an event on a pool on chain
//         we will spam the network with transaction simulations and pick the best one
pub async fn transactor(routes: Arc<RwLock<HashSet<Vec<Pool>>>>) {
    loop {
        let mut routes = routes.write().await;
        if routes.len() <= 0 {
            continue;
        }
        //println!("transactor> Routes to simulate {:?}", routes.len());
        let route = routes.iter().next().unwrap().clone();
        let _r = routes.take(&route).unwrap();
        std::mem::drop(routes);
        println!("`````````````````````` Simulating Route ``````````````````````");
        for (i, pool) in route.iter().enumerate() {
            println!("{}. {}", i + 1, pool);
        }
        println!("\n\n");

        // TODO: Write a move module that will take a vector of pools and simulates the transaction
    }
}
