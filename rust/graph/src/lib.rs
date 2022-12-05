use std::collections::{HashMap, HashSet};

use async_std::sync::Arc;
use garb_sync_aptos::Calculator;
use garb_sync_aptos::{EventSource, Pool};
use petgraph::algo::all_simple_paths;
use petgraph::prelude::{Graph, NodeIndex};
use petgraph::Undirected;
use tokio::runtime::Runtime;
use tokio::sync::{RwLock, Semaphore};
use tokio::time::Duration;
use once_cell::sync::Lazy;
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
where
    T: Clone,
{
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

pub struct Order {
    pub size: u64,
    pub decimals: u64,
    pub route: Vec<Pool>
}

static CHECKED_COIN: Lazy<String> =  Lazy::new(|| {
    std::env::var("APTOS_CHECKED_COIN").unwrap_or("0x1::aptos_coin::AptosCoin".to_string())

    
});

pub async fn start(
    pools: Arc<RwLock<HashMap<String, Pool>>>,
    updated_q: kanal::AsyncReceiver<Box<dyn EventSource<Event = Pool>>>,
    routes: Arc<RwLock<kanal::AsyncSender<Order>>>,
) -> anyhow::Result<()> {
    // This can be any token or coin but we use stable coins because they are used more as quote tokens
    // if we want to arb staked apt for example
    // 0x84d7aeef42d38a5ffc3ccef853e1b82e4958659d16a7de736a29c55fbbeb0114::staked_aptos_coin::StakedAptosCoin tAPT
    // 0xd11107bdf0d6d7040c6c0bfbdecb6545191fdf13e8d8d259952f53e1713f61b5::staked_coin::StakedAptos stAPT

    let mut the_graph: Graph<String, Pool, Undirected> =
        Graph::<String, Pool, Undirected>::new_undirected();
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
        if the_graph
            .edges_connecting(i1, i2)
            .find(|e| {
                e.weight().y_address == pool.y_address
                    && e.weight().address == pool.address
                    && e.weight().provider == pool.provider
                    && e.weight().x_address == pool.x_address
            })
            .is_none()
        {
          
            the_graph.add_edge(i1, i2, pool.clone());
        }
    }
    println!(
        "graph service> Preparing routes {} ",
        the_graph.node_count(),
    );

    let mut checked_coin_indices: Vec<NodeIndex> = vec![];
    
        if let Some(index) = the_graph
            .node_indices()
            .find(|i| the_graph[*i] == CHECKED_COIN.clone())
        {
            checked_coin_indices.push(index);
        } else {
            println!(
                "graph service> Skipping {} because there are no pools with that coin",
                CHECKED_COIN.clone()
            );
        }
    

    let mut path_lookup = Arc::new(RwLock::new(
        HashMap::<Pool, HashSet<(String, Vec<Pool>)>>::new(),
    ));
    let mut two_step_routes = HashSet::<(String, Vec<Pool>)>::new();

    // two step routes first
    for node in the_graph.node_indices() {
        for checked_coin in &*checked_coin_indices {
            let in_address = the_graph.node_weight(*checked_coin).unwrap().to_string();

            two_step_routes = two_step_routes
                .union(
                    &the_graph
                        .neighbors(node.clone())
                        .map(|p| {
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
                                    return combined_routes
                                        .iter()
                                        .filter(|path| {
                                            let first_pool = path.first().unwrap();
                                            let last_pool = path.last().unwrap();
                                            if (first_pool.x_address == in_address
                                                && last_pool.y_address == in_address)
                                                || (first_pool.y_address == in_address
                                                    && last_pool.x_address == in_address)
                                            {
                                                return true;
                                            } else {
                                                return false;
                                            }
                                        })
                                        .map(|path| (in_address.clone(), path.clone()))
                                        .collect::<Vec<(String, Vec<Pool>)>>();
                                } else {
                                    return vec![];
                                }
                            } else {
                                vec![]
                            }
                        })
                        .filter(|p| p.len() > 0)
                        .flatten()
                        .collect::<HashSet<(String, Vec<Pool>)>>(),
                )
                .map(|i| i.clone())
                .collect();
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
                                        new_edge_paths.push(
                                            old_path
                                                .clone()
                                                .into_iter()
                                                .chain(vec![pool])
                                                .collect(),
                                        );
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
            safe_paths = safe_paths
                .into_iter()
                .filter(|(_in, path)| {
                    path.iter().any(|p| {
                        p.address == edge.address
                            && p.x_address == edge.x_address
                            && p.y_address == edge.y_address
                        && p.provider == edge.provider
                    })
                })
                .collect();
            let two_step = two_step_routes
                .clone()
                .into_iter()
                .filter(|(_in_addr, path)| {
                    path.iter().any(|p| {
                        p.address == edge.address
                            && p.x_address == edge.x_address
                            && p.y_address == edge.y_address
                        && p.provider == edge.provider
                    })
                })
                .map(|(in_addr, path)| {
                    let mut first_pool = path.first().unwrap().clone();
                    let mut second_pool = path.last().unwrap().clone();
                    if first_pool.x_address != in_addr {
                        first_pool.x_to_y = false;
                    }
                    if second_pool.x_address != in_addr {
                        second_pool.x_to_y = false;
                    }
                    return (in_addr, vec![first_pool.clone(), second_pool.clone()]);
                })
                .collect::<HashSet<(String, Vec<Pool>)>>();
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
    // for (pool, paths) in path_lookup.read().await.iter() {
    //     routes.send(paths.clone()).await.unwrap();
    //     tokio::time::sleep(Duration::from_secs(2)).await;
    // }
    std::thread::spawn(move || {
        let rt = Runtime::new().unwrap();
        let task_set = tokio::task::LocalSet::new();
        task_set.block_on(&rt, async {
            while let Ok(updated_market_event) = updated_q.recv().await {
                let updated_market = updated_market_event.get_event();
                let path_lookup = path_lookup.clone();
                let routes = routes.clone();
                tokio::task::spawn_local(async move {
                    if let Some((pool, market_routes)) = path_lookup.read().await.iter().find(|(key, value)|updated_market.address == key.address && updated_market.x_address == key.x_address && updated_market.y_address == key.y_address && updated_market.provider == key.provider) {
                        if market_routes.len() <= 0 {
                            return;
                        }
                        // println!(
                        //     "graph service> {} routes that go through {}",
                        //     market_routes.len(),
                        //     updated_market
                        // );
                        for (pool_addr, paths) in market_routes.iter() {
                            if !paths.contains(pool) {
                                continue
                            }
                            let in_addr = if paths.first().unwrap().x_to_y {
                                paths.first().unwrap().x_address.clone()
                            } else {
                                paths.first().unwrap().y_address.clone()
                            };
                            let decimals = decimals(in_addr);
                    
                            let mut best_route_index = 0;
                            let mut best_route = 0.0;
                            for i in 1..120 {
                                let i_atomic = (i as u64) * 10_u64.pow(decimals as u32);
                                let mut in_ = i_atomic;
                                for route in paths {
                                    let calculator = route.provider.build_calculator();
                                   
                                    in_ = calculator.calculate_out(in_, route);
                                }
                                if in_ < i_atomic {
                                    continue;
                                }
                        
                                let percent = in_ as f64 - i_atomic as f64;
                                // println!("{}: {} {} {} {}", i, in_, percent, i_atomic, paths.iter().map(|p| format!("{:?}", p.provider.clone())).collect::<Vec<String>>().join("->"));

                                if percent > best_route {
                                    best_route = percent;
                                    best_route_index = i;
                                }
                            }
                            
                            if best_route > 0.0 {
                                // println!(
                                //     "graph service> {} increase for size {} -> {}",
                                //     best_route, best_route_index, pool_addr
                                // );
                                
                                let order = Order {
                                    size: best_route_index as u64,
                                    decimals,
                                    route: paths.clone(),
                                };
                                // println!("graph service> trying size {:?} on route", order.size);
                                // for (i, pool) in order.route.iter().enumerate() {
                                //     println!("{}. {}", i + 1, pool);
                                // }
                                // println!("\n\n");
                                let mut r = routes.write().await;
                                let s = r.try_send(order).unwrap();
                            }
                    
                    
                        }
                
                    } else {
                        eprintln!("graph service> No routes found for {}", updated_market);
                    }
                });
            }
        });
        
    }).join().unwrap();
    
    
    Ok(())
}
fn decimals(coin: String) -> u64 {
    match coin.as_str() {
        "0x1::aptos_coin::AptosCoin" => {
            8
        }
        _ => {6}
    }
}
