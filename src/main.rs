#![feature(drain_filter)]

#[macro_use]
extern crate serde;

use std::io;
use std::io::prelude::*;

pub mod internet;
use internet::{InternetID, InternetSim};
pub mod router;
use router::{Node, NodeAction};

fn main() {
	println!("Hello, world!");

	let mut internet = InternetSim::new();
	internet.add(Node::default());

	let stdin = io::stdin();
	for line_result in stdin.lock().lines() {
		if let Ok(line) = line_result {
			let input = line.split(" ").collect::<Vec<&str>>();
			match input[..] {
				["add"] => {
					println!("Adding Node: {}", internet.add(Node::default()));
				},
				["del", x] => {
					if let Ok(num) = x.parse::<InternetID>() {
						internet.delete(num);
					}
				},
				["node", x, command, ] => {
					if let Ok(num) = x.parse::<InternetID>() {
						if let Some(node) = internet.node_mut(num) {
							match command {
								"bootstrap" => {
									if let Ok(num) = x.parse::<InternetID>() {
										node.action(NodeAction::Bootstrap(num));
									}
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
