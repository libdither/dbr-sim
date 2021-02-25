#![allow(dead_code)]
#![allow(unused_variables)]

mod router;
use router::{InternetRouter, EuclidianLatencyCalculator};

use std::collections::HashMap;

pub type InternetID = usize;

#[derive(Default, Debug)]
pub struct InternetPacket {
	pub dest_addr: InternetID,
	pub data: Vec<u8>,
	pub src_addr: InternetID,
}

pub trait CustomNode: std::fmt::Debug {
	type CustomNodeAction;
	fn net_id(&self) -> InternetID;
	fn tick(&mut self, incoming: Vec<InternetPacket>) -> Vec<InternetPacket>;
	fn action(&mut self, action: Self::CustomNodeAction);
}

#[derive(Debug)]
pub struct InternetSim<CN: CustomNode> {
	pub nodes: HashMap<InternetID, CN>,
	router: InternetRouter<EuclidianLatencyCalculator>,
}
impl<CN: CustomNode> InternetSim<CN> {
	pub fn new() -> InternetSim<CN> {
		InternetSim {
			nodes: HashMap::new(),
			router: Default::default(),
		}
	}
	pub fn lease(&self) -> InternetID {
		self.nodes.len()
	}
	pub fn add_node(&mut self, node: CN) {
		self.nodes.insert(node.net_id(), node);
	}
	pub fn del_node(&mut self, net_id: InternetID) {
		self.nodes.remove(&net_id);
	}
	pub fn node_mut(&mut self, node_id: InternetID) -> Option<&mut CN> {
		self.nodes.get_mut(&node_id)
	}
	pub fn node(&self, node_id: InternetID) -> Option<&CN> {
		self.nodes.get(&node_id)
	}
	pub fn list_nodes(&mut self) {
		
	}
	pub fn run(&mut self, ticks: usize) {
		//let packets_tmp = Vec::new();
		for i in 0..ticks {
			for node in self.nodes.values_mut() {
				// Get Packets going to node
				let incoming_packets = self.router.tick_node(node.net_id());
				// Get packets coming from node
				let mut outgoing_packets = node.tick(incoming_packets);
				// Make outgoing packets have the correct return address
				for packet in &mut outgoing_packets { packet.src_addr = node.net_id(); }
				// Send packets through the router
				self.router.add_packets(outgoing_packets);
			}
		}
		
	}
}
