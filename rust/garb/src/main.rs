use aptos_sdk::crypto::_once_cell::sync::Lazy;
use aptos_sdk::crypto::ed25519::Ed25519PrivateKey;
use aptos_sdk::crypto::ValidCryptoMaterialStringExt;
use aptos_sdk::move_types::identifier::Identifier;
use aptos_sdk::move_types::language_storage::{ModuleId, StructTag, TypeTag};
use aptos_sdk::rest_client::Client;
use aptos_sdk::types::account_address::AccountAddress;
use aptos_sdk::types::chain_id::ChainId;
use aptos_sdk::types::transaction::{
    EntryFunction, ExecutionStatus, TransactionInfo, TransactionPayload,
};
use aptos_sdk::types::LocalAccount;
use async_std::sync::Arc;
use garb_sync_aptos::{EventSource, LiquidityProvider, LiquidityProviders, Pool, SyncConfig};
use std::collections::{HashMap, HashSet};
use std::str::FromStr;
use std::time::Duration;
use aptos_sdk::types::vm_status::StatusCode;
use tokio::runtime::Runtime;
use tokio::sync::RwLock;
use url::Url;
use clap::Parser;
use garb_graph_aptos::Order;

// 0x7eb53ac5b9d0c6dd0b29da998a9c1d1e6f8592c677d8e601c1bbde4fcd0c1480
// 0xfd2594ac71d95d1e86df9921d03ad2a409b871ee7f866560c21ff17945fd2fca
static NODE_URLS: Lazy<Vec<Url>> = Lazy::new(|| {
    vec![ Url::from_str(
        std::env::var("APTOS_NODE_URL")
            .as_ref()
            .map(|s| s.as_str())
            .unwrap_or("http://val1.mainnet.aptos.p2p.org"),
    )
    .unwrap(),
          // Url::from_str(
          //     std::env::var("APTOS_NODE_URL")
          //           .as_ref()
          //           .map(|s| s.as_str())
          //           .unwrap_or("https://rpc.argo.fi"),
          // )
          //       .unwrap(),
          Url::from_str(
              std::env::var("APTOS_NODE_URL")
                    .as_ref()
                    .map(|s| s.as_str())
                    .unwrap_or("http://aptos.lavenderfive.com/"),
          )
                .unwrap()
    ]
});



static KEY: Lazy<LocalAccount> = Lazy::new(|| {
    LocalAccount::new(
        AccountAddress::from_hex_literal(std::env::var("APTOS_PUBLIC_KEY").unwrap_or_else(|_| std::env::args().nth(1).unwrap()).as_str())
            .unwrap(),
        Ed25519PrivateKey::from_encoded_string(
            std::env::var("APTOS_PRIVATE_KEY").unwrap_or_else(|_|std::env::args().nth(2).unwrap()).as_str(),
        )
        .unwrap(),
        0,
    )
});

static PARALLEL_REQUESTS: Lazy<usize> = Lazy::new(|| {
    std::env::var("APTOS_PARALLEL_REQUESTS").unwrap_or_else(|_| std::env::args().nth(3).unwrap())
        .parse()
        .unwrap_or(100)
});

static MAX_GAS_UNITS: Lazy<u64> = Lazy::new(|| {
    std::env::var("APTOS_MAX_GAS_UNITS").unwrap_or_else(|_| std::env::args().nth(4).unwrap())
                                            .parse()
                                            .unwrap_or(20000)
});
static PROVIDERS: Lazy<Vec<LiquidityProviders>> = Lazy::new(|| {
    std::env::var("APTOS_PROVIDERS").unwrap_or_else(|_| std::env::args().nth(5).unwrap())
        .split(",")
          .map(|s| s.parse::<u8>().unwrap())
          .map(|i| LiquidityProviders::from(i))
        .collect()
});
static QTY: Lazy<u64> = Lazy::new(|| {
    std::env::var("APTOS_QTY").unwrap_or_else(|_| std::env::args().nth(5).unwrap())
                                    .parse()
                                    .unwrap_or(100)
});

//TODO: use message channels between sync and graph, and graph and routes
fn main() -> Result<(), Box<dyn std::error::Error>> {
    let subscriber = tracing_subscriber::fmt()
        .compact()
        .with_file(true)
        .with_line_number(true)
        .with_thread_ids(true)
        .with_target(true)
        .finish();
    tracing::subscriber::set_global_default(subscriber)?;

    // let matches = Args::parse();
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
    let (update_q_sender, update_q_receiver) = kanal::bounded_async::<Box<dyn EventSource<Event = Pool>>>(1000);
    // routes holds the routes that pass through an updated pool
    // this will be populated by the graph module when there is an updated pool
    let (routes_sender, mut routes_receiver) =
        kanal::bounded_async::<Order>(10000);
    // we start the sync service first to load the pools first
    let sync_config = SyncConfig {
        providers: PROVIDERS.clone(),
    };
    garb_sync_aptos::start(pools.clone(), update_q_sender, sync_config)
        .await
        .unwrap();
    
    let mut joins = vec![];
    
    joins.push(std::thread::spawn(move || {
        let mut rt = Runtime::new().unwrap();
       
        rt.block_on(async move {
            
            garb_graph_aptos::start(pools.clone(), update_q_receiver, Arc::new(RwLock::new(routes_sender))).await;
    
        });
    }));
    joins.push(std::thread::spawn(move || {
        let mut rt = Runtime::new().unwrap();
        rt.block_on(async move {
            transactor(&mut routes_receiver).await;
    
        });
    }));
    for join in joins {
        join.join().unwrap()
    }
    Ok(())
}

// Transactor
// routes: the updated paths compiled by the graph task
//         it is updated every time there is an event on a pool on chain
//         we will spam the network with transaction simulations and pick the best one
pub async fn transactor(routes: &mut kanal::AsyncReceiver<Order>) {
    let seq_number = Arc::new(tokio::sync::RwLock::new(22_u64));
    let gas_unit_price = Arc::new(tokio::sync::RwLock::new(100_u64));
    
    
    let mut join_handles = vec![];
    let seq_num = seq_number.clone();
    let gas_price = gas_unit_price.clone();
    join_handles.push(tokio::spawn(async move {
        // keep updating seq_number
    
        loop {
            let aptos_client = Client::new_with_timeout(NODE_URLS.first().unwrap().clone(), Duration::from_secs(45));
            if let Ok(account) = aptos_client.get_account(KEY.address()).await {
                let seq = account.inner().sequence_number;
                let mut w = seq_num.write().await;
                *w = seq ;
                std::mem::drop(w);
            }
            tokio::time::sleep(Duration::from_secs(5)).await;
        }
    }));
    join_handles.push(tokio::spawn(async move {
        // keep updating gas unit price
        
        loop {
            let aptos_client = Client::new_with_timeout(NODE_URLS.first().unwrap().clone(), Duration::from_secs(45));
            if let Ok(account) = aptos_client.estimate_gas_price().await {
                let gas_estimate = account.inner().gas_estimate;
                let mut w = gas_price.write().await;
                *w = gas_estimate ;
                std::mem::drop(w);
            }
            tokio::time::sleep(Duration::from_secs(100)).await;
        }
    }));
   
   
    let par_requests = Arc::new(tokio::sync::Semaphore::new(*PARALLEL_REQUESTS));
    let total_tasks = Arc::new(tokio::sync::RwLock::new(0_u64));
    let active_tasks = Arc::new(tokio::sync::RwLock::new(0_u64));
    let t = total_tasks.clone();
    let a = active_tasks.clone();
    let p = par_requests.clone();
    join_handles.push(tokio::spawn(async move {
        // print info
        loop {
            tokio::time::sleep(Duration::from_secs(30)).await;
            let tasks = t.read().await;
            println!("total_tasks: {}, waiting_tasks: {}, working_tasks: {}/{}", tasks, a.read().await,*PARALLEL_REQUESTS - p.available_permits(), *PARALLEL_REQUESTS);
        }
    }));
    
    while let Ok(order) = routes.try_recv() {
        
        if let Some(order) = order {
            let mut w = total_tasks.write().await;
            *w += 1;
            std::mem::drop(w);
            if order.route.len() <= 0 {
                continue;
            }
            let mut w = active_tasks.write().await;
            *w += 1;
            std::mem::drop(w);
            let sequence_number = seq_number.clone();
            let gas_unit_price = gas_unit_price.clone();
            let requests = par_requests.clone();
            let active_tasks = active_tasks.clone();
            join_handles.push(tokio::spawn(async move {
            
                // build the transaction
                let first_pool = order.route.first().unwrap();
                let second_pool = order.route.get(1).unwrap();
                let third_pool = order.route.get(2);
                let mut function = "two_step_route";
                let mut type_tags: Vec<TypeTag> = vec![];
                let mut args = vec![];
                args.push(
                    (&(first_pool.clone().provider as u8))
                          .to_le_bytes()
                          .to_vec(),
                );
                args.push(0_u64.to_le_bytes().to_vec());
                args.push(1_u8.to_le_bytes().to_vec());
            
                if first_pool.x_to_y {
                    type_tags.push(TypeTag::Struct(Box::new(
                        StructTag::from_str(&first_pool.x_address).unwrap(),
                    )));
                    type_tags.push(TypeTag::Struct(Box::new(
                        StructTag::from_str(&first_pool.y_address).unwrap(),
                    )));
                } else {
                    type_tags.push(TypeTag::Struct(Box::new(
                        StructTag::from_str(&first_pool.y_address).unwrap(),
                    )));
                    type_tags.push(TypeTag::Struct(Box::new(
                        StructTag::from_str(&first_pool.x_address).unwrap(),
                    )));
                }
                args.push(
                    (&(second_pool.clone().provider as u8))
                          .to_le_bytes()
                          .to_vec(),
                );
                args.push(0_u64.to_le_bytes().to_vec());
            
                if second_pool.y_address == first_pool.x_address
                      || second_pool.y_address == first_pool.y_address
                {
                    args.push(0_u8.to_le_bytes().to_vec());
                    type_tags.push(TypeTag::Struct(Box::new(
                        StructTag::from_str(&second_pool.x_address).unwrap(),
                    )));
                } else {
                    args.push(1_u8.to_le_bytes().to_vec());
                    type_tags.push(TypeTag::Struct(Box::new(
                        StructTag::from_str(&second_pool.y_address).unwrap(),
                    )));
                }
            
                if let Some(pool) = third_pool.clone() {
                    args.push((&(pool.clone().provider as u8)).to_le_bytes().to_vec());
                    args.push(0_u64.to_le_bytes().to_vec());
                
                    if pool.y_address == second_pool.x_address
                          || pool.y_address == second_pool.y_address
                    {
                        args.push(0_u8.to_le_bytes().to_vec());
                        type_tags.push(TypeTag::Struct(Box::new(
                            StructTag::from_str(&pool.x_address).unwrap(),
                        )));
                    } else {
                        args.push(1_u8.to_le_bytes().to_vec());
                        type_tags.push(TypeTag::Struct(Box::new(
                            StructTag::from_str(&pool.y_address).unwrap(),
                        )));
                    }
                    type_tags.push(TypeTag::Struct(Box::new(
                        StructTag::from_str(if first_pool.x_to_y {
                            &first_pool.x_address
                        } else {
                            &first_pool.y_address
                        })
                              .unwrap(),
                    )));
                    function = "three_step_route"
                }
            
                type_tags.push(TypeTag::Struct(Box::new(
                    StructTag::from_str(if first_pool.x_to_y {
                        &first_pool.x_address
                    } else {
                        &first_pool.y_address
                    })
                          .unwrap(),
                )));
                type_tags.push(TypeTag::Struct(Box::new(
                    StructTag::from_str(if first_pool.x_to_y {
                        &first_pool.x_address
                    } else {
                        &first_pool.y_address
                    })
                          .unwrap(),
                )));
            
                if first_pool.curve.is_some() {
                    let first_extra_index = if function == "two_step_route" { 3 } else { 4 };
                    std::mem::replace(
                        &mut type_tags[first_extra_index],
                        TypeTag::Struct(Box::new(
                            StructTag::from_str(&first_pool.curve.as_ref().unwrap()).unwrap(),
                        )),
                    );
                }
                if second_pool.curve.is_some() {
                    let second_extra_index = if function == "two_step_route" { 4 } else { 5 };
                    std::mem::replace(
                        &mut type_tags[second_extra_index],
                        TypeTag::Struct(Box::new(
                            StructTag::from_str(&second_pool.curve.as_ref().unwrap()).unwrap(),
                        )),
                    );
                }
                if third_pool.is_some() {
                    let third_pool = third_pool.unwrap();
                    if third_pool.curve.is_some() {
                        std::mem::replace(
                            &mut type_tags[6],
                            TypeTag::Struct(Box::new(
                                StructTag::from_str(&third_pool.curve.as_ref().unwrap()).unwrap(),
                            )),
                        );
                    }
                }
            
                // TODO: get price of tokens for non APT arbs
                let max_gas_units = MAX_GAS_UNITS.clone();
            
                let decimals = order.decimals;
                args.push((order.size * 10_u64.pow(decimals as u32)).to_le_bytes().to_vec());
            
                let tx_f =
                      aptos_sdk::transaction_builder::TransactionFactory::new(ChainId::new(1_u8));
                let aptos_client =
                      Client::new_with_timeout(NODE_URLS.get(0).unwrap().clone(), Duration::from_secs(20));
                // simulate and try when it works
                'sim_loop: for _ in 0..5 {
                    let permit = requests.acquire().await.unwrap();
                
                
                    let gas_unit = gas_unit_price.read().await;
                    let mut args = args.clone();
                    args.push(
                        (order.size * 10_u64.pow(decimals as u32) + (max_gas_units * *gas_unit))
                              .to_le_bytes()
                              .to_vec(),
                    );
                    std::mem::drop(gas_unit);
                    let seq_num = sequence_number.read().await;
                    let tx = tx_f.payload(TransactionPayload::EntryFunction(
                        EntryFunction::new(
                            ModuleId::new(AccountAddress::from_str("0x89576037b3cc0b89645ea393a47787bb348272c76d6941c574b053672b848039").unwrap(), Identifier::new("aggregator").unwrap()),
                            Identifier::new(function).unwrap(),
                            type_tags.clone(),
                            args
                        )
                    ))
                                 .sender(KEY.address())
                                 .sequence_number(*seq_num)
                                 .max_gas_amount(max_gas_units)
                                 .build();
                    std::mem::drop(seq_num);
                    // println!("tx: {:?}", tx);
                    let signed_tx = KEY.sign_transaction(tx);
                    let sim_result = aptos_client
                          .simulate_bcs_with_gas_estimation(&signed_tx, true, true)
                          .await;
                    if let Ok(result) = sim_result {
                        std::mem::drop(permit);
                    
                        match result.into_inner().info {
                            TransactionInfo::V0(info) => {
                                match info.status() {
                                    ExecutionStatus::Success => {
                                        if info.gas_used() > max_gas_units {
                                            continue 'sim_loop;
                                        }
                                    
                                        // send the transaction
                                        let result = aptos_client.submit_bcs(&signed_tx).await;
                                        if let Ok(result) = result {
                                            println!("`````````````````````` Tried Route ``````````````````````");
                                            for (i, pool) in order.route.iter().enumerate() {
                                                println!("{}. {}", i + 1, pool);
                                            }
                                            println!("\n\n");
                                            let mut seq_num = sequence_number.write().await;
                                            *seq_num += 1;
                                            println!("seq_num: {:?}", seq_num);
                                        } else {
                                            println!("error: {:?}", result.unwrap_err());
                                        }
                                    }
                                    ExecutionStatus::OutOfGas => {}
                                    ExecutionStatus::MoveAbort {
                                        location,
                                        code,
                                        info,
                                    } => {
    
                                        if let Some(info) = info {
                                            if info.reason_name == "ECOIN_STORE_NOT_PUBLISHED" {
                                                for (i, pool) in order.route.iter().enumerate() {
                                                    println!("{}. {}", i + 1, pool);
                                                }
                                                println!("\n\n");
                                                continue 'sim_loop;
                                            }
                                            if info.reason_name == "E_OUTPUT_LESS_THAN_MINIMUM" || info.reason_name == "ERROR_INSUFFICIENT_OUTPUT_AMOUNT" || info.reason_name == "ERR_INSUFFICIENT_OUTPUT_AMOUNT" {
                                                continue 'sim_loop;
                                            }
                                            if info.reason_name == "EINSUFFICIENT_LIQUIDITY" || info.reason_name == "EPOOL_NOT_FOUND" || info.reason_name == "E_PAIR_NOT_CREATED" || info.reason_name == "ERR_INCORRECT_SWAP" || info.reason_name == "ERR_COIN_TYPE_SAME_ERROR" {
                                                break 'sim_loop;
                                            }
                                        }
                                        println!("MoveAbort: {:?} {:?} {:?}", location, code, info);
    
                                    }
                                    ExecutionStatus::ExecutionFailure {
                                        location,
                                        function,
                                        code_offset,
                                    } => {
                                        if (*function == 67 && *code_offset == 10) || (*function == 63 && *code_offset == 10) || (*function == 44 && *code_offset == 39) {
                                            break 'sim_loop;
                                        }
                                        println!(
                                            "ExecutionFailure: {:?} {:?} {:?}",
                                            location, function, code_offset
                                        );
                                    }
                                    ExecutionStatus::MiscellaneousError(val) => {
                                        if let Some(code) = val {
                                            if *code == StatusCode::SEQUENCE_NUMBER_TOO_NEW || *code == StatusCode::SEQUENCE_NUMBER_TOO_OLD {
                                                continue 'sim_loop;
                                            }
                                            println!("MiscellaneousError: {:?}", code);
                                        }
                                    }
                                }
                            }
                        }
                    } else {
                        std::mem::drop(permit);
                        eprintln!("sim_result: {:?} {}", sim_result.unwrap_err(), NODE_URLS.get(0).unwrap().clone());
                    }
                }
                let mut w = active_tasks.write().await;
                *w -= 1;
                std::mem::drop(w);
            }));
        }
    }
    for task in join_handles {
        task.await.unwrap();
    }
}

// this is going to be too slow, find a better way to do this
