#![feature(drain_filter)]
#![feature(backtrace)]
#![feature(try_blocks)]

#[macro_use]
extern crate serde;
extern crate log;
#[macro_use]
extern crate thiserror;
#[macro_use]
extern crate derivative;
#[macro_use]
extern crate anyhow;
#[macro_use]
extern crate bitflags;
#[macro_use]
extern crate slotmap;

use std::{fs::File, io::{self, BufReader, prelude::*}};
use anyhow::Context;

pub mod internet;
use internet::{NetAddr, NetSim, CustomNode};
pub mod node;
use node::{Node, NodeAction, NodeID};
pub mod plot;
use rand::SeedableRng;

const CACHE_FILE: &str = "./target/net.cache";

fn main() -> anyhow::Result<()> {
	env_logger::init();
	println!("Hello, Network!");
	let _ = std::fs::create_dir_all("target/images");

	let rng = &mut rand::rngs::SmallRng::seed_from_u64(0);
	// Try and read cache file, else gen new network
	let mut internet = NetSim::new();
	if let Ok(cache_file) = File::open(CACHE_FILE) {
		if let Ok(cached_network) = bincode::deserialize_from(BufReader::new(cache_file)) {
			println!("Loaded Cached Network: {}", CACHE_FILE);
			internet = cached_network;
		} else {
			println!("Found cache file but was unable to deserialize it, perhaps it is from an older version?");
		}
	}

	let stdin = io::stdin();
	let split_regex = fancy_regex::Regex::new(r#"((?<=")[^"]*(?=")|[^" ]+)"#)?;

	for line_result in stdin.lock().lines() {
		if let Ok(line) = line_result {
			// Look for 
			let input: Vec<&str> = split_regex.find_iter(&line[..]).flatten().map(|m|m.as_str()).collect();
			
			if let Err(err) = parse_command(&mut internet, &input, rng) {
				println!("{}", err);
			}
			
		} else { println!("Could not read line from input"); }
	}
	Ok(())
}

fn parse_command(internet: &mut NetSim<Node>, input: &[&str], rng: &mut impl rand::Rng) -> anyhow::Result<()> {
	match input {
		["help"] => {
			println!(
				r#"
						command list:
						add <NodeID> - add a node to network
						del <NetAddr> - delete node from network
						tick <usize> - run network a certain number of iterations
						net <subcommand> - network operations
						graph - output graph of current network as targe/images/network_snapshot.png
						list <subcommand> - list various aspects of network
						print <NetAddr> - pretty-print a node on the network
						node <subcommand> - node operations
						test <test> - run a specific test
				"#
			)
		}
		// Adding Nodes
		["add", id] => {
			if let Ok(node_id) = id.parse::<NodeID>() {
				let node = Node::new(node_id, internet.lease());
				println!("Adding Node: {:?}", node);
				internet.add_node(node, rng);
			} else { bail!("add: {:?} cannot be parsed as NodeID", id) }
		}
		["add"] => bail!("add: requires second argument to be NodeID"),
		// Removing Nodes
		["del", addr] => {
			let net_addr = addr.parse::<NetAddr>().context(anyhow!("del: {:?} cannot be parsed as NetAddr", addr))?;
			internet.del_node(net_addr);
		}
		["del"] => bail!("tick: requires second argument to be an existing NetAddr"),
		["tick", times] => {
			let num_ticks = times.parse::<usize>().context("tick: number of ticks must be type usize")?;
			println!("Running {} ticks", num_ticks);
			internet.tick(num_ticks, rng);
		}
		["tick"] => bail!("tick: requires second argument to be a valid positive integer"),
		// Configuring network
		["net", subcommand @ ..] => {
			match subcommand {
				["save", filepath] => {
					let mut file = File::create(filepath).context("net: save: failed to create file (check perms)")?;
					let data = bincode::serialize(&internet).context("net: save: failed to serialize network")?;
					file.write_all(&data).context("net: save: failed to write to file")?;
				}
				["save"] => bail!("net: save: must pass file path to save network"),
				["load", filepath] => {
					let file = File::open(filepath).context("net: load: failed to open file (check perms)")?;
					let internet_new: NetSim<Node> = bincode::deserialize_from(BufReader::new(file)).context("net: load: failed to deserialize network")?;
					*internet = internet_new;
					//internet = bincode::deserialize_from(BufReader::new(file)).context("net: save: failed to serialize object")?;
				}
				["load"] => bail!("net: load: must pass file path to load network"),
				["cache"] => {
					let mut cache_file = File::create(CACHE_FILE).context("net: cache: can't create ./net.cache (check perms?)")?;
					let data = bincode::serialize(&internet).context("net: cache: failed to serialize network")?;
					cache_file.write_all(&data).context("net: cache: failed to write to cache file")?;
					println!("Created network cache");
				}
				["clear"] => *internet = NetSim::new(),
				["gen", number] => {
					*internet = NetSim::new();

					let num_nodes = number.parse::<u32>().context("net: gen: <number:u32> for first argument")?;
					for i in 0..num_nodes {
						let node2 = Node::new(i, internet.lease());
						internet.add_node(node2, rng);
					}
		
					let snapshots_per_boot = 10;
					for i in 1..(internet.nodes.len()+0) {
						let node = internet.node_mut(i as NetAddr)?;
						node.action(NodeAction::Bootstrap(0,0));
						for _j in 0..snapshots_per_boot {
							internet.tick(4000/snapshots_per_boot, rng);
							//plot::default_graph(&internet, &internet.router.field_dimensions, &format!("target/images/{:0>6}.png", (i-1)*snapshots_per_boot+_j), (1280,720))?;
						}
					}
					internet.tick(10000, rng);
				}
				["print"] => println!("{:#?}", internet),
				_ => bail!("net: must pass valid subcommand: save <filepath>, load <filepath>, cache, clear, gen <number>, print"),
			}
		}
		["graph"] => {
			plot::default_graph(internet, &internet.router.field_dimensions, "target/images/network_snapshot.png", (1280,720))?;
			//internet.gen_routing_plot("target/images/network_snapshot.png", (500, 500))?;
		}
		// List nodes
		["list", subcommand @ ..] => {
			match *subcommand {
				["directs"] => internet.nodes.iter().for_each(|(addr,node)| println!("{}: {:?}", addr, node.direct_sorted)),
				["peers"] => internet.nodes.iter().for_each(|(addr,node)| println!("{}: {:?}", addr, node.peer_list)),
				["sessions"] => internet.nodes.iter().for_each(|(addr,node)| println!("{}: {:?}", addr, node.sessions)),
				["routes"] => internet.nodes.iter().for_each(|(addr,node)| println!("{}: {:?}", addr, node.route_coord)),
				["router"] => internet.router.node_map.iter().for_each(|(net_addr,lc)| println!("{}: {:?}", net_addr, lc)),
				["node", addr] => {
					let net_addr = addr.parse::<NetAddr>().context("Need NetAddr")?;
					println!("{:#?}", internet.node(net_addr));
				}
				["all"] => internet.nodes.iter().for_each(|(addr,node)|println!("{}:	{:?}", addr, node)),
				_ => { println!("list: unknown subcommand. valid: directs, peers, sessions, routes, router, node, all") }
			}
		}
		//["list"] => bail!("list: must have secondary command. allowed: directs, peers, sessions, routes, router, node, all"),
		["print", addr] => {
			if let Ok(net_addr) = addr.parse::<NetAddr>() {
				println!("{:#?}", internet.node(net_addr)?);
			} else { bail!("print: could not parse: {:?} as NetAddr", addr) };
		}
		["print"] => bail!("print: requires NetAddr as argument"),
		// Node subcommand
		["node", addr, command @ ..] => {
			let net_addr = addr.parse::<NetAddr>().context("node: must pass NetAddr corresponding to existing node")?;
			let node = internet.node_mut(net_addr)?;
			match command {
				["connect" | "conn", id, addr] => {
					let remote_node_id = id.parse::<NodeID>().context("node: connect: must pass valid NodeID")?;
					let remote_net_addr = addr.parse::<NetAddr>().context("node: connect: must pass valid NetAddr")?;
					println!("Connecting NodeID({:?}) to NodeID({:?}), NetAddr({:?}))", node.node_id, remote_node_id, remote_net_addr);
					node.action(NodeAction::Connect(remote_node_id, remote_net_addr, vec![]));
				}
				["connect" | "conn"] => bail!("node: connect: <NodeID> <NetAddr>"),
				["bootstrap" | "boot", id, addr] => {
					let remote_node_id = id.parse::<NodeID>().context("node: boostrap: must pass valid NodeID")?;
					let remote_net_addr = addr.parse::<NetAddr>().context("node: boostrap: must pass valid NetAddr")?;
					println!("Bootstrapping NodeID({:?}) to NodeID({:?}), NetAddr({:?}))", node.node_id, remote_node_id, remote_net_addr);
					node.action(NodeAction::Connect(remote_node_id, remote_net_addr, vec![]));
				}
				["boostrap" | "boot"] => bail!("node: bootstrap: <NodeID> <NetAddr>"),
				["print"] => println!("Node: {:#?}", node),
				["notify", id, data] => {
					let remote_node_id = id.parse::<NodeID>().context("node: notify: requires remote NodeID")?;
					let data = data.parse::<u64>().context("node: notify: data must be u64")?;
					node.action(NodeAction::Notify(remote_node_id, data));
				}
				["traverse", id] => {
					let remote_node_id = id.parse::<NodeID>().context("node: traverse: must pass valid NodeID")?;
					node.action(NodeAction::ConnectTraversal(remote_node_id));
				}
				["route", id] => {
					let remote_node_id = id.parse::<NodeID>().context("node: route: must pass valid NodeID")?;
					node.action(NodeAction::ConnectRouted(remote_node_id, 3));
				}
				_ => bail!("node: unknown subcommand"),
			}
		}
		["node"] => bail!("node: requires subcommand"),
		["test", subcommand @ ..] => {
			match subcommand {
				["sample", amount] => {
					let num_samples = amount.parse::<usize>().context("test: sample: requires number of samples")?;
					use permutation_iterator::{RandomPairPermutor, Permutor};
					let nlen = internet.nodes.len() as u32;
					let permutor = RandomPairPermutor::new(nlen, nlen).map(|(i,j)|(i as NetAddr, j as NetAddr));

					let nodes = permutor.take(num_samples).map(|(i,j)|((i, internet.nodes.get(&i).unwrap()), (j, internet.nodes.get(&j).unwrap())));
					let nodes = nodes.collect::<Vec<((NetAddr, &Node), (NetAddr, &Node))>>();
					//println!("{:?}", nodes);
					println!("Sampled Nodes: {:?}", nodes.iter().map(|(s,e)|(s.1.node_id, e.1.node_id)).collect::<Vec<(NodeID, NodeID)>>());

					//let hops = 3;
					let mut all_times: Vec<(NetAddr, NetAddr, Vec<u64>, u64, Vec<u64>, u64)> = Vec::new();
					for ((start_addr,start),(end_addr,end)) in nodes {
						
						/* let start_route_coord = start.route_coord.unwrap().map(|s|s as f64);
						let end_route_coord = end.route_coord.unwrap().map(|s|s as f64);
						let diff = (end_route_coord - start_route_coord) / hops as f64;
						let mut routes: Vec<Point2<f64>> = Vec::with_capacity(hops);
						for i in 1..hops {
							routes.push(start_route_coord + diff * i as f64);
						}) */

						// Calculate traversal times
						let mut routed_times: Vec<u64> = Vec::new();
						let end_route = end.route_coord.unwrap();
						//let mut current_id: NodeID = 0;
						let mut current_node: &Node = start;
						//println!("Current Sample: {:?} -> {:?}", current_node.node_id, end.node_id);
						let mut timeout = 10;
						// Run through path
						while current_node.node_id != end.node_id {
							let node_idx = current_node.find_closest_peer(&end_route).unwrap();
							let next_node = current_node.remote(node_idx).unwrap();
							//println!("Found Path {:?} -> {:?}", current_node.node_id, next_node.node_id);
							
							let next_node_session = next_node.session().unwrap();
							routed_times.push(next_node_session.dist());
							let next_net_addr = next_node_session.direct().unwrap().net_addr;
							current_node = internet.node(next_net_addr).unwrap();

							timeout -= 1;
							if timeout <= 0 {
								break
							}
						}
						let routed_times_sum: u64 = routed_times.iter().sum();
						if timeout <= 0 || routed_times_sum == 0 {
							continue
						}

						// Calculate random times
						let mut random_times = Vec::with_capacity(3);
						// Get some nodes
						let random_itermediate_nodes = Permutor::new(internet.nodes.len() as u64).take(3).map(|i|&internet.nodes[&(i as u128)]);
						let mut current_node = start;
						for node in random_itermediate_nodes {
							let dist = node::types::route_dist(&current_node.route_coord.unwrap(), &node.route_coord.unwrap());
							current_node = node;
							random_times.push(dist as u64);
						}
						let random_times_sum: u64 = random_times.iter().sum();

						all_times.push((start_addr, end_addr, routed_times, routed_times_sum, random_times, random_times_sum));
					}
					println!("All Times: {:?}", all_times);

					// Write to CSV Output
					let csv_file = File::create(format!("target/test_sample_{}.csv", num_samples)).unwrap();
					let mut wtr = csv::Writer::from_writer(csv_file);
					#[derive(Debug, Serialize)]
					struct TimeRecord {
						name: String,
						routed_time: u64,
						random_time: u64,
					}
					for time in all_times {
						wtr.serialize(TimeRecord {
							name: format!("{} -> {}", time.0, time.1),
							routed_time: time.3,
							random_time: time.5,
						}).unwrap();
					}
					wtr.flush().unwrap();
				}
				_ => {
					//internet.tick(5000, rng);
					//plot::default_graph(internet, &internet.router.field_dimensions, "target/images/network_snapshot.png", (1280, 720)).expect("Failed to output image");
					//internet.node_mut(1)?.action(NodeAction::ConnectRouted(19, 2));
					internet.node_mut(1)?.action(NodeAction::ConnectTraversal(19));
					//internet.node_mut(8)?.action(NodeAction::ConnectRouted(19, 3)); 
					internet.tick(10000, rng);
				}
			}
			
		}
		[command, ..] => bail!("unknown command: {}, type help for list of valid commands", command),
		_ => return Ok(())
	}
	Ok(())
}