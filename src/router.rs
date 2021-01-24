#![allow(dead_code)]
#![allow(unused_variables)]

use std::collections::{HashMap, VecDeque};
use std::sync::Arc;

/*use libp2p::{
	identity::{Keypair, PublicKey},
};*/
pub use crate::internet::{CustomNode, InternetID, InternetPacket};

pub type NodeID = u8;
pub struct SymmetricEncryption {
	session_id: u32,
	data: Vec<u8>,
}
pub struct AsymmetricEncryption {
	node_id: NodeID, // In implementation, this will be the PeerID (e.g. the hash of the public key)
	data: Vec<u8>, // Data that is "representative" of 
}

#[derive(Debug)]
struct RemoteNodeConnection {
	//pub_key: PublicKey,
	//noise_session: Option<snow::TransportState>,
	net_id: Option<InternetID>,
	route_id: Vec<u16>,
	latency: u32,
}
impl RemoteNodeConnection {
	fn new(/*pub_key: PublicKey*/) -> Self {
		Self {
			//pub_key,
			//session_key: Default::default(),
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
	// PingResponse packet, time between Ping and PingResponse is measured
	PingResponse,
	
	// Request PeerIDs
	RequestPeers,
	RequestPeersReponse(Vec<NodeID>),

	// Request to a peer for them to request their peers to ping me
	RequestPings(u32), // u32: max number of pings
	
	/// Request to establish a 2-way route between InternetID and this node through another node
	/// Vec<u8> is an encrypted packet (this can contain anything)
	Route(NodeID, Vec<u8>), 
	RouteError()
}
impl NodePacket {
	pub fn package(&self, node: &Node, dest: InternetID) -> InternetPacket {
		InternetPacket {
			src_addr: node.net_id,
			data: serde_json::to_vec(&self).expect("Failed to encode json"),
			dest_addr: dest,
		}
	}
	pub fn unpackage(packet: InternetPacket) -> Self {
		serde_json::from_slice(&packet.data).expect("Failed to decode json")
	}
}

#[derive(Debug)]
pub enum NodeAction {
	Bootstrap(InternetID, NodeID),
	Connect(NodeID),
}

#[derive(Derivative)]
#[derivative(Debug)]
pub struct Node {
	pub node_id: NodeID,
	// #[derivative(Debug="ignore")]
	// keypair: Keypair,
	pub net_id: InternetID,

	my_route: Vec<u16>,
	ticks: usize, // Amount of time passed since startup of this node

	peers: Vec<Arc<RemoteNode>>,
	net_id_map: HashMap<InternetID, Arc<RemoteNode>>,
	node_id_map: HashMap<NodeID, Arc<RemoteNode>>,

	actions_queue: VecDeque<NodeAction>,
}
impl CustomNode for Node {
	type CustomNodeAction = NodeAction;
	fn net_id(&self) -> InternetID {
		self.net_id
	}
	fn tick(&mut self, incoming: Vec<InternetPacket>) -> Vec<InternetPacket> {
		let mut outgoing: Vec<InternetPacket> = Vec::new();

		for packet in incoming {
			//let mut noise = builder.local_private_key(self.keypair.)
			self.parse_packet(packet, &mut outgoing);
		}
		while let Some(action) = self.actions_queue.pop_front() {
			match action {
				NodeAction::Bootstrap(net_id, node_id) => {
					outgoing.push(NodePacket::RequestPeers.package(&self, net_id));
				},
				NodeAction::Connect(node) => {

				}
			}
		}
		self.ticks += 1;
		
		outgoing
	}
	fn action(&mut self, action: NodeAction) {
		self.actions_queue.push_back(action);
	}
}
impl Node {
	pub fn new(node_id: NodeID, net_id: InternetID) -> Node {
		//let keypair = Keypair::generate_ed25519();
		//let node_id = key.public().into_peer_id();
		Node {
			node_id,
			//keypair,
			net_id,

			my_route: Default::default(),
			ticks: Default::default(),

			peers: Default::default(),
			net_id_map: Default::default(),
			node_id_map: Default::default(),
			actions_queue: Default::default(),
		}
	}
	pub fn parse_packet(&mut self, packet: InternetPacket, outgoing: &mut Vec<InternetPacket>) {
		let mut outgoing: Vec<InternetPacket> = Default::default();

		if packet.dest_addr == self.net_id {
			if let Ok(node_packet) = serde_json::from_slice::<NodePacket>(&packet.data[..]) {
				match node_packet {
					NodePacket::Ping => {
						outgoing.push(NodePacket::PingResponse.package(&self, packet.src_addr));
					},
					NodePacket::PingResponse => {
						// TODO: Log the time it too between Ping and PingResponse
						//self.ping
					},
					NodePacket::RequestPeers => {
						// TODO: Find Peer Ids to return (preferably close to the original requester)
					},
					NodePacket::RequestPeersReponse(node_ids) => {
						// TODO: Save these peers
						//let mut remote = self.net_id_map.get_mut(&packet.src_addr);
					},
					NodePacket::RequestPings(num) => {
						// TODO: Find nodes that might be close to requester and ask them to ping requester
					},
					NodePacket::Route(net_id, data) => {
						// outgoing.push(value)
					},
					_ => { },
				}
			} else {
				println!("Unknown Packet Data: {:?}", packet.data);
			};
		} else {
			println!("Received packet from: {} addressed to {} not me", packet.src_addr, packet.dest_addr);
		}
	}
}
