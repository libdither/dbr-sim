#![feature(drain_filter)]

#[macro_use]
extern crate serde;
#[macro_use]
extern crate derivative;
#[macro_use]
extern crate log;

use std::io::{self, prelude::*};

pub mod internet;
use internet::{InternetID, InternetSim};
pub mod router;
use router::{Node, NodeAction, NodeID};

fn main() {
	env_logger::init();
	println!("Hello, Network!");

	let mut internet = InternetSim::new();
	let node = Node::new(0, internet.lease());
	internet.add_node(node);

	let stdin = io::stdin();
	let split_regex = fancy_regex::Regex::new(r#"((?<=")[^"]*(?=")|[^" ]+)"#).unwrap();

	for line_result in stdin.lock().lines() {
		if let Ok(line) = line_result {
			// This replaces .find_iter() in regular regex crate
			let mut input: Vec<&str> = Vec::new();
			let mut current_pos = 0;
			loop {
				let capture = split_regex.captures_from_pos(&line[..], current_pos).map_or(None, |c|c);
				if let Some(Some(cap)) = capture.map(|c|c.get(0)) { current_pos = cap.end(); input.push(cap.as_str()) } else { break }
			}
			// This is what is should be
			//let input = split_regex.find_iter(string).map(|x| x.as_str()).collect::<Vec<&str>>();

			println!("Parsing Info: {:?}", input);
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
		// Configuring network
		Some(&"net") => {

		},
		// List nodes
		Some(&"list") => {
			internet.list_nodes();
		},
		// Node subcommand
		Some(&"node") => {
			if let Some(Ok(net_id)) = command.next().map(|s|s.parse::<InternetID>()) {
				match command.next() {
					// Bootstrap a node onto the network
					Some(&"bootstrap") => {
						if let Some(Ok(bootstrap_net_id)) = command.next().map(|s|s.parse::<InternetID>()) {
							if let Some(Ok(bootstrap_node_id)) = command.next().map(|s|s.parse::<NodeID>()) {
								internet.node_action(net_id, NodeAction::Bootstrap(bootstrap_net_id, bootstrap_node_id))?;
							} else { Err("node: bootstrap: requires a NodeID to establish secure connection")? }
						} else { Err("node: bootstrap: requires InternetID to bootstrap off of")? }
					}
					// Initiate a connection and send some message
					Some(&"send") => {
						if let Some(Ok(num)) = command.next().map(|s|s.parse::<NodeID>()) {
							internet.node_action(net_id, NodeAction::Connect(num))?;
						}
					}
					Some(&"info") => {
						println!("Node: {:#?}", internet.node(net_id).ok_or("node: info: No node matches this InternetID")?);
					}
					Some(_) => Err(format!("node: unknown node command: {:?}", input[2]))?,
					None => Err(format!("node: requires subcommand"))?
				}
			} else { println!("node: Must pass valid InternetID as second argument to identify specific node") }
		},
		Some(_) => println!("Invalid Command: {:?}", input),
		None => {},
	}
	Ok(())
}