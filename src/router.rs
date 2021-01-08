#![allow(dead_code)]
#![allow(unused_variables)]

use std::collections::{HashMap, VecDeque};
use std::sync::Arc;

pub use crate::internet::{CustomNode, InternetID, InternetPacket};

#[derive(Debug, Default)]
pub struct RemoteNode {
	net_id: InternetID,
	node_id: u32,
	route_id: Vec<u16>,
	latency: u32,
}

#[derive(Serialize, Deserialize, Debug)]
pub enum NodePacket {
	Ping,
	PingResponse,
	
	GetInfo(u32),

	
	Route(InternetID, Vec<u8>),
}
#[derive(Debug)]
pub enum NodeAction {
	Bootstrap(InternetID),
	Connect(u32),
}

#[derive(Default)]
pub struct Node {
	node_id: u32,
	pub net_id: InternetID,
	my_route: Vec<u16>,
	ticks: usize, // Amount of time passed since startup of this node

	peers: Vec<Arc<RemoteNode>>,
	net_id_map: HashMap<InternetID, Arc<RemoteNode>>,
	actions: VecDeque<NodeAction>,
}
impl CustomNode for Node {
	fn net_id(&self) -> InternetID {
		self.net_id
	}
	fn tick(&mut self, incoming: Vec<InternetPacket>) -> Vec<InternetPacket> {
		//let packets = Vec::new();
		for packet in incoming {
			packet.data
		}
		self.ticks += 1;
		Vec::new()
	}
}
impl Node {
	pub fn new(node_id: u32) -> Node {
		Node {
			node_id,
			..Default::default()
		}
	}
	pub fn action(&mut self, action: NodeAction) {
		self.actions.push_back(action);
	}
}
