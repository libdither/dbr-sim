#![feature(drain_filter)]

#[macro_use]
extern crate serde;
#[macro_use]
extern crate derivative;

use std::io::{self, prelude::*};

pub mod internet;
use internet::{InternetID, InternetSim};
pub mod router;
use router::{Node, NodeAction, NodeID};

fn main() {
	println!("Hello, Network!");

	let mut internet = InternetSim::new();
	let node = Node::new(internet.lease());
	internet.add_node(node);

	let stdin = io::stdin();
	let split_regex = fancy_regex::Regex::new(r#"((?<=")[^"]*(?=")|[^" ]+)"#).unwrap();

	for line_result in stdin.lock().lines() {
		if let Ok(line) = line_result {
			//let input = line.split(" ").collect::<Vec<&str>>();
			let string = &line[..];

			// This replaces .find_iter() in regular regex crate
			let mut input: Vec<&str> = Vec::new();
			let mut current_pos = 0;
			loop {
				let capture = split_regex.captures_from_pos(string, current_pos).map_or(None, |c|c);
				if let Some(Some(cap)) = capture.map(|c|c.get(0)) { current_pos = cap.end(); input.push(cap.as_str()) } else { break }
			}
			// This is what is should be
			//let input = split_regex.find_iter(string).map(|x| x.as_str()).collect::<Vec<&str>>();

			match input[..] {
				// Adding Nodes
				["add"] => {
					let node = Node::new(internet.lease());
					println!("Adding Node: {:?}", node);
					internet.add_node(node);
				},
				// Removing Nodes
				["del", x] => {
					if let Ok(num) = x.parse::<InternetID>() {
						internet.del_node(num);
					}
				},
				// Node subcommand
				["node", x, command, arg] => {
					if let Ok(num) = x.parse::<InternetID>() {
						if let Some(node) = internet.node_mut(num) {
							match command {
								// Bootstrap a node onto the network
								"bootstrap" => {
									if let Ok(num) = arg.parse::<InternetID>() {
										node.action(NodeAction::Bootstrap(num));
									}
								}
								// Initiate a connection and send some message
								"send" => {
									if let Ok(num) = arg.parse::<NodeID>() {
										node.action(NodeAction::Connect(num));
									}
								}
								"info" => {
									println!("Node: {:#?}", node);
								}
								_ => println!("Unknown node command: {:?}", command),
							}
						}
					}
				},
				_ => println!("Invalid Command: {:?}", input),
			}
		}
	}
}
