#![allow(where_clauses_object_safety)]
#![allow(unused_must_use)]
mod animeswap;
mod aptoswap;
mod aux;
mod cetue;
mod events;
mod liquidswap;
mod obric;
mod pancakeswap;
mod types;

use crate::animeswap::AnimeswapMetadata;
use crate::aptoswap::AptoswapMetadata;
use crate::aux::AuxMetadata;
use crate::cetue::CetueMetadata;
use crate::events::{EventEmitter};
use crate::liquidswap::LiquidswapMetadata;
use crate::obric::ObricMetadata;
use crate::pancakeswap::PancakeSwapMetadata;
use async_std::sync::Arc;
use async_trait::async_trait;
use once_cell::sync::Lazy;
use std::collections::HashMap;
use std::fmt::Display;
use std::hash::Hash;
use std::str::FromStr;
use tokio::task::JoinHandle;
use url::Url;

static NODE_URL: Lazy<Url> = Lazy::new(|| {
    Url::from_str(
        std::env::var("APTOS_NODE_URL")
              .as_ref()
              .map(|s| s.as_str())
              .unwrap_or("https://aptos-mainnet.pontem.network/"),
    )
          .unwrap()
});

static KNOWN_STABLECOINS: Lazy<Vec<(&str, usize)>> = Lazy::new(|| {
    vec![
        ("0x8d87a65ba30e09357fa2edea2c80dbac296e5dec2b18287113500b902942929d::celer_coin_manager::BusdCoin",8), // celer BUSD
        ("0xf22bede237a07e121b56d91a491eb7bcdfd1f5907926a9e58338f964a01b17fa::asset::USDC", 6), // layer zero USDC
        ("0x5e156f1207d0ebfa19a9eeff00d62a282278fb8719f4fab3a586a0a2c0fffbea::coin::T", 6), // wormhole USDC
        ("0x8d87a65ba30e09357fa2edea2c80dbac296e5dec2b18287113500b902942929d::celer_coin_manager::UsdcCoin", 6), //celer USDC
        ("0xf22bede237a07e121b56d91a491eb7bcdfd1f5907926a9e58338f964a01b17fa::asset::USDT",6), // layerzero USDT
        ("0xa2eda21a58856fda86451436513b867c97eecb4ba099da5775520e0f7492e852::coin::T", 6), // wormhole USDT
        ("0x8d87a65ba30e09357fa2edea2c80dbac296e5dec2b18287113500b902942929d::celer_coin_manager::UsdtCoin", 6), // celer USDT
        ("0x8d87a65ba30e09357fa2edea2c80dbac296e5dec2b18287113500b902942929d::celer_coin_manager::DaiCoin", 8), // celer DAI
        ("0x1000000fa32d122c18a6a31c009ce5e71674f22d06a581bb0a15575e6addadcc::usda::USDA", 6), // partners USDA
    ]
});

pub type BoxedLiquidityProvider = Box<
    dyn LiquidityProvider<Metadata = Box<dyn Meta>, EventType = Box<dyn EventSource<Event = Pool>>>
    + Send,
>;

pub trait Meta {}

pub trait EventSource: Send {
    type Event: Send;
    fn get_event(&self) -> Self::Event;
}
pub trait Calculator {
    fn calculate_out(&self, in_: u64, pool: &Pool) -> u64;
}
pub struct CpmmCalculator {}

#[async_trait]
pub trait LiquidityProvider: EventEmitter {
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
        let numerator = amount_in_with_fee
            .checked_mul(swap_destination_amount as u128)
            .unwrap_or(0);
        let denominator = ((swap_source_amount as u128) * 10000) + amount_in_with_fee;
        (numerator / denominator) as u64
    }
}

impl LiquidityProviders {
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
                    contract_address:
                        "0xbd35135844473187163ca197ca93b2ab014370587bb0ed3befff9e902d6bb541"
                            .to_string(),
                    pool_module: String::from("amm"),
                    pool_name: String::from("Pool"),
                };
                Box::new(aux::Aux::new(metadata))
            }
            LiquidityProviders::Obric => {
                let metadata = ObricMetadata {
                    contract_address:
                        "0xc7ea756470f72ae761b7986e4ed6fd409aad183b1b2d3d2f674d979852f45c4b"
                            .to_string(),
                    pool_module: String::from("piece_swap"),
                    pool_name: String::from("PieceSwapPoolInfo"),
                };
                Box::new(obric::Obric::new(metadata))
            }
            LiquidityProviders::Cetue => {
                let metadata = CetueMetadata {
                    contract_address:
                        "0xec42a352cc65eca17a9fa85d0fc602295897ed6b8b8af6a6c79ef490eb8f9eba"
                            .to_string(),
                    pool_module: String::from("amm_swap"),
                    pool_name: String::from("Pool"),
                };
                Box::new(cetue::Cetue::new(metadata))
            }
            LiquidityProviders::PancakeSwap => {
                let metadata = PancakeSwapMetadata {
                    contract_address:
                        "0xc7efb4076dbe143cbcd98cfaaa929ecfc8f299203dfff63b95ccb6bfe19850fa"
                            .to_string(),
                    pool_module: String::from("swap"),
                    pool_name: String::from("TokenPairMetadata"),
                };
                Box::new(pancakeswap::PancakeSwap::new(metadata))
            }
            LiquidityProviders::Aptoswap => {
                let metadata = AptoswapMetadata {
                    contract_address:
                        "0xa5d3ac4d429052674ed38adc62d010e52d7c24ca159194d17ddc196ddb7e480b"
                            .to_string(),
                    pool_module: String::from("pool"),
                    pool_name: String::from("Pool"),
                };
                Box::new(aptoswap::Aptoswap::new(metadata))
            }
            LiquidityProviders::AnimeSwap => {
                let metadata = AnimeswapMetadata {
                    contract_address:
                        "0x796900ebe1a1a54ff9e932f19c548f5c1af5c6e7d34965857ac2f7b1d1ab2cbf"
                            .to_string(),
                    pool_module: String::from("AnimeSwapPoolV1"),
                    pool_name: String::from("LiquidityPool"),
                };
                Box::new(animeswap::Animeswap::new(metadata))
            }
            LiquidityProviders::LiquidSwap => {
                let metadata = LiquidswapMetadata {
                    contract_address:
                        "0x05a97986a9d031c4567e15b797be516910cfcb4156312482efc6a19c0a30c948"
                            .to_string(),
                    pool_module: String::from("liquidity_pool"),
                    pool_name: String::from("LiquidityPool"),
                };
                Box::new(liquidswap::Liquidswap::new(metadata))
            }
            _ => panic!("Invalid liquidity provider"),
        }
    }
}

#[derive(Debug, Clone, Hash, Eq, PartialEq)]
pub enum Curve {
    Uncorrelated,
    Stable
}

#[derive(Debug, Clone, Hash, Eq, PartialEq)]
pub struct Pool {
    pub address: String,
    pub x_address: String,
    pub y_address: String,
    pub curve: Option<String>,
    pub curve_type: Curve,
    pub fee_bps: u64,
    pub x_amount: u64,
    pub y_amount: u64,
    pub x_to_y: bool,
    pub provider: LiquidityProviders,
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
            curve_type: pool.curve_type.clone(),
            x_amount: pool.x_amount,
            y_amount: pool.y_amount,
            x_to_y: pool.x_to_y,
            provider: pool.provider.clone(),
            fee_bps: pool.fee_bps,
        }
    }
}

impl Display for Pool {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "Pool {{ \n\tProvider: {:?}\n\taddress: {}\n\tx_address: {}\n\ty_address: {}\n\tis_x_to_y: {}\n\t", self.provider, self.address,self.x_address, self.y_address,  self.x_to_y)
    }
}

pub struct SyncConfig {
    pub providers: Vec<LiquidityProviders>,
}



pub async fn start(
    pools: Arc<tokio::sync::RwLock<HashMap<String, Pool>>>,
    updated_q: kanal::AsyncSender<Box<dyn EventSource<Event = Pool>>>,
    config: SyncConfig,
) -> anyhow::Result<tokio::task::JoinHandle<()>> {
    
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
