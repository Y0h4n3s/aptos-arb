use aptos_sdk::rest_client::aptos_api_types::{Address, U64};
use aptos_sdk::rest_client::types::GUID;
use serde::{Serialize, Deserialize};
use aptos_sdk::types::event::EventKey;

#[derive(Serialize, Deserialize, Debug, PartialEq, Eq)]
pub struct AptosCoin {
	pub value: U64,
}

#[derive(Serialize, Deserialize, Debug, PartialEq, Eq)]
pub struct CoinStoreResource {
	pub coin: AptosCoin,
	frozen: bool,
	deposit_events: EventHandle,
	withdraw_events: EventHandle,
}
#[derive(Serialize, Deserialize, Debug, PartialEq, Eq)]
pub struct EventHandle {
	counter: String,
	guid: GUID
}

#[derive(Serialize, Deserialize, Debug, PartialEq, Eq)]
pub struct AuxAmmPool {
	pub add_liquidity_events: EventHandle,
	pub fee_bps: U64,
	pub frozen: bool,
	pub lp_burn: DummyField,
	pub lp_mint: DummyField,
	pub remove_liquidity_events: EventHandle,
	pub swap_events: EventHandle,
	pub timestamp: U64,
	pub x_reserve: AptosCoin,
	pub y_reserve: AptosCoin,
	
}

#[derive(Serialize, Deserialize, Debug, PartialEq, Eq)]
pub struct DummyField {
	dummy_field: bool
}

#[derive(Serialize, Deserialize, Debug, PartialEq, Eq)]
pub struct ObricPieceSwapPoolInfo {
	pub K: U64,
	pub K2: U64,
	pub Xa: U64,
	pub Xb: U64,
	pub lp_amt: U64,
	pub lp_burn_cap: DummyField,
	pub lp_mint_cap: DummyField,
	pub lp_freeze_cap: DummyField,
	pub m: U64,
	pub n: U64,
	pub protocol_fee_share_per_thousand: U64,
	pub protocol_fee_x: AptosCoin,
	pub protocol_fee_y: AptosCoin,
	pub reserve_x: AptosCoin,
	pub reserve_y: AptosCoin,
	pub swap_fee_per_million: U64,
	pub x_deci_mult: U64,
	pub y_deci_mult: U64,
	
}

#[derive(Serialize, Deserialize, Debug, PartialEq, Eq)]
pub struct CetuePool {
	pub burn_capability: DummyField,
	pub coin_a: AptosCoin,
	pub coin_b: AptosCoin,
	pub locked_liquidity: AptosCoin,
	pub mint_capability: DummyField,
	pub protocol_fee_to: Address
}

#[derive(Serialize, Deserialize, Debug, PartialEq, Eq)]
pub struct PancakeTokenPairMetadata {
	pub balance_x: AptosCoin,
	pub balance_y: AptosCoin,
	pub burn_cap: DummyField,
	pub creator: Address,
	pub fee_amount: AptosCoin,
	pub freeze_cap: DummyField,
	pub k_last: String,
	pub mint_cap: DummyField,
}

#[derive(Serialize, Deserialize, Debug,PartialEq, Eq)]
pub struct AptoswapPool {
	pub admin_fee: U64,
	pub connect_fee: U64,
	pub fee_direction: u8,
	pub freeze: bool,
	pub incentive_fee: U64,
	pub index: U64,
	pub ksp_e8_sma: AptoswapKspE8Sma,
	pub last_trade_time: U64,
	pub liquidity_event: EventHandle,
	pub lp_fee: U64,
	pub lsp_supply: U64,
	pub pool_type: u8,
	pub snapshot_event: EventHandle,
	pub snapshot_last_capture_time: U64,
	pub stable_amp: U64,
	pub stable_x_scale: U64,
	pub stable_y_scale: U64,
	pub swap_token_event: EventHandle,
	pub total_trade_24h_last_capture_time: U64,
	pub total_trade_x: U64,
	pub total_trade_x_24h: U64,
	pub total_trade_y: U64,
	pub total_trade_y_24h: U64,
	pub withdraw_fee: U64,
	pub x: AptosCoin,
	pub y: AptosCoin,
}
#[derive(Serialize, Deserialize, Debug, PartialEq, Eq)]
pub struct AptoswapKspE8Sma {
	pub a0: String,
	pub a1: String,
	pub a2: String,
	pub a3: String,
	pub a4: String,
	pub a5: String,
	pub a6: String,
	pub c0: String,
	pub c1: String,
	pub c2: String,
	pub c3: String,
	pub c4: String,
	pub c5: String,
	pub c6: String,
	pub current_time: U64,
	pub start_time: U64,
}