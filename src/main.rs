#![feature(drain_filter)]

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

use std::io::{self, prelude::*};

pub mod internet;
use internet::{NetAddr, InternetSim, CustomNode};
pub mod node;
use node::{Node, NodeAction, NodeID};
pub mod plot;
use rand::SeedableRng;

fn main() {
	env_logger::init();
	println!("Hello, Network!");
	let _ = std::fs::create_dir_all("target/images");

	let rng = &mut rand::rngs::SmallRng::seed_from_u64(0);
	let mut internet = InternetSim::new();

	for i in 0..3 {
		let node2 = Node::new(i, internet.lease());
		internet.add_node(node2, rng);
	}

	let snapshots_per_boot = 10;
	for i in 1..(internet.nodes.len()+0) {
		if let Some(node) = internet.node_mut(i as NetAddr) {
			node.action(NodeAction::Bootstrap(0,0));
		} else { log::error!("Node at NetAddr({}) doesn't exist", i)}
		for _j in 0..snapshots_per_boot {
			internet.tick(4000/snapshots_per_boot, rng);
			//plot::default_graph(&internet, &internet.router.field_dimensions, &format!("target/images/{:0>6}.png", (i-1)*snapshots_per_boot+_j), (1280,720)).unwrap();
		}
	}
	internet.tick(4000, rng);
	plot::default_graph(&internet, &internet.router.field_dimensions, "target/images/network_snapshot.png", (1280, 720)).expect("Failed to output image");
	//internet.node_mut(8).unwrap().action(NodeAction::Traverse(7, 1000));
	//internet.node_mut(8).unwrap().action(NodeAction::ConnectRouted(19, 3)); 
	//internet.tick(1000, rng);

	let stdin = io::stdin();
	let split_regex = fancy_regex::Regex::new(r#"((?<=")[^"]*(?=")|[^" ]+)"#).unwrap();

	for line_result in stdin.lock().lines() {
		if let Ok(line) = line_result {
			// Look for 
			let input: Vec<&str> = split_regex.find_iter(&line[..]).flatten().map(|m|m.as_str()).collect();
			
			if let Err(err) = parse_command(&mut internet, &input, rng) {
				println!("Error: {:?}", err);
			}
			
		} else { println!("Could not read line from input"); }
	}
}

use std::error::Error;
fn parse_command(internet: &mut InternetSim<Node>, input: &Vec<&str>, rng: &mut impl rand::Rng) -> Result<(), Box<dyn Error>> {
	let mut command = input.iter();
	match command.next() {
		// Adding Nodes
		Some(&"add") => {
			if let Some(Ok(node_id)) = command.next().map(|s|s.parse::<NodeID>()) {
				let node = Node::new(node_id, internet.lease());
				println!("Adding Node: {:?}", node);
				internet.add_node(node, rng);
			} else { Err("add: requires second argument to be NodeID")? }
		},
		// Removing Nodes
		Some(&"del") => {
			if let Some(Ok(net_addr)) = command.next().map(|s|s.parse::<NetAddr>()) {
				internet.del_node(net_addr);
			}
		},
		Some(&"tick") => {
			if let Some(Ok(num_ticks)) = command.next().map(|s|s.parse::<usize>()) {
				println!("Running {} ticks", num_ticks);
				internet.tick(num_ticks, rng);
			}
		},
		// Configuring network
		Some(&"net") => {
			println!("{:#?}", internet);
		},
		Some(&"graph") => {
			plot::default_graph(internet, &internet.router.field_dimensions, "target/images/network_snapshot.png", (1280,720))?;
			//internet.gen_routing_plot("target/images/network_snapshot.png", (500, 500))?;
		},
		// List nodes
		Some(&"list") => {
			if let Some(subcommand) = command.next() {
				match *subcommand {
					"directs" => internet.nodes.iter().for_each(|(id,node)| println!("{}: {:?}", id, node.direct_sorted)),
					"peers" => internet.nodes.iter().for_each(|(id,node)| println!("{}: {:?}", id, node.peer_list)),
					"sessions" => internet.nodes.iter().for_each(|(id,node)| println!("{}: {:?}", id, node.sessions)),
					"routes" => internet.nodes.iter().for_each(|(id,node)| println!("{}: {:?}", id, node.route_coord)),
					"router" => internet.router.node_map.iter().for_each(|(net_addr,lc)| println!("{}: {:?}", net_addr, lc)),
					"node" => {
						if let Some(node_id) = command.next().map(|s|s.parse::<NetAddr>().ok()).flatten() {
							println!("{:#?}", internet.node(node_id));
						}
					}
					_ => { println!("list: unknown subcommand") }
				}
			} else {
				internet.nodes.iter().for_each(|(id,node)|println!("{}:	{:?}", id, node));
			}
		},
		Some(&"print") => {
			if let Some(Ok(net_addr)) = command.next().map(|s|s.parse::<NetAddr>()) {
				if let Some(node) = internet.node(net_addr) { println!("{:#?}", node) }
				else { Err("print: No node currently leases this NetAddr")? };
			} else { Err("print: invalid NetAddr format")? };
		},
		// Node subcommand
		Some(&"node") => {
			let net_addr = if let Some(Ok(net_addr)) = command.next().map(|s|s.parse::<NetAddr>()) { net_addr } else { return Err("node: must pass valid NetAddr as second argument to identify specific node")? };
			let node = if let Some(node) = internet.node_mut(net_addr) { node } else { return Err("node: no node at that network address")? };
			match command.next() {
				// Bootstrap a node onto the network
				Some(&"connect") | Some(&"conn") => {
					if let Some(Ok(remote_node_id)) = command.next().map(|s|s.parse::<NodeID>()) {
						if let Some(Ok(remote_net_addr)) = command.next().map(|s|s.parse::<NetAddr>()) {
							println!("Connecting NodeID({:?}) to NodeID({:?}), NetAddr({:?}))", node.node_id, remote_node_id, remote_net_addr);
							node.action(NodeAction::Connect(remote_node_id, remote_net_addr, vec![]));
						} else { Err("node: connect: requires NetAddr to bootstrap off of")? }
					} else { Err("node: connect: requires a NodeID to establish secure connection")? }
				},
				Some(&"bootstrap") | Some(&"boot") => {
					if let Some(Ok(remote_node_id)) = command.next().map(|s|s.parse::<NodeID>()) {
						if let Some(Ok(remote_net_addr)) = command.next().map(|s|s.parse::<NetAddr>()) {
							println!("Bootstrapping NodeID({:?}) to NodeID({:?}), NetAddr({:?}))", node.node_id, remote_node_id, remote_net_addr);
							node.action(NodeAction::Bootstrap(remote_node_id, remote_net_addr));
						} else { Err("node: bootstrap: requires NetAddr to bootstrap off of")? }
					} else { Err("node: bootstrap: requires a NodeID to establish secure connection")? }
				},
				Some(&"print") => {
					println!("Node: {:#?}", internet.node(net_addr).ok_or("node: info: No node matches this NetAddr")?);
				},
				Some(&"notify") | Some(&"nt") => {
					if let Some(Ok(remote_node_id)) = command.next().map(|s|s.parse::<NodeID>()) {
						if let Some(Ok(data)) = command.next().map(|s|s.parse::<u64>()) {
							node.action(NodeAction::Notify(remote_node_id, data));
						} else { Err("node: traverse: data must be u64")? }
					} else { Err("node: traverse: requires a NodeID to send to")? }
				},
				Some(&"route") => {
					if let Some(Ok(remote_node_id)) = command.next().map(|s|s.parse::<NodeID>()) {
						node.action(NodeAction::ConnectRouted(remote_node_id, 3));
					} else { Err("node: route: requires a NodeID to create route to")? }
				}
				Some(_) => Err(format!("node: unknown node command: {:?}", input[2]))?,
				None => Err(format!("node: requires subcommand"))?
			}
		},
		Some(_) => Err(format!("Invalid Command: {:?}", input))?,
		None => {},
	}
	Ok(())
}