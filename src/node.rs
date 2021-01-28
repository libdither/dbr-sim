#![allow(dead_code)]
#![allow(unused_variables)]

use std::collections::{HashMap, VecDeque};

pub use crate::internet::{CustomNode, InternetID, InternetPacket};

mod types;
pub use types::{NodeID, SessionID, RouteCoord, NodePacket, NodeEncryption, RemoteNode, RemoteNodeError};

#[derive(Debug)]
pub enum NodeAction {
	Bootstrap(InternetID, NodeID),
	Connect(NodeID),
}

#[derive(Debug)]
pub struct Node {
	pub node_id: NodeID,
	pub net_id: InternetID,

	my_route: Vec<u16>,
	ticks: usize, // Amount of time passed since startup of this node

	pub peers: HashMap<NodeID, RemoteNode>,
	pub sessions: HashMap<SessionID, NodeID>,
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
					if let Some(remote) = self.peers.get(&node_id) {
						if remote.session_active() {
							if let Ok(packet) = remote.gen_packet(&self, NodePacket::RequestPings(10)) {
								outgoing.push(packet);
							}
						}
					}
				},
				NodeAction::Connect(node) => {
					log::info!("Outgoing Connection");
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
#[derive(Error, Debug)]
enum PacketParseError {
    #[error("There is no known session: {session_id:?}")]
	UnknownSession { session_id: SessionID },
	#[error("InternetPacket from {from:?} was addressed to {intended_dest:?}, not me")]
	InvalidNetworkRecipient { from: InternetID, intended_dest: InternetID },
	#[error("Handshake was addressed to {node_id:?} and not me")]
	InvalidHandshakeRecipient { node_id: NodeID },
	#[error("Acknowledgement from {from:?} was recieved, but I didn't previously send a Handshake Request")]
	UnknownAcknowledgement { from: NodeID },
	#[error("Triggered RemoteNodeError")]
	RemoteNodeError(#[from] RemoteNodeError),
	#[error("Failed to decode packet data")]
	SerdeDecodeError(#[from] serde_json::Error),
	#[error("Session entry exists, but node entry does not {node_id:?}")]
	InvalidRemoteButSessionExists { node_id: NodeID },
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
			sessions: Default::default(),
			actions_queue: Default::default(),
		}
	}
	pub fn parse_packet(&mut self, packet: InternetPacket, outgoing: &mut Vec<InternetPacket>) -> Result<(), PacketParseError> {
		let mut outgoing: Vec<InternetPacket> = Default::default();

		if packet.dest_addr == self.net_id {
			use NodeEncryption::*;
			match NodeEncryption::unpackage(&packet)? {
				Handshake { recipient, session_id, signer } => {
					if recipient == self.node_id {
						// If receive a Handshake Request, acknowledge it
						let remote = self.peers.entry(signer).or_insert(RemoteNode::new(recipient));
						let acknowledge_packet = remote.gen_acknowledgement(&mut self, recipient, session_id, signer);
						outgoing.push(acknowledge_packet.package(self.net_id, packet.src_addr));
					} else {
						return Err( PacketParseError::InvalidHandshakeRecipient { node_id: recipient } )
					}
				},
				Acknowledge { session_id, acknowledger } => {
					// If receive an Acknowledge request, validate Handshake previously sent out
					if let Some(remote) = self.peers.get(&acknowledger) {
						remote.validate_handshake(&mut self, session_id, acknowledger);
					} else {
						return Err( PacketParseError::UnknownAcknowledgement { from: acknowledger } )
					}
				},
				Session { session_id, packet: node_packet } => {
					log::info!("Node {} received packet ({:?}) from InternetID:{}", self.node_id, node_packet, packet.src_addr);
					let remote_node_id = self.sessions.get(&session_id).ok_or(PacketParseError::UnknownSession { session_id })?;
					let remote = self.peers.get(remote_node_id).ok_or(PacketParseError::InvalidRemoteButSessionExists {node_id: remote_node_id.clone()})?;
					match node_packet {
						NodePacket::Ping => {
							outgoing.push(remote.gen_packet(self, NodePacket::PingResponse)?);
						},
						NodePacket::PingResponse => {
							// TODO: Log the time it too between Ping and PingResponse
							//self.ping
						},
						NodePacket::RequestPings(num) => {
							// TODO: Find nodes that might be close to requester and ask them to ping requester
						},
						NodePacket::WantPing(net_id, node_id) => {
							
						},
						NodePacket::Route(net_id, data) => {
							// outgoing.push(value)
						},
						_ => { },
					}
				}
			}
		} else {
			return Err( PacketParseError::InvalidNetworkRecipient { from: packet.src_addr, intended_dest: packet.dest_addr } )
		}
		Ok(())
	}
}
