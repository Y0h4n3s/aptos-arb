use std::collections::{HashMap, HashSet};

use async_std::sync::Arc;
use petgraph::algo::all_simple_paths;
use petgraph::prelude::{Graph, NodeIndex};
use petgraph::{Undirected};
use tokio::sync::{RwLock, Semaphore};
use garb_sync_aptos::Pool;
use tokio::time::Duration;
// fn find_new_cycles(mut path: Vec<NodeIndex>, graph: &Graph<String, Pool, Undirected>, mut cycles: Vec<Vec<NodeIndex>>) -> Vec<Vec<NodeIndex>>   {
//         let start_node = path.first().unwrap().clone();
//         let mut next_node: Option<NodeIndex> = None;
//         let mut sub = vec![];
//
//         for edge in graph.edge_indices() {
//             let (node1, node2) = graph.edge_endpoints(edge).unwrap();
//             if start_node == node1 || start_node == node2 {
//
//                 if node1 == start_node {
//                     next_node = Some(node2);
//                 } else {
//                     next_node = Some(node1);
//                 }
//                 if !path.contains(&next_node.unwrap()) {
//                     sub.push(next_node.unwrap());
//                     sub.append(&mut path);
//                     cycles.append(&mut find_new_cycles(sub.clone(), graph, cycles.clone()));
//                 } else if path.len() > 2 && next_node.unwrap() == *path.last().unwrap() {
//                     println!("cycle found: {:?}", path);
//                     cycles.push( path.clone());
//                 }
//
//             }
//         }
//     return cycles.clone()
//
// }
//
// pub fn cycles_through(graph: &Graph<String, Pool, Undirected>, node: &NodeIndex) -> Vec<Vec<NodeIndex>> {
//     let mut cycles = vec![];
//     find_new_cycles(vec![*node], graph, cycles)
//
// }

fn combinations<T>(v: &[T], k: usize) -> Vec<Vec<T>>
where T: Clone {
    if k == 0 {
        return vec![vec![]];
    }
    let mut result = vec![];
    for i in 0..v.len() {
        let mut rest = v.to_vec();
        rest.remove(i);
        for mut c in combinations(&rest, k - 1) {
            c.push(v[i].clone());
            result.push(c);
        }
    }
    result
}
pub async fn start(
    pools: Arc<RwLock<HashMap<String, Pool>>>,
    updated_q: kanal::AsyncReceiver<Pool>,
    routes: Arc<kanal::AsyncSender<HashSet<(String, Vec<Pool>)>>>,
) -> anyhow::Result<()> {
    // This can be any token or coin but we use stable coins because they are used more as quote tokens
    // if we want to arb staked apt for example
    // 0x84d7aeef42d38a5ffc3ccef853e1b82e4958659d16a7de736a29c55fbbeb0114::staked_aptos_coin::StakedAptosCoin tAPT
    // 0xd11107bdf0d6d7040c6c0bfbdecb6545191fdf13e8d8d259952f53e1713f61b5::staked_coin::StakedAptos stAPT
    let checked_coins: Vec<&str> = vec![
        // "0x8d87a65ba30e09357fa2edea2c80dbac296e5dec2b18287113500b902942929d::celer_coin_manager::BusdCoin", // celer BUSD
        // "0xf22bede237a07e121b56d91a491eb7bcdfd1f5907926a9e58338f964a01b17fa::asset::USDC", // layer zero USDC
        // "0x5e156f1207d0ebfa19a9eeff00d62a282278fb8719f4fab3a586a0a2c0fffbea::coin::T", // wormhole USDC
        // "0x8d87a65ba30e09357fa2edea2c80dbac296e5dec2b18287113500b902942929d::celer_coin_manager::UsdcCoin", //celer USDC
        // "0xf22bede237a07e121b56d91a491eb7bcdfd1f5907926a9e58338f964a01b17fa::asset::USDT", // layerzero USDT
        // "0xa2eda21a58856fda86451436513b867c97eecb4ba099da5775520e0f7492e852::coin::T", // wormhole USDT
        // "0x8d87a65ba30e09357fa2edea2c80dbac296e5dec2b18287113500b902942929d::celer_coin_manager::UsdtCoin", // celer USDT
        // "0x8d87a65ba30e09357fa2edea2c80dbac296e5dec2b18287113500b902942929d::celer_coin_manager::DaiCoin", // celer DAI
        // "0xf22bede237a07e121b56d91a491eb7bcdfd1f5907926a9e58338f964a01b17fa::asset::USDD", // partners USDD
        // "0x1000000fa32d122c18a6a31c009ce5e71674f22d06a581bb0a15575e6addadcc::usda::USDA", // partners USDA
        "0x0000000000000000000000000000000000000000000000000000000000000001::aptos_coin::AptosCoin" // APT
    ];
    
    let mut the_graph: Graph<String, Pool, Undirected> = Graph::<String, Pool, Undirected>::new_undirected();
    let pr = pools.read().await;
    for (_, pool) in pr.iter() {
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
        if the_graph.edges_connecting(i1, i2).find(|e| e.weight().y_address == pool.y_address && e.weight().address == pool.address && e.weight().provider.id == pool.provider.id && e.weight().x_address == pool.x_address).is_none() {
            the_graph.add_edge(i1, i2, pool.clone());
        }
    }
    println!("graph service> Preparing routes {} ", the_graph.node_count(), );
    
    let mut checked_coin_indices: Vec<NodeIndex> = vec![];
    for checked_coin in checked_coins {
        if let Some(index) = the_graph
              .node_indices()
              .find(|i| the_graph[*i] == checked_coin)
        {
            checked_coin_indices.push(index);
            
        } else {
            println!(
                "graph service> Skipping {} because there are no pools with that coin",
                checked_coin
            );
        }
    }
    
    let mut path_lookup = Arc::new(RwLock::new(HashMap::<Pool, HashSet<(String, Vec<Pool>)>>::new()));
    let mut two_step_routes = HashSet::<(String, Vec<Pool>)>::new();
    
    // two step routes first
    for node in the_graph.node_indices() {
        for checked_coin in &*checked_coin_indices {
            let in_address = the_graph.node_weight(*checked_coin).unwrap().to_string();
        
            two_step_routes = two_step_routes.union(&the_graph.neighbors(node.clone()).map(|p| {
                let neighbor = the_graph.node_weight(p).unwrap();
                if *neighbor == in_address {
                    let edges_connecting = the_graph.edges_connecting(node.clone(), p);
                    if edges_connecting.clone().count() >= 2 {
                        let mut pools: Vec<Pool> = vec![];
                        // println!("graph service> Found route for {} to {} via {}", node_addr, neighbor, edges_connecting.clone().count());
                    
                        for edge in edges_connecting {
                            pools.push(Pool::from(edge.weight()));
                        }
                        let combined_routes = combinations(pools.as_mut_slice(), 2);
                        return combined_routes.iter().filter(|path| {
                            let first_pool = path.first().unwrap();
                            let last_pool = path.last().unwrap();
                            if (first_pool.x_address == in_address && last_pool.y_address == in_address) || (first_pool.y_address == in_address && last_pool.x_address == in_address) {
                                return true;
                            } else {
                                return false
                            }
                        }).map(|path| {
                            (in_address.clone(), path.clone())
                        }).collect::<Vec<(String, Vec<Pool>)>>();
                    } else {
                        return vec![]
                    }
                } else {
                    vec![]
                }
            }).filter(|p| p.len() > 0).flatten().collect::<HashSet<(String, Vec<Pool>)>>())
                  .map(|i| i.clone()).collect();
        }
    }
    let cores = num_cpus::get();
    let permits = Arc::new(Semaphore::new(cores));
    let edges_count = the_graph.edge_count();
    for (i, edge) in the_graph.edge_indices().enumerate() {
        let permit = permits.clone().acquire_owned().await.unwrap();
        println!("graph service> Preparing routes {} / {}", i, edges_count);
        let the_graph = the_graph.clone();
        let checked_coin_indices = checked_coin_indices.clone();
        let two_step_routes = two_step_routes.clone();
        let path_lookup = path_lookup.clone();
        tokio::spawn(async move {
            let edge = the_graph.edge_weight(edge).unwrap();
    
            let index1 = the_graph
                  .node_indices()
                  .find(|i| the_graph[*i] == edge.x_address)
                  .unwrap();
            let index2 = the_graph
                  .node_indices()
                  .find(|i| the_graph[*i] == edge.y_address)
                  .unwrap();
            // println!("graph service> Finding routes for {}", edge);
    
            let updated_nodes = vec![index1, index2];
            let mut safe_paths: HashSet<(String, Vec<Pool>)> = HashSet::new();
    
            for node in updated_nodes {
                for checked_coin in &*checked_coin_indices {
                    let in_address = the_graph.node_weight(*checked_coin).unwrap().to_string();
            
                    // TODO: make max_intermediate_nodes and min_intermediate_nodes configurable
                    // find all the paths that lead to the current checked coin from the updated coin
                    // max_intermediate_nodes limits the number of swaps we make, it can be any number but the bigger
                    // the number the more time it will take to find the paths
                    let to_checked_paths = all_simple_paths::<Vec<NodeIndex>, _>(
                        &the_graph,
                        node,
                        *checked_coin,
                        0,
                        Some(1),
                    )
                          .collect::<Vec<_>>();
                    for (i, ni) in to_checked_paths.iter().enumerate() {
                        for (j, nj) in to_checked_paths.iter().enumerate() {
                            // skip routing back and forth
                            if i == j {
                                continue;
                            }
                    
                            // eg. assuming the checked coin is wormhole usdc and the other coin is apt
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
                            // collect all the pools between the coins
                            // combine the edges
                            let mut edge_paths: Vec<Vec<Pool>> = vec![];
                            for (i, node) in new_path.iter().enumerate() {
                                if i == 0 {
                                    continue;
                                }
                                let edges = the_graph.edges_connecting(**node, *new_path[i - 1]);
                        
                                if edge_paths.len() <= 0 {
                                    for edge in edges.clone() {
                                        let mut pool = edge.weight().clone();
                                        pool.x_to_y = pool.x_address == in_address;
                                        edge_paths.push(vec![pool]);
                                    }
                                    continue;
                                }
                                let mut new_edge_paths = vec![];
                        
                                for old_path in edge_paths {
                                    let last_pool = old_path.last().unwrap();
                                    let in_a = if last_pool.x_to_y {
                                        last_pool.y_address.clone()
                                    } else {
                                        last_pool.x_address.clone()
                                    };
                                    for edge in edges.clone() {
                                        let mut pool = edge.weight().clone();
                                        pool.x_to_y = pool.x_address == in_a;
                                        new_edge_paths.push(old_path.clone().into_iter().chain(vec![pool]).collect());
                                    }
                                }
                                edge_paths = new_edge_paths;
                            }
                    
                    
                            for edge_path in edge_paths {
                                safe_paths.insert((in_address.clone(), edge_path));
                        
                            }
                        }
                    }
                }
            }
            // use only paths that route through the current edge
            // println!("Total Paths for {:?}: {}", edge.address, safe_paths.len());
            safe_paths = safe_paths.into_iter().filter(|(_in, path)| {
                path.iter().any(|p| p.address == edge.address && p.x_address == edge.x_address && p.y_address == edge.y_address)
            }).collect();
            let two_step = two_step_routes.clone().into_iter().filter( |(_in_addr, path) | {
                path.iter().any(|p| p.address == edge.address && p.x_address == edge.x_address && p.y_address == edge.y_address)
            }).map(|(in_addr, path) | {
                let mut first_pool = path.first().unwrap().clone();
                let mut second_pool = path.last().unwrap().clone();
                if first_pool.x_address != in_addr {
                    first_pool.x_to_y = false;
                }
                if second_pool.x_address != in_addr {
                    second_pool.x_to_y = false;
                }
                return (in_addr, vec![first_pool.clone(), second_pool.clone()]);
            }).collect::<HashSet<(String, Vec<Pool>)>>();
            safe_paths.extend(two_step);
            let mut w = path_lookup.write().await;
            w.insert(Pool::from(edge), safe_paths);
            std::mem::drop(permit);
        });
        
        
    }
    
    let mut total_paths = 0;
    for (_pool, paths) in path_lookup.read().await.clone() {
        total_paths += paths.len();
    }
    
    
    
    
    println!("graph service> Found {} routes", total_paths);
    for (pool, paths) in path_lookup.read().await.iter() {
        routes.send(paths.clone()).await.unwrap();
        tokio::time::sleep(Duration::from_secs(2)).await;
    }
    // while let Ok(updated_market) = updated_q.recv().await {
    //     if let Some(market_routes) = path_lookup.get(&updated_market) {
    //         if market_routes.len() <= 0 {
    //             continue
    //         }
    //         // println!("graph service> {} routes that go through {}", market_routes.len(), updated_market);
    //         routes.send(market_routes.clone()).await.unwrap();
    //     } else {
    //         eprintln!("graph service> No routes found for {}", updated_market);
    //     }
    // }
    Ok(())
}