#![feature(drain_filter)]

#[macro_use]
extern crate serde;
extern crate log;
#[macro_use]
extern crate thiserror;
#[macro_use]
extern crate derivative;

use std::io::{self, prelude::*};

pub mod internet;
use internet::{InternetID, InternetSim, CustomNode};
pub mod node;
use node::{Node, NodeAction, NodeID, NodeActionCondition};
use rand::SeedableRng;

fn main() {
	env_logger::init();
	println!("Hello, Network!");
	let _ = std::fs::create_dir_all("target/images");

	let rng = &mut rand::rngs::SmallRng::seed_from_u64(0);
	let mut internet = InternetSim::new();

	for i in 0..3 {
		let node2 = Node::new(i, internet.lease());
		internet.add_node(node2);
	}

	for i in 1..(internet.nodes.len()+0) {
		if let Some(node) = internet.node_mut(i) {
			node.action(NodeAction::Bootstrap(0,0));
		} else { log::error!("Node at InternetID({}) doesn't exist", i)}
		for _j in 0..30 {
			internet.tick(100, rng);
			//internet.gen_routing_plot(&format!("target/images/{:0>6}.png", (i-1)*30+j), (500, 500)).expect("Failed to output image");
		}
	}
	//internet.gen_routing_plot(&format!("target/images/{:0>6}.png", i/100), (500, 500)).expect("Failed to output image");

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
				internet.add_node(node);
			} else { Err("add: requires second argument to be NodeID")? }
		},
		// Removing Nodes
		Some(&"del") => {
			if let Some(Ok(net_id)) = command.next().map(|s|s.parse::<InternetID>()) {
				internet.del_node(net_id);
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
			internet.gen_routing_plot("target/images/network_snapshot.png", (500, 500))?;
		},
		// List nodes
		Some(&"list") => {
			if let Some(subcommand) = command.next() {
				match *subcommand {
					"peered" => internet.nodes.iter().for_each(|(id,node)| println!("{}: {:?}", id, node.node_list)),
					"router" => internet.router.speed_map.iter().for_each(|(net_id,lc)| println!("{}: {:?}", net_id, lc)),
					"routes" => internet.nodes.iter().for_each(|(id,node)| println!("{}: {:?}", id, node.route_coord)),
					_ => { println!("list: unknown subcommand") }
				}
			} else {
				internet.nodes.iter().for_each(|(id,node)|println!("{}:	{:?}", id, node));
			}
		},
		Some(&"print") => {
			if let Some(Ok(net_id)) = command.next().map(|s|s.parse::<InternetID>()) {
				if let Some(node) = internet.node(net_id) { println!("{:#?}", node) }
				else { Err("print: No node currently leases this InternetID")? };
			} else { Err("print: invalid InternetID format")? };
		},
		// Node subcommand
		Some(&"node") => {
			let net_id = if let Some(Ok(net_id)) = command.next().map(|s|s.parse::<InternetID>()) { net_id } else { return Err("node: must pass valid InternetID as second argument to identify specific node")? };
			let node = if let Some(node) = internet.node_mut(net_id) { node } else { return Err("node: no node at that network address")? };
			match command.next() {
				// Bootstrap a node onto the network
				Some(&"connect") | Some(&"conn") => {
					if let Some(Ok(remote_node_id)) = command.next().map(|s|s.parse::<NodeID>()) {
						if let Some(Ok(remote_net_id)) = command.next().map(|s|s.parse::<InternetID>()) {
							println!("Connecting NodeID({:?}) to NodeID({:?}), InternetID({:?}))", node.node_id, remote_node_id, remote_net_id);
							node.action(NodeAction::Connect(remote_node_id, remote_net_id, vec![]));
						} else { Err("node: connect: requires InternetID to bootstrap off of")? }
					} else { Err("node: connect: requires a NodeID to establish secure connection")? }
				},
				// Test a remote node
				Some(&"test") => {
					if let Some(Ok(remote_node_id)) = command.next().map(|s|s.parse::<NodeID>()) {
						node.action(NodeAction::TestNode(remote_node_id, 1000).gen_condition(NodeActionCondition::Session(remote_node_id)));
					}
				}
				Some(&"bootstrap") | Some(&"boot") => {
					if let Some(Ok(remote_node_id)) = command.next().map(|s|s.parse::<NodeID>()) {
						if let Some(Ok(remote_net_id)) = command.next().map(|s|s.parse::<InternetID>()) {
							println!("Bootstrapping NodeID({:?}) to NodeID({:?}), InternetID({:?}))", node.node_id, remote_node_id, remote_net_id);
							node.action(NodeAction::Bootstrap(remote_node_id, remote_net_id));
						} else { Err("node: bootstrap: requires InternetID to bootstrap off of")? }
					} else { Err("node: bootstrap: requires a NodeID to establish secure connection")? }
				}
				
				Some(&"info") => {
					println!("Node: {:#?}", internet.node(net_id).ok_or("node: info: No node matches this InternetID")?);
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