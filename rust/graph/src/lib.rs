use std::borrow::BorrowMut;
use std::collections::{HashMap, HashSet, VecDeque};

use async_std::sync::Arc;
use petgraph::algo::all_simple_paths;
use petgraph::prelude::{Graph, NodeIndex};
use petgraph::Undirected;
use tokio::sync::mpsc::{Receiver, Sender};
use tokio::sync::RwLock;
use garb_sync_aptos::Pool;

// TODO: index all the possible routes and build a look up table
pub async fn start(
    pools: Arc<RwLock<HashMap<String, Pool>>>,
    mut updated_q: Receiver<Pool>,
    routes: Arc<Sender<Vec<Pool>>>,
) -> anyhow::Result<()> {
    // This can be any token or coin but we use stable coins because they are used more as quote tokens
    // if we want to arb staked apt for example
    // 0x84d7aeef42d38a5ffc3ccef853e1b82e4958659d16a7de736a29c55fbbeb0114::staked_aptos_coin::StakedAptosCoin tAPT
    // 0xd11107bdf0d6d7040c6c0bfbdecb6545191fdf13e8d8d259952f53e1713f61b5::staked_coin::StakedAptos stAPT
    let checked_coins: Vec<&str> = vec![
        "0x8d87a65ba30e09357fa2edea2c80dbac296e5dec2b18287113500b902942929d::celer_coin_manager::BusdCoin", // celer BUSD
        "0xf22bede237a07e121b56d91a491eb7bcdfd1f5907926a9e58338f964a01b17fa::asset::USDC", // layer zero USDC
        "0x5e156f1207d0ebfa19a9eeff00d62a282278fb8719f4fab3a586a0a2c0fffbea::coin::T", // wormhole USDC
        "0x8d87a65ba30e09357fa2edea2c80dbac296e5dec2b18287113500b902942929d::celer_coin_manager::UsdcCoin", //celer USDC
        "0xf22bede237a07e121b56d91a491eb7bcdfd1f5907926a9e58338f964a01b17fa::asset::USDT", // layerzero USDT
        "0xa2eda21a58856fda86451436513b867c97eecb4ba099da5775520e0f7492e852::coin::T", // wormhole USDT
        "0x8d87a65ba30e09357fa2edea2c80dbac296e5dec2b18287113500b902942929d::celer_coin_manager::UsdtCoin", // celer USDT
        "0x8d87a65ba30e09357fa2edea2c80dbac296e5dec2b18287113500b902942929d::celer_coin_manager::DaiCoin", // celer DAI
        "0xf22bede237a07e121b56d91a491eb7bcdfd1f5907926a9e58338f964a01b17fa::asset::USDD", // partners USDD
        "0x1000000fa32d122c18a6a31c009ce5e71674f22d06a581bb0a15575e6addadcc::usda::USDA", // partners USDA
        "0x0000000000000000000000000000000000000000000000000000000000000001::aptos_coin::AptosCoin" // APT
    ];

    let mut the_graph_g: Arc<RwLock<Graph<String, Pool, Undirected>>> = Arc::new(RwLock::new(Graph::<String, Pool, Undirected>::new_undirected()));
    let pr = pools.read().await;
    let mut the_graph = the_graph_g.write().await;
    for (_, pool) in pr.iter() {
        // println!("Adding node {:?} {} {}", pool.provider.id,pool.x_address.clone(), pool.y_address.clone());
        let index1 = the_graph
            .node_indices()
            .find(|i| the_graph[*i] == pool.x_address.clone());
        let index2 = the_graph
            .node_indices()
            .find(|i| the_graph[*i] == pool.y_address.clone());
        let i1 = if index1.is_none() {
            the_graph.add_node(pool.x_address.clone())
        } else {
            index1.unwrap()
        };
        let i2 = if index2.is_none() {
            the_graph.add_node(pool.y_address.clone())
        } else {
            index2.unwrap()
        };

        the_graph.update_edge(i1, i2, Pool::from(pool));
    }
    println!("Number of coins: {}", the_graph.node_count());

    let mut checked_coin_indices_s: Vec<NodeIndex> = vec![];
    for checked_coin in checked_coins {
        if let Some(index) = the_graph
            .node_indices()
            .find(|i| the_graph[*i] == checked_coin)
        {
            checked_coin_indices_s.push(index)
        } else {
            println!(
                "Skipping {} because there are no pools with that coin",
                checked_coin
            );
        }
    }
    let checked_coin_indices = Arc::new(checked_coin_indices_s);
    std::mem::drop(the_graph);
    
    while let Some(updated_market) = updated_q.recv().await {
        let routes = routes.clone();
        let the_graph_a = the_graph_g.clone();
        let checked_coin_indices = checked_coin_indices.clone();
        println!(
            "graph service> number of liquidity pools: {:?} routes_queued: {:?}",
            pools.read().await.len(),
            routes.clone().max_capacity() - routes.clone().capacity(),
        );
        tokio::spawn(async move {
            let the_graph = the_graph_a.read().await;
    
            // maybe pop the last element and drain the rest to get only the latest event
            let index1 = the_graph
                  .node_indices()
                  .find(|i| the_graph[*i] == updated_market.x_address)
                  .unwrap();
            let index2 = the_graph
                  .node_indices()
                  .find(|i| the_graph[*i] == updated_market.y_address)
                  .unwrap();
            println!("graph service> Finding routes for {} {} {}", updated_market, index1.index(), index2.index());
    
            let updated_nodes = vec![index1, index2];
    
            for updated_node in updated_nodes {
                if checked_coin_indices.contains(&updated_node) {
                    continue;
                }
                let mut safe_paths: HashSet<Vec<Pool>> = HashSet::new();
                for checked_coin in &*checked_coin_indices {
                    // TODO: make max_intermediate_nodes and min_intermediate_nodes configurable
                    // find all the paths that lead to the current checked coin from the updated coin
                    // max_intermediate_nodes limits the number of swaps we make, it can be any number but the bigger
                    // the number the more time it will take to find the paths
                    let to_checked_paths = all_simple_paths::<Vec<NodeIndex>, _>(
                        &*the_graph,
                        updated_node,
                        *checked_coin,
                        0,
                        Some(1),
                    )
                          .collect::<Vec<_>>();
                    println!("Total Paths found : {}", to_checked_paths.len());
                    for (i, ni) in to_checked_paths.iter().enumerate() {
                        'second: for (j, nj) in to_checked_paths.iter().enumerate() {
                            // skip routing back and forth
                            if i == j {
                                continue;
                            }
                    
                            // eg. assuming the checked coin is wormhole usdc and one of the coins in the updated pool  is apt
                            //     ni: [apt -> via aux -> usdd -> via liquidswap -> usdc]
                            //     for each nj: [[apt -> via animeswap -> usdc],[apt -> via aux -> mojo -> via aux -> usdc],...]
                            //
                            // p1 -> reverse -> pop = [usdc -> via liquidswap -> usdd]
                            // new_path = [usdc -> via liquidswap -> usdd -> via aux -> apt -> via animeswap -> usdc]
                            let mut p1 = ni.clone();
                            p1.reverse();
                            p1.pop();
                    
                            let new_path: Vec<&NodeIndex> = p1.iter().chain(nj).collect();
                    
                            if new_path.len() > 4 {
                                continue;
                            }
                            // collect the pools between the coins
                            let mut edge_path = vec![];
                            let mut in_address = the_graph.node_weight(*checked_coin).unwrap().to_string();
                            for (i, node) in new_path.iter().enumerate() {
                                if i == 0 {
                                    continue;
                                }
                                let edge = the_graph.find_edge(**node, *new_path[i - 1]);
                                if let Some(edge) = edge {
                                    let mut pool = the_graph.edge_weight(edge).unwrap().clone();
                                    pool.x_to_y = pool.x_address == *in_address;
                                    in_address = if pool.x_to_y {
                                        pool.y_address.clone()
                                    } else {
                                        pool.x_address.clone()
                                    };
                                    edge_path.push(pool);
                                } else {
                                    // There is no pool between the two paths continue
                                    // this should never happen
                                    println!(
                                        "Route not found between {} and {}",
                                        the_graph[**node],
                                        the_graph[*new_path[i - 1]]
                                    );
                                    continue 'second;
                                }
                                ;
                            }
                    
                            // println!("`````````````````````` Found Updated Path ``````````````````````");
                            // for (i,pool) in edge_path.iter().enumerate() {
                            //     println!("{}. {}", i+1, pool);
                            // }
                            // println!("\n\n");
                    
                            safe_paths.insert(edge_path);
                        }
                    }
                }
                // use only paths that route through the updated pool
                safe_paths = safe_paths.into_iter().filter(|path| {
                    path.iter().any(|p| p.address == updated_market.address && p.x_address == updated_market.x_address && p.y_address == updated_market.y_address)
                }).collect();
                println!("Total Valid Paths: {}", safe_paths.len());
                for path in safe_paths {
                    let routes = routes.clone();
                    routes.send(path).await.unwrap();
                }
                
                // let mut w = routes.write().await;
                // *w = w
                //       .union(&safe_paths)
                //       .cloned()
                //       .collect::<HashSet<Vec<Pool>>>();
                // std::mem::drop(w);
            }
        });
        
    }
    Ok(())

}
