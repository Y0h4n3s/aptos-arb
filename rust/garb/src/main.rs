use std::collections::{HashMap, HashSet, VecDeque};
use std::str::FromStr;
use std::time::Duration;
use aptos_sdk::crypto::_once_cell::sync::Lazy;
use aptos_sdk::crypto::ed25519::{Ed25519PrivateKey};
use aptos_sdk::crypto::ValidCryptoMaterialStringExt;
use aptos_sdk::move_types::identifier::Identifier;
use aptos_sdk::move_types::language_storage::{ModuleId, StructTag, TypeTag};
use aptos_sdk::rest_client::Client;
use aptos_sdk::types::account_address::AccountAddress;
use aptos_sdk::types::chain_id::ChainId;
use aptos_sdk::types::LocalAccount;
use aptos_sdk::types::transaction::{EntryFunction, TransactionPayload};
use async_std::sync::Arc;
use tokio::runtime::Runtime;
use tokio::sync::RwLock;
use url::Url;
use itertools::Itertools;
use tokio::sync::mpsc::Receiver;
use garb_sync_aptos::Pool;
static NODE_URL: Lazy<Url> = Lazy::new(|| {
    Url::from_str(
        std::env::var("APTOS_NODE_URL")
              .as_ref()
              .map(|s| s.as_str())
              .unwrap_or("https://aptos-mainnet.pontem.network/"),
    )
          .unwrap()
});

static KEY: Lazy<LocalAccount> = Lazy::new(|| {
   LocalAccount::new(
       AccountAddress::from_hex_literal(std::env::var("APTOS_PUBLIC_KEY").unwrap().as_str()).unwrap(),
       Ed25519PrivateKey::from_encoded_string(std::env::var("APTOS_PRIVATE_KEY").unwrap().as_str()).unwrap(),
       0
   )
   
});
//TODO: use message channels between sync and graph, and graph and routes
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
    let (update_q_sender, mut update_q_receiver) = tokio::sync::mpsc::channel::<Pool>(100);
    // routes holds the routes that pass through an updated pool
    // this will be populated by the graph module when there is an updated pool
    let (routes_sender, mut routes_receiver) = tokio::sync::mpsc::channel::<Vec<Pool>>(100);
    // we start the sync service first to load the pools first
    garb_sync_aptos::start(pools.clone(), Arc::new(update_q_sender))
        .await
        .unwrap();
    
    let (_, _) = tokio::join!(
        garb_graph_aptos::start(pools.clone(), update_q_receiver, Arc::new(routes_sender)),
        transactor(&mut routes_receiver)
    );
    Ok(())
}
// Transactor
// routes: the updated paths compiled by the graph task
//         it is updated every time there is an event on a pool on chain
//         we will spam the network with transaction simulations and pick the best one
pub async fn transactor(routes: &mut Receiver<Vec<Pool>>) {
    while let Some(route) = routes.recv().await {
        // TODO: Write a move module that will take a vector of pools and simulates the transaction
        //       and executes the best one
    
        tokio::spawn(async move {
            println!("`````````````````````` Simulating Route ``````````````````````");
            for (i, pool) in route.iter().enumerate() {
                println!("{}. {}", i + 1, pool);
            }
            println!("\n\n");
            let aptos_client = Client::new_with_timeout(
                NODE_URL.clone(),
                Duration::from_secs(45),
            );
            let i = aptos_client
                  .get_index()
                  .await;
            if let Ok(index) = i {
                let mut type_tags: Vec<TypeTag> = route.iter()
                                                       .map(|p| if p.x_to_y { vec![p.x_address.clone(), p.y_address.clone()] } else { vec![p.y_address.clone(), p.x_address.clone()] })
                                                       .flatten()
                                                       .unique()
                                                       .map(|a| TypeTag::Struct(Box::new(StructTag::from_str(&a).unwrap())))
                                                       .collect();
                let first_pool = route.first().unwrap();
                type_tags.push(TypeTag::Struct(Box::new(StructTag::from_str(if first_pool.x_to_y { &first_pool.x_address } else { &first_pool.y_address }).unwrap())));
                type_tags.push(TypeTag::Struct(Box::new(StructTag::from_str(if first_pool.x_to_y { &first_pool.x_address } else { &first_pool.y_address }).unwrap())));
                type_tags.push(TypeTag::Struct(Box::new(StructTag::from_str(if first_pool.x_to_y { &first_pool.x_address } else { &first_pool.y_address }).unwrap())));
                type_tags.push(TypeTag::Struct(Box::new(StructTag::from_str(if first_pool.x_to_y { &first_pool.x_address } else { &first_pool.y_address }).unwrap())));
        
                let mut args = vec![];
        
                for (_i, pool) in route.iter().enumerate() {
                    args.push((&(pool.clone().provider.id as u8)).to_be_bytes().to_vec());
                    args.push(1_u64.to_be_bytes().to_vec());
                    args.push(1_u8.to_be_bytes().to_vec());
                }
                args.push(1_u64.to_be_bytes().to_vec());
                args.push(1_u64.to_be_bytes().to_vec());
                let tx_f = aptos_sdk::transaction_builder::TransactionFactory::new(ChainId::new(index.inner().chain_id));
                let tx = tx_f.payload(TransactionPayload::EntryFunction(
                    EntryFunction::new(
                        ModuleId::new(AccountAddress::from_str("0x89576037b3cc0b89645ea393a47787bb348272c76d6941c574b053672b848039").unwrap(), Identifier::new("aggregator").unwrap()),
                        Identifier::new("three_step_route").unwrap(),
                        type_tags,
                        args
                    )
                ))
                             .sender(KEY.address())
                             .sequence_number(index.inner().ledger_timestamp.0)
                             .build();
                println!("tx: {:?}", tx);
                let signed_tx = KEY.sign_transaction(tx);
                let sim_result = aptos_client.simulate_bcs_with_gas_estimation(&signed_tx, true, true).await;
                if let Ok(result) = sim_result {
                    println!("sim_result: {:?}", result);
                } else {
                    eprintln!("sim_result: {:?}", sim_result.unwrap_err());
                }
            } else {
                eprintln!("Error getting index {:?}", i.unwrap_err());
            }
        });
        
    }
}
