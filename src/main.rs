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

fn main() -> anyhow::Result<()> {
	env_logger::init();
	println!("Hello, Network!");
	let _ = std::fs::create_dir_all("target/images");

	let rng = &mut rand::rngs::SmallRng::seed_from_u64(0);
	let mut internet = NetSim::new();

	for i in 0..20 {
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
	internet.tick(5000, rng);
	plot::default_graph(&internet, &internet.router.field_dimensions, "target/images/network_snapshot.png", (1280, 720)).expect("Failed to output image");
	//internet.node_mut(1)?.action(NodeAction::ConnectRouted(19, 2));
	internet.node_mut(1)?.action(NodeAction::ConnectTraversal(19));
	//internet.node_mut(8)?.action(NodeAction::ConnectRouted(19, 3)); 
	internet.tick(10000, rng);

	println!("Finished Simulation");


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
					let data = bincode::serialize(&internet).context("net: save: failed to serialize object")?;
					file.write_all(&data).context("net: save: failed to write to file")?;
				}
				["save"] => bail!("net: save: must pass file path to save network"),
				["load", filepath] => {
					let file = File::open(filepath).context("net: save: failed to create file (check perms)")?;
					//internet = serde_json::from_reader(BufReader::new(file)).context("net: save: failed to serialize object")?;
				}
				["load"] => bail!("net: load: must pass file path to load network"),
				["print"] => println!("{:#?}", internet),
				_ => bail!("net: must pass valid subcommand: save, load, print"),
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
		_ => bail!("unknown command: {:?}", input)
	}
	Ok(())
}