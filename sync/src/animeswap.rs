use aptos_sdk::rest_client::Client;
use tokio::task::{JoinHandle, LocalSet};
use tokio::sync::{RwLock};
use tokio::runtime::Runtime;
use async_std::sync::Arc;
use std::time::Duration;
use aptos_sdk::types::account_address::AccountAddress;
use std::collections::HashMap;
use kanal::AsyncSender;
use async_trait::async_trait;
use std::str::FromStr;
use aptos_sdk::move_types::language_storage::TypeTag;
use crate::{EventSource, KNOWN_STABLECOINS, LiquidityProvider, LiquidityProviders, Pool};
use crate::Curve::Uncorrelated;
use crate::Meta;
use crate::NODE_URL;
use crate::events::{EventEmitter};
use crate::types::{AnimeswapLiquidityPool};
#[derive(Clone)]
pub struct AnimeswapMetadata {
	pub contract_address: String,
	pub pool_module: String,
	pub pool_name: String,
}

impl Meta for AnimeswapMetadata {

}

pub struct Animeswap {
	pub metadata: AnimeswapMetadata,
	pub pools: Arc<RwLock<HashMap<String, Pool>>>,
	subscribers: Arc<RwLock<Vec<AsyncSender<Box<dyn EventSource<Event = Pool>>>>>>,
}

impl Animeswap {
	pub fn new(metadata: AnimeswapMetadata) -> Self {
		Self {
			metadata,
			pools: Arc::new(RwLock::new(HashMap::new())),
			subscribers: Arc::new(RwLock::new(Vec::new())),
		}
	}
}




impl EventEmitter for Animeswap {
	type EventType = Box<dyn EventSource<Event = Pool>>;
	fn get_subscribers(&self) -> Arc<RwLock<Vec<AsyncSender<Self::EventType>>>> {
		self.subscribers.clone()
	}
	fn emit(&self) -> std::thread::JoinHandle<()> {
		let pools = self.pools.clone();
		let subscribers = self.subscribers.clone();
		std::thread::spawn( move || {
			let mut rt = Runtime::new().unwrap();
			let pools = pools.clone();
			let tasks = LocalSet::new();
			tasks.block_on(&mut rt, async move {
				let mut joins = vec![];
				for pool in pools.read().await.values() {
					let subscribers = subscribers.clone();
					let mut pool = pool.clone();
					joins.push(tokio::task::spawn_local(async move {
						let aptos_client = Client::new_with_timeout(NODE_URL.clone(), Duration::from_secs(10));
						let resource_address = pool.address.split("::").collect::<Vec<&str>>()[0];
						// let mut bench = SystemTime::now();
						let mut value = None;
						loop {
							if let Ok(pool_resource_option) = aptos_client.get_account_resource(AccountAddress::from_str(resource_address).unwrap(), &pool.address).await {
								if let Some(pool_resource) = pool_resource_option.inner() {
									let amm: AnimeswapLiquidityPool = serde_json::from_value(pool_resource.data.clone()).unwrap();
									if value.is_none() {
										value = Some(amm);
										continue;
									}
									if value.as_ref().unwrap().ne(&amm) {
										pool.x_amount = amm.coin_x_reserve.value.0;
										pool.y_amount = amm.coin_y_reserve.value.0;
										value = Some(amm);
										let mut subscribers = subscribers.write().await;
										for subscriber in subscribers.iter_mut() {
											subscriber.send(Box::new(pool.clone())).await.unwrap();
										}
									}
									// println!("Elapsed time: {:?}", bench.elapsed().unwrap());
									// bench = SystemTime::now();
								}
								
							}
						}
					}));
				}
				futures::future::join_all(joins).await;
				
			});
			
		})
	}
}

#[async_trait]
impl LiquidityProvider for Animeswap {
	type Metadata = Box<dyn Meta>;
	fn get_id(&self) -> LiquidityProviders {
		LiquidityProviders::Aux
	}
	fn get_metadata(&self) -> Self::Metadata {
		Box::new(self.metadata.clone())
	}
	async fn get_pools(&self) -> HashMap<String, Pool> {
		let lock = self.pools.read().await;
		lock.clone()
	}
	fn load_pools(&self) -> JoinHandle<()> {
		let metadata = self.metadata.clone();
		let pools = self.pools.clone();
		
		tokio::spawn(async move {
			let aptos_client = Client::new_with_timeout(NODE_URL.clone(), Duration::from_secs(60));
			let resources_address = metadata.contract_address.clone();
			let wrapped_resources = aptos_client
				  .get_account_resources(AccountAddress::from_str(&resources_address).unwrap())
				  .await;
			if let Ok(resources) = wrapped_resources {
				for resource in resources.inner().into_iter() {
					match (
						&resource.clone().resource_type.module.into_string().as_str(),
						&resource.clone().resource_type.name.into_string().as_str(),
					) {
						(&module, &resource_name) => {
							
							if module != metadata.pool_module || resource_name != metadata.pool_name {
								continue;
							}
							
							let (coin_x, coin_y) = match (
								resource.resource_type.type_params.get(0).unwrap(),
								resource.resource_type.type_params.get(1).unwrap(),
							) {
								(TypeTag::Struct(struct_tag_x), TypeTag::Struct(struct_tag_y)) => (
									struct_tag_x.to_string(),
									struct_tag_y.to_string(),
								
								),
								_ => continue,
							};
							
							let amm: AnimeswapLiquidityPool = serde_json::from_value(resource.data.clone()).unwrap();
							if amm.coin_x_reserve.value.0 == 0 || amm.coin_y_reserve.value.0 == 0 {
								continue;
							}
							if KNOWN_STABLECOINS.iter().any(|(x, decimals)| x.to_string() == coin_x && amm.coin_x_reserve.value.0 as f64 / 10.0_f64.powf(*decimals as f64) < 10.0) {
									continue
							}
							if KNOWN_STABLECOINS.iter().any(|(y, decimals)| y.to_string() == coin_y &&  amm.coin_y_reserve.value.0 as f64 / 10.0_f64.powf(*decimals as f64) < 10.0) {
									continue
							}
							
							let pool = Pool {
								address: metadata.contract_address.clone()
									  + "::"
									  + module
									  + "::"
									  + resource_name
									  + "<"
									  + coin_x.as_str()
									  + ", "
									  + coin_y.as_str()
									  + ">",
								x_address: coin_x.clone(),
								fee_bps: 30,
								y_address: coin_y.clone(),
								curve_type: Uncorrelated,
								curve: None,
								x_amount: amm.coin_x_reserve.value.0,
								y_amount: amm.coin_y_reserve.value.0,
								x_to_y: true,
								provider: LiquidityProviders::AnimeSwap
							};
							// Get the pool's event source from resources
							
							let mut w = pools.write().await;
							w.insert(pool.address.clone(), pool);
						}
					}
				}
			} else {
				eprintln!("{:?}: {:?}", LiquidityProviders::AnimeSwap, wrapped_resources.unwrap_err());
			}
			println!("{:?} Pools: {}",LiquidityProviders::AnimeSwap, pools.read().await.len());
		})
	}
}
