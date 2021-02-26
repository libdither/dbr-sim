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

fn main() {
	env_logger::init();
	println!("Hello, Network!");

	let mut internet = InternetSim::new();
	let node = Node::new(0, internet.lease());
	internet.add_node(node);
	let node1 = Node::new(1, internet.lease())
		.with_action(NodeAction::Bootstrap(0, 0));
	internet.add_node(node1);

	for i in 2..4 {
		let node2 = Node::new(i, internet.lease())
		.with_action(NodeAction::Bootstrap(0, 0)); //.gen_condition(NodeActionCondition::RunAt(3000)));
		internet.add_node(node2);
	}
	

	internet.run(30000);


	let stdin = io::stdin();
	let split_regex = fancy_regex::Regex::new(r#"((?<=")[^"]*(?=")|[^" ]+)"#).unwrap();

	for line_result in stdin.lock().lines() {
		if let Ok(line) = line_result {
			// Look for 
			let input: Vec<&str> = split_regex.find_iter(&line[..]).flatten().map(|m|m.as_str()).collect();
			
			if let Err(err) = parse_command(&mut internet, &input) {
				println!("Error: {:?}", err);
			}
			
		} else { println!("Could not read line from input"); }
	}
}

use std::error::Error;
fn parse_command(internet: &mut InternetSim<Node>, input: &Vec<&str>) -> Result<(), Box<dyn Error>> {
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
				internet.run(num_ticks);
			}
		},
		// Configuring network
		Some(&"net") => {
			println!("{:#?}", internet);
		},
		Some(&"graph") => {
			internet.gen_routing_plot("target/network_graph.png", (500, 500))?;
		},
		// List nodes
		Some(&"list") => {
			if let Some(subcommand) = command.next() {
				match *subcommand {
					"peered" => internet.nodes.iter().for_each(|(id,node)| println!("{}: {:?}", id, node.peered_nodes)),
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
							node.action(NodeAction::Connect(remote_node_id, remote_net_id));
						} else { Err("node: connect: requires InternetID to bootstrap off of")? }
					} else { Err("node: connect: requires a NodeID to establish secure connection")? }
				},
				Some(&"bootstrap") | Some(&"boot") => {
					if let Some(Ok(remote_node_id)) = command.next().map(|s|s.parse::<NodeID>()) {
						if let Some(Ok(remote_net_id)) = command.next().map(|s|s.parse::<InternetID>()) {
							println!("Bootstrapping NodeID({:?}) to NodeID({:?}), InternetID({:?}))", node.node_id, remote_node_id, remote_net_id);
							node.action(NodeAction::Bootstrap(remote_node_id, remote_net_id));
						} else { Err("node: bootstrap: requires InternetID to bootstrap off of")? }
					} else { Err("node: bootstrap: requires a NodeID to establish secure connection")? }
				}
				// Initiate a connection and send some message
				Some(&"ping") => {
					if let Some(Ok(remote_node_id)) = command.next().map(|s|s.parse::<NodeID>()) {
						if let Some(Ok(num)) = command.next().map(|s|s.parse::<usize>()) {
							node.action(NodeAction::Ping(remote_node_id, num).gen_condition(NodeActionCondition::Session(remote_node_id)));
						}
					}
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