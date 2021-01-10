#![allow(dead_code)]
#![allow(unused_variables)]

use std::collections::{HashMap, VecDeque};
use std::sync::Arc;

use libp2p::{
	PeerId,
	identity::{Keypair, PublicKey},
};

pub use crate::internet::{CustomNode, InternetID, InternetPacket};

pub type NodeID = PeerId;

#[derive(Debug)]
struct RemoteNodeConnection {
	pub_key: PublicKey,
	session_key:
	net_id: Option<InternetID>,
	route_id: Vec<u16>,
	latency: u32,
}
impl RemoteNodeConnection {
	fn new(pub_key: PublicKey) -> Self {
		Self {
			pub_key,
			net_id: Default::default(),
			route_id: Default::default(),
			latency: Default::default(),
		}
	}
}
#[derive(Debug)]
pub struct RemoteNode {
	node_id: NodeID,
	connection: Option<RemoteNodeConnection>,
}
impl RemoteNode {
	fn new(node_id: NodeID) -> RemoteNode {
		RemoteNode {
			node_id,
			connection: None,
		}
	}
}

#[derive(Serialize, Deserialize, Debug)]
pub enum NodePacket {
	// Sent to other Nodes. Expects PingResponse returned
	Ping,
	// PingResponse, contains
	PingResponse(u16),
	
	GetInfo(u32),
	
	/// Request to establish a 2-way route between InternetID and this node through another node
	/// Vec<u8> is an encrypted packet (this can contain anything)
	Route(InternetID, Vec<u8>), 
	RouteError()
}
#[derive(Debug)]
pub enum NodeAction {
	Bootstrap(InternetID),
	Connect(NodeID),
}

#[derive(Derivative)]
#[derivative(Debug)]
pub struct Node {
	pub node_id: NodeID,
	#[derivative(Debug="ignore")]
	keypair: Keypair,
	pub net_id: InternetID,

	my_route: Vec<u16>,
	ticks: usize, // Amount of time passed since startup of this node

	peers: Vec<Arc<RemoteNode>>,
	net_id_map: HashMap<InternetID, Arc<RemoteNode>>,
	node_id_map: HashMap<PeerId, Arc<RemoteNode>>,

	actions_queue: VecDeque<NodeAction>,
}
impl CustomNode for Node {
	fn net_id(&self) -> InternetID {
		self.net_id
	}
	fn tick(&mut self, incoming: Vec<InternetPacket>) -> Vec<InternetPacket> {
		let outgoing: Vec<InternetPacket> = Vec::new();
		for packet in incoming {

			if let Ok(node_packet) = serde_json::from_slice::<NodePacket>(&packet.data[..]) {
				match node_packet {
					NodePacket::Ping => {

					},
					NodePacket::PingResponse => {

					},
					NodePacket::GetInfo => {

					},
					NodePacket::Route(InternetID) => {

					},
				}
			}
			packet.data
		}
		while let Some(action) = self.actions_queue.pop_front() {
			match action {
				NodeAction::Bootstrap(id) => {

				},
				NodeAction::Connect()
			}
		}
		self.ticks += 1;
		Vec::new()
	}
}
impl Node {
	pub fn new(net_id: InternetID) -> Node {
		let keypair = Keypair::generate_ed25519();
		let node_id = key.public().into_peer_id();
		Node {
			node_id,
			keypair,
			net_id,

			peers: Default::default(),
			net_id_map: Default::default(),
			node_id_map: Default::default(),
			actions_queue: Default::default(),
		}
	}
	pub fn action(&mut self, action: NodeAction) {
		self.actions_queue.push_back(action);
	}
}
