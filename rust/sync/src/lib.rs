mod events;
mod aux;
mod obric;
mod pancakeswap;
mod cetue;
mod aptoswap;
mod types;

use std::collections::HashMap;
use std::fmt::Display;
use spl_math::checked_ceil_div::CheckedCeilDiv;
use num_traits::cast::ToPrimitive;
use std::hash::Hash;
use std::str::FromStr;
use std::time::Duration;
use async_trait::async_trait;
use aptos_sdk::move_types::language_storage::StructTag;
use aptos_sdk::move_types::language_storage::TypeTag;
use aptos_sdk::rest_client::Client;
use aptos_sdk::types::account_address::AccountAddress;
use aptos_sdk::types::event::EventKey;
use async_std::sync::Arc;
use once_cell::sync::Lazy;
use serde::{Deserialize, Serialize};
use serde_json::Value;
use tokio::task::JoinHandle;
use url::Url;
use crate::aux::AuxMetadata;
use crate::obric::ObricMetadata;
use crate::cetue::CetueMetadata;
use crate::aptoswap::AptoswapMetadata;
use crate::pancakeswap::PancakeSwapMetadata;
use crate::events::{EventEmitter, EventSink};

const OBRIC_CONTRACT: &str = "0xc7ea756470f72ae761b7986e4ed6fd409aad183b1b2d3d2f674d979852f45c4b";
const PANCAKESWAP_CONTRACT: &str =
    "0xc7efb4076dbe143cbcd98cfaaa929ecfc8f299203dfff63b95ccb6bfe19850fa";
const CETUE_CONTRACT: &str =
    "0xec42a352cc65eca17a9fa85d0fc602295897ed6b8b8af6a6c79ef490eb8f9eba";
const APTOSWAP_CONTRACT: &str =
    "0xa5d3ac4d429052674ed38adc62d010e52d7c24ca159194d17ddc196ddb7e480b";
const AUX_CONTRACT: &str =
    "0xbd35135844473187163ca197ca93b2ab014370587bb0ed3befff9e902d6bb541";
const LIQUIDSWAP_CONTRACT: &str =
    "0x190d44266241744264b964a37b8f09863167a12d3e70cda39376cfb4e3561e12";
const LIQUIDSWAP_RESOURCE: &str =
    "0x05a97986a9d031c4567e15b797be516910cfcb4156312482efc6a19c0a30c948";
const ANIMESWAP_CONTRACT: &str =
    "0x16fe2df00ea7dde4a63409201f7f4e536bde7bb7335526a35d05111e68aa322c";
const ANIMESWAP_RESOURCE: &str =
    "0x796900ebe1a1a54ff9e932f19c548f5c1af5c6e7d34965857ac2f7b1d1ab2cbf";

// #[derive(Clone, Debug, Hash, PartialEq, Eq)]
// pub struct LiquidityProvider {
//     pub contract_address: String,
//     pub resource_address: Option<String>,
//     pub id: LiquidityProviders,
//     pub pool_module: String,
//     pub pool_name: String,
//     pub events_module: Option<String>,
//     pub events_name: Option<String>,
//     pub reserve_module: Option<String>,
//     pub reserve_name: Option<String>,
//     pub event_has_types: bool,
// }
pub trait Meta {

}

pub trait EventSource: Send {
    type Event: Send ;
    fn get_event(&self) -> Self::Event;
}

#[async_trait]
pub trait LiquidityProvider: EventEmitter {
    // const ID: LiquidityProviders;
    type Metadata;
    fn get_metadata(&self) -> Self::Metadata;
    async fn get_pools(&self) -> HashMap<String, Pool>;
    fn load_pools(&self) -> JoinHandle<()>;
    fn get_id(&self) -> LiquidityProviders;
}


#[derive(Debug, Clone, PartialOrd, PartialEq, Eq, Hash)]
#[repr(u8)]
pub enum LiquidityProviders {
    Hippo = 1,
    Econia = 2,
    LiquidSwap = 3,
    Basiq = 4,
    Ditto = 5,
    Tortuga = 6,
    Aptoswap = 7,
    Aux = 8,
    AnimeSwap = 9,
    Cetue = 10,
    PancakeSwap = 11,
    Obric = 12,
}

impl From<u8> for LiquidityProviders {
    fn from(value: u8) -> Self {
        match value {
            1 => LiquidityProviders::Hippo,
            2 => LiquidityProviders::Econia,
            3 => LiquidityProviders::LiquidSwap,
            4 => LiquidityProviders::Basiq,
            5 => LiquidityProviders::Ditto,
            6 => LiquidityProviders::Tortuga,
            7 => LiquidityProviders::Aptoswap,
            8 => LiquidityProviders::Aux,
            9 => LiquidityProviders::AnimeSwap,
            10 => LiquidityProviders::Cetue,
            11 => LiquidityProviders::PancakeSwap,
            12 => LiquidityProviders::Obric,
            _ => panic!("Invalid liquidity provider"),
        }
    }
}
pub type BoxedLiquidityProvider = Box<dyn LiquidityProvider<Metadata = Box<dyn Meta>, EventType = Box<dyn EventSource<Event = Pool>>> + Send>;


pub trait Calculator {
    fn calculate_out(&self,in_: u64, pool: &Pool) -> u64;
}

pub struct CpmmCalculator {

}

impl CpmmCalculator {
    fn new() -> Self {
        Self {}
    }
}


impl Calculator for CpmmCalculator {
    fn calculate_out(&self, in_: u64, pool: &Pool) -> u64 {
        let swap_source_amount = if pool.x_to_y {
            pool.x_amount
        } else {
            pool.y_amount
        };
        let swap_destination_amount = if pool.x_to_y {
            pool.y_amount
        } else {
            pool.x_amount
        };
        if in_ >= swap_source_amount {
            return swap_destination_amount;
        }
        let amount_in_with_fee = (in_ as u128) * ((10000 - pool.fee_bps) as u128);
        let numerator = amount_in_with_fee.checked_mul(swap_destination_amount as u128).unwrap_or(0);
        let denominator = ((swap_source_amount as u128) * 10000) + amount_in_with_fee;
        ((numerator / denominator) as u64)
    }
}



impl LiquidityProviders  {
    pub fn build_calculator(&self) -> Box<dyn Calculator> {
        match *self {
            LiquidityProviders::LiquidSwap => Box::new(CpmmCalculator::new()),
            LiquidityProviders::Aptoswap => Box::new(CpmmCalculator::new()),
            LiquidityProviders::Aux => Box::new(CpmmCalculator::new()),
            LiquidityProviders::AnimeSwap => Box::new(CpmmCalculator::new()),
            LiquidityProviders::Cetue => Box::new(CpmmCalculator::new()),
            LiquidityProviders::PancakeSwap => Box::new(CpmmCalculator::new()),
            LiquidityProviders::Obric => Box::new(CpmmCalculator::new()),
            _ => panic!("Invalid liquidity provider"),
        }
    }
    pub fn build(&self) -> BoxedLiquidityProvider {
        match *self {
            LiquidityProviders::Aux => {
                let metadata = AuxMetadata {
                    contract_address: "0xbd35135844473187163ca197ca93b2ab014370587bb0ed3befff9e902d6bb541".to_string(),
                    pool_module: String::from("amm"),
                    pool_name: String::from("Pool"),
                };
                Box::new(aux::Aux::new(metadata))
            }
            LiquidityProviders::Obric => {
                let metadata = ObricMetadata {
                    contract_address: "0xc7ea756470f72ae761b7986e4ed6fd409aad183b1b2d3d2f674d979852f45c4b".to_string(),
                    pool_module: String::from("piece_swap"),
                    pool_name: String::from("PieceSwapPoolInfo"),
                };
                Box::new(obric::Obric::new(metadata))
            }
            LiquidityProviders::Cetue => {
                let metadata = CetueMetadata {
                    contract_address: "0xec42a352cc65eca17a9fa85d0fc602295897ed6b8b8af6a6c79ef490eb8f9eba".to_string(),
                    pool_module: String::from("amm_swap"),
                    pool_name: String::from("Pool"),
                };
                Box::new(cetue::Cetue::new(metadata))
            }
            LiquidityProviders::PancakeSwap => {
                let metadata = PancakeSwapMetadata {
                    contract_address: "0xc7efb4076dbe143cbcd98cfaaa929ecfc8f299203dfff63b95ccb6bfe19850fa".to_string(),
                    pool_module: String::from("swap"),
                    pool_name: String::from("TokenPairMetadata"),
                };
                Box::new(pancakeswap::PancakeSwap::new(metadata))
            }
            LiquidityProviders::Aptoswap => {
                let metadata = AptoswapMetadata {
                    contract_address: "0xa5d3ac4d429052674ed38adc62d010e52d7c24ca159194d17ddc196ddb7e480b".to_string(),
                    pool_module: String::from("pool"),
                    pool_name: String::from("Pool"),
                };
                Box::new(aptoswap::Aptoswap::new(metadata))
            }
            _ => panic!("Invalid liquidity provider"),
        }
    }
}

#[derive(Debug, Clone, Hash, Eq, PartialEq)]
pub struct Pool {
    pub address: String,
    pub x_address: String,
    pub y_address: String,
    pub curve: Option<String>,
    pub fee_bps: u64,
    pub x_amount: u64,
    pub y_amount: u64,
    pub events_sources: Vec<IEventHandle>,
    pub x_to_y: bool,
    pub provider: LiquidityProviders
}

impl Pool {
    fn total_events(&self) -> usize {
        self.events_sources.len()
    }
}

impl EventSource for Pool {
    type Event = Self;
    fn get_event(&self) -> Self::Event {
        self.clone()
    }
}

impl From<&Pool> for Pool {
    fn from(pool: &Pool) -> Self {
        Self {
            address: pool.address.clone(),
            x_address: pool.x_address.clone(),
            y_address: pool.y_address.clone(),
            curve: pool.curve.clone(),
            x_amount: pool.x_amount,
            y_amount: pool.y_amount,
            events_sources: pool.events_sources.clone(),
            x_to_y: pool.x_to_y,
            provider: pool.provider.clone(),
            fee_bps: pool.fee_bps,
        }
    }
}

impl Display for Pool {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "Pool {{ \n\tProvider: {:?}\n\taddress: {}\n\tx_address: {}\n\ty_address: {}\n\tis_x_to_y: {}\n\tevents_sources: {:?}\n }}\n", self.provider, self.address,self.x_address, self.y_address,  self.x_to_y,self.events_sources.len())
    }
}
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq, Hash)]
pub struct EventHandle {
    pub key: EventKey,
    pub count: u64,
}

impl EventHandle {
    pub fn new(key: EventKey, count: u64) -> Self {
        Self { key, count }
    }
}

pub struct SyncConfig {
    pub providers: Vec<LiquidityProviders>
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq, Hash)]
pub struct IEventHandle {
    pub handle: EventHandle,
    pub struct_tag: String,
    pub field_name: String,
}
static NODE_URL: Lazy<Url> = Lazy::new(|| {
    Url::from_str(
        std::env::var("APTOS_NODE_URL")
            .as_ref()
            .map(|s| s.as_str())
            .unwrap_or("https://aptos-mainnet.pontem.network/"),
    )
    .unwrap()
});

// We can comment out ones with less liquidity to make routing faster
// static PROVIDERS: Lazy<Vec<LiquidityProvider>> = Lazy::new(|| {
//     vec![
//         LiquidityProvider {
//             contract_address: String::from(PANCAKESWAP_CONTRACT),
//             resource_address: None,
//             id: LiquidityProviders::PancakeSwap,
//             pool_module: String::from("swap"),
//             pool_name: String::from("TokenPairMetadata"),
//             events_module: Some(String::from("swap")),
//             events_name: Some(String::from("PairEventHolder")),
//             event_has_types: true,
//         },
//         LiquidityProvider {
//             contract_address: String::from(OBRIC_CONTRACT),
//             resource_address: None,
//             id: LiquidityProviders::Obric,
//             pool_module: String::from("piece_swap"),
//             pool_name: String::from("PieceSwapPoolInfo"),
//             events_module: None,
//             events_name: None,
//             event_has_types: true,
//         },
//         LiquidityProvider {
//             contract_address: String::from(CETUE_CONTRACT),
//             resource_address: None,
//             id: LiquidityProviders::Aptoswap,
//             pool_module: String::from("amm_swap"),
//             pool_name: String::from("Pool"),
//             events_module: Some(String::from("amm_swap")),
//             events_name: Some(String::from("PoolSwapEventHandle")),
//             event_has_types: false,
//         },
//         LiquidityProvider {
//             contract_address: String::from(APTOSWAP_CONTRACT),
//             resource_address: None,
//             id: LiquidityProviders::Aptoswap,
//             pool_module: String::from("pool"),
//             pool_name: String::from("Pool"),
//             events_module: None,
//             events_name: None,
//             event_has_types: true,
//         },
//         LiquidityProvider {
//             contract_address: String::from(LIQUIDSWAP_CONTRACT),
//             resource_address: Some(String::from(LIQUIDSWAP_RESOURCE)),
//             id: LiquidityProviders::LiquidSwap,
//             pool_module: String::from("liquidity_pool"),
//             pool_name: String::from("LiquidityPool"),
//             events_module: Some(String::from("liquidity_pool")),
//             events_name: Some(String::from("EventsStore")),
//             event_has_types: true,
//         },
//         LiquidityProvider {
//             contract_address: String::from(AUX_CONTRACT),
//             resource_address: None,
//             id: LiquidityProviders::Aux,
//             pool_module: String::from("amm"),
//             pool_name: String::from("Pool"),
//             events_module: None,
//             events_name: None,
//             event_has_types: true,
//         },
//         LiquidityProvider {
//             contract_address: String::from(ANIMESWAP_CONTRACT),
//             resource_address: Some(String::from(ANIMESWAP_RESOURCE)),
//             id: LiquidityProviders::AnimeSwap,
//             pool_module: String::from("AnimeSwapPoolV1"),
//             pool_name: String::from("LiquidityPool"),
//             events_module: Some(String::from("AnimeSwapPoolV1")),
//             events_name: Some(String::from("Events")),
//             event_has_types: true,
//         },
//     ]
// });

pub async fn start(
    pools: Arc<tokio::sync::RwLock<HashMap<String, Pool>>>,
    updated_q: kanal::AsyncSender<Box<dyn EventSource<Event = Pool>>>,
    config: SyncConfig,
) -> anyhow::Result<tokio::task::JoinHandle<()>> {
    let aptos_client = Arc::new(Client::new_with_timeout(
        NODE_URL.clone(),
        Duration::from_secs(45),
    ));
    let mut join_handles = vec![];
    let mut amms = vec![];
    for provider in config.providers {
        let mut amm = provider.build();
        join_handles.push(amm.load_pools());
        amm.subscribe(updated_q.clone()).await;
        amms.push(amm);
    
    }

    // Make sure pools are loaded before starting the listener
    println!("Loading Pools...");
    for handle in join_handles {
        handle.await?;
    }
    let mut loaded_pools = HashMap::new();
    for amm in &amms {
        let pools = amm.get_pools().await;
        for (addr, pool) in pools {
            loaded_pools.insert(addr, pool);
        }
    }
    let mut pools = pools.write().await;
    *pools = loaded_pools;
    std::mem::drop(pools);
    let mut emitters = vec![];
    for amm in &amms {
        let emitter = amm.emit();
        emitters.push(emitter);
    }
    
    Ok(tokio::spawn(async move {
        for emitter in emitters {
            emitter.join().unwrap();
        }
    }))
}
//
// async fn poll_events(
//     pools: Arc<tokio::sync::RwLock<HashMap<String, Pool>>>,
//     updated_q: Arc<kanal::AsyncSender<Pool>>,
//     aptos_client: Arc<Client>,
// ) {
//     let pr = pools.read().await;
//     let mut total_events = 0;
//     let mut event_listener_tasks = vec![];
//     for (_, p) in pr.iter() {
//         let pool = p.clone();
//         total_events += pool.total_events();
//         for event in pool.events_sources.into_iter() {
//             let provider = pool.provider.clone();
//             let resources_address = if provider.resource_address.is_some() {
//                 String::from(provider.resource_address.as_ref().unwrap())
//             } else {
//                 provider.contract_address.clone()
//             };
//             let client = aptos_client.clone();
//             let mut last_sequence_number: Option<u64> = None;
//             let update_q = updated_q.clone();
//             let pl = p.clone();
//             event_listener_tasks.push(tokio::spawn(async move {
//                 loop {
//                     let new_event = client
//                         .get_account_events(
//                             AccountAddress::from_str(&resources_address).unwrap(),
//                             &event.struct_tag,
//                             &event.field_name,
//                             last_sequence_number,
//                             Some(1),
//                         )
//                         .await;
//
//                     if let Ok(response) = new_event {
//                         if response.inner().len() > 0 {
//                             let event = response.inner().get(0).unwrap();
//                             if last_sequence_number.is_none() {
//                                 last_sequence_number = Some(event.sequence_number.0 + 1);
//                                 continue;
//                             }
//                             last_sequence_number = Some(event.sequence_number.0 + 1);
//                             update_q.send(pl.clone()).await.unwrap_or(());
//                         }
//                     } else {
//                         // eprintln!("{:?}", new_event.unwrap_err());
//                     }
//                     // TODO: Make this configurable
//                     tokio::time::sleep(Duration::from_secs(5)).await;
//                 }
//             }));
//         }
//     }
//
//     println!("Polling {} event types on {} pools", total_events, pr.len());
//     for task in event_listener_tasks {
//         task.await.unwrap();
//     }
// }
//4.19729386
//4.253248
// //3.015342
// async fn register_provider_events(
//     pools: Arc<tokio::sync::RwLock<HashMap<String, Pool>>>,
//     aptos_client: Arc<Client>,
//     provider: LiquidityProvider,
//     i_events: Vec<&str>,
// ) {
//     let resources_address = if provider.resource_address.is_some() {
//         String::from(provider.resource_address.as_ref().unwrap())
//     } else {
//         provider.contract_address.clone()
//     };
//     let wrapped_resources = aptos_client
//         .get_account_resources(AccountAddress::from_str(&resources_address).unwrap())
//         .await;
//     if let Ok(resources) = wrapped_resources {
//         for resource in resources.inner().into_iter() {
//             match (
//                 &resource.clone().resource_type.module.into_string().as_str(),
//                 &resource.clone().resource_type.name.into_string().as_str(),
//             ) {
//                 (&module, &resource_name) => {
//                     if resource.resource_type.type_params.len() < 2 {
//                         continue;
//                     }
//                     if module != provider.pool_module || resource_name != provider.pool_name {
//                         continue;
//                     }
//
//                     let (coin_x, coin_y) = match (
//                         resource.resource_type.type_params.get(0).unwrap(),
//                         resource.resource_type.type_params.get(1).unwrap(),
//                     ) {
//                         (TypeTag::Struct(struct_tag_x), TypeTag::Struct(struct_tag_y)) => (
//                             join_struct_tag_to_string(struct_tag_x),
//                             join_struct_tag_to_string(struct_tag_y),
//                         ),
//                         _ => continue,
//                     };
//                     println!("{:?}", resources.inner());
//                     let coin_x_resource = resources.inner().clone().into_iter().find(|r| {
//                         r.resource_type.module.clone().into_string().as_str() == "coin"
//                             && r.resource_type.name.clone().into_string().as_str() == "CoinStore"
//                         && r.resource_type.type_params.len() == 1
//                         && r.resource_type.type_params.get(0).unwrap().to_string() == coin_x
//                     }).unwrap();
//
//                     let mut pool = Pool {
//                         address: provider.contract_address.clone()
//                             + "::"
//                             + module
//                             + "::"
//                             + resource_name
//                             + "<"
//                             + coin_x.as_str()
//                             + ", "
//                             + coin_y.as_str()
//                             + ">",
//                         x_address: coin_x.clone(),
//                         y_address: coin_y.clone(),
//                         curve: None,
//                         x_amount: 0,
//                         y_amount: 0,
//                         provider: provider.clone(),
//                         events_sources: vec![],
//                         x_to_y: true,
//                     };
//                     println!("Loading Pool: {} {:?}", pool, coin_x_resource);
//                     // Get the pool's event source from resources
//                     pool.events_sources = if provider.events_module.is_some() {
//                         let resource_type = (&provider).contract_address.clone()
//                             + "::"
//                             + &provider.events_module.as_ref().unwrap()
//                             + "::"
//                             + &provider.events_name.as_ref().unwrap()
//                             + &(if provider.event_has_types {
//                                 vec!["<", coin_x.as_str(), ", ", coin_y.as_str(), ">"].join("")
//                             } else {
//                                 "".to_string()
//                             });
//                         let event_resource = aptos_client
//                             .get_account_resource(
//                                 AccountAddress::from_str(&resources_address).unwrap(),
//                                 &resource_type,
//                             )
//                             .await;
//                         if let Ok(e_resource) = event_resource {
//                             if let Some(e_r) = e_resource.inner() {
//                                 extract_event_handles_from_resource(
//                                     &e_r.data,
//                                     pool.address.clone(),
//                                     &i_events,
//                                 )
//                             } else {
//                                 continue;
//                             }
//                         } else {
//                             continue;
//                         }
//                     } else {
//                         extract_event_handles_from_resource(
//                             &resource.data,
//                             pool.address.clone(),
//                             &i_events,
//                         )
//                     };
//                     let mut w = pools.write().await;
//                     w.insert(pool.address.clone(), pool);
//                 }
//             }
//         }
//     } else {
//         eprintln!("{:?}: {:?}", provider.id, wrapped_resources.unwrap_err());
//     }
// }
// #[derive(Debug, Serialize, Deserialize)]
// #[serde(rename_all = "camelCase")]
// struct ILiquidswapPool {
//     coin_x: String,
//     coin_y: String,
//     curve: String,
//     network_id: usize,
// }
//
// async fn register_liquidswap_events(
//     pools: Arc<tokio::sync::RwLock<HashMap<String, Pool>>>,
//     aptos_client: Arc<Client>,
//     provider: LiquidityProvider,
// ) {
//     let resources_address = if provider.resource_address.is_some() {
//         String::from(provider.resource_address.as_ref().unwrap())
//     } else {
//         provider.contract_address.clone()
//     };
//     let liquidswap_resources = aptos_client
//         .get_account_resources(AccountAddress::from_str(&resources_address).unwrap())
//         .await;
//     let known_pools: Vec<ILiquidswapPool> = serde_json::from_str(
//         &std::fs::read_to_string(std::path::Path::new("./liquidswap_pools.json")).unwrap(),
//     )
//     .unwrap();
//     if let Ok(resources) = liquidswap_resources {
//         for resource in resources.into_inner().into_iter() {
//             match (
//                 &resource.clone().resource_type.module.into_string().as_str(),
//                 &resource.clone().resource_type.name.into_string().as_str(),
//             ) {
//                 (&"liquidity_pool", &"EventsStore") => {
//                     if resource.resource_type.type_params.len() != 3 {
//                         continue;
//                     }
//                     let (coin_x, coin_y, curve) = match (
//                         resource.resource_type.type_params.get(0).unwrap(),
//                         resource.resource_type.type_params.get(1).unwrap(),
//                         resource.resource_type.type_params.get(2).unwrap(),
//                     ) {
//                         (
//                             TypeTag::Struct(struct_tag_x),
//                             TypeTag::Struct(struct_tag_y),
//                             TypeTag::Struct(curve),
//                         ) => (
//                             join_struct_tag_to_string(struct_tag_x),
//                             join_struct_tag_to_string(struct_tag_y),
//                             join_struct_tag_to_string(curve),
//                         ),
//                         _ => continue,
//                     };
//
//                     if !known_pools
//                         .iter()
//                         .any(|pool| pool.coin_x == coin_x && pool.coin_y == coin_y)
//                     {
//                         continue;
//                     }
//
//                     let mut pool = Pool {
//                         address: LIQUIDSWAP_CONTRACT.to_string()
//                             + "::liquidity_pool::EventsStore<"
//                             + coin_x.as_str()
//                             + ", "
//                             + coin_y.as_str()
//                             + ", "
//                             + curve.as_str()
//                             + ">",
//                         x_address: coin_x,
//                         y_address: coin_y,
//                         curve: Some(curve),
//                         x_amount: 0,
//                         y_amount: 0,
//                         provider: provider.clone(),
//                         events_sources: vec![],
//                         x_to_y: true,
//                     };
//                     let i_events = vec![
//                         "flashloan_handle",
//                         "liquidity_added_handle",
//                         "liquidity_removed_handle",
//                         "oracle_updated_handle",
//                         "swap_handle",
//                     ];
//                     pool.events_sources = extract_event_handles_from_resource(
//                         &resource.data,
//                         pool.address.clone(),
//                         &i_events,
//                     );
//                     if pool.events_sources.len() < i_events.len() {
//                         continue;
//                     }
//                     let mut w = pools.write().await;
//
//                     w.insert(pool.address.clone(), pool);
//                 }
//
//                 (&&_, &&_) => {}
//             }
//         }
//     } else {
//         eprintln!("Liquidswap: {:?}", liquidswap_resources.unwrap_err());
//     }
// }

pub(crate) fn join_struct_tag_to_string(struct_tag: &StructTag) -> String {
    vec![
        "0x".to_string(),
        struct_tag.address.to_string(),
        "::".to_string(),
        struct_tag.module.clone().into_string(),
        "::".to_string(),
        struct_tag.name.clone().into_string(),
    ]
    .join("")
}



pub(crate) fn extract_event_handles_from_resource(
    resource_data: &Value,
    event_source: String,
    i_events: &Vec<&str>,
) -> Vec<IEventHandle> {
    //println!("{:?}", resource_data);
    let events: HashMap<String, Option<IEventHandle>> = resource_data
        .as_object()
        .unwrap()
        .iter()
        .map(|(k, v)| {
            if !i_events.iter().any(|i| i == k) {
                return (k.clone(), None);
            }
            let handles: &serde_json::Map<String, Value> = v.as_object().unwrap();
            let count = handles
                .get("counter")
                .unwrap()
                .as_str()
                .unwrap()
                .to_string();
            let guid: &serde_json::Map<String, Value> =
                handles.get("guid").unwrap().as_object().unwrap();
            let id: &serde_json::Map<String, Value> = guid.get("id").unwrap().as_object().unwrap();
            let addr = id.get("addr").unwrap().as_str().unwrap().to_string();
            let creation_num = id
                .get("creation_num")
                .unwrap()
                .as_str()
                .unwrap()
                .to_string();

            (
                k.clone(),
                Some(IEventHandle {
                    handle: EventHandle::new(
                        EventKey::new(
                            creation_num.parse::<u64>().unwrap(),
                            AccountAddress::from_str(&addr).unwrap(),
                        ),
                        count.parse::<u64>().unwrap(),
                    ),
                    field_name: k.clone(),
                    struct_tag: event_source.clone(),
                }),
            )
        })
        .filter(|(_, v)| v.is_some())
        .collect();
    return events.values().map(|v| v.clone().unwrap()).collect();
}
