#![allow(dead_code)]
#![allow(unused_variables)]

use std::collections::{HashMap};

pub use crate::internet::{CustomNode, InternetID, InternetPacket};

mod types;
pub use types::{NodeID, SessionID, RouteCoord, NodePacket, NodeEncryption, RemoteNode, RemoteNodeError};

#[derive(Debug, Clone)]
pub enum NodeAction {
	Bootstrap,
	Connect,
}
#[derive(Debug, Clone)]
/// A condition that should be satisfied before an action is executed
pub enum NodeActionCondition {
	DirectSession(NodeID, InternetID), // Has direct Internet Connection
	IndirectSession(NodeID), // Has routed connection
	None,
}

#[derive(Debug)]
pub struct Node {
	pub node_id: NodeID,
	pub net_id: InternetID,

	my_route: Vec<u16>,
	ticks: usize, // Amount of time passed since startup of this node

	pub peers: HashMap<NodeID, RemoteNode>,
	pub sessions: HashMap<SessionID, NodeID>,
	actions_queue: Vec<(NodeAction, NodeActionCondition)>, // Actions will wait here until NodeID session is established
}
impl CustomNode for Node {
	type CustomNodeAction = (NodeAction, NodeActionCondition);
	fn net_id(&self) -> InternetID {
		self.net_id
	}
	fn tick(&mut self, incoming: Vec<InternetPacket>) -> Vec<InternetPacket> {
		let mut outgoing: Vec<InternetPacket> = Vec::new();

		for packet in incoming {
			//let mut noise = builder.local_private_key(self.keypair.)
			if let Err(err) = self.parse_packet(packet, &mut outgoing) {
				println!("Failed to parse packet: {:?}", err);
			}
		}

		// Yield actions that are ready to be executed
		let mut aq = self.actions_queue.clone();
		let actions = aq.drain_filter(|(action, condition)|{
			match condition {
				// Init handshake and wait until direct connection is established
				NodeActionCondition::DirectSession(node_id, net_id) => {
					if let Some(remote) = self.peers.get(node_id) {
						remote.session_active()
					} else {
						let remote = self.peers.entry(*node_id).or_insert(RemoteNode::new(*node_id));
						outgoing.push(remote.gen_handshake(self.node_id).package(self.net_id, *net_id));
						false
					}
				},
				// Find RouteCoord, init onion route and wait until established
				NodeActionCondition::IndirectSession(node_id) => {
					/*match action {
						NodeAction::Connect => {
							
						},
						_ => { log::error!("NodeAction {:?} cannot be paired with NodeActionCondition {:?}", action, condition); true }
					}*/
					true
				},
				NodeActionCondition::None => { true },
			}
		}).collect::<Vec<_>>();
		for (action, condition) in actions.iter() {
			match condition {
				// Init handshake and wait until direct connection is established
				NodeActionCondition::DirectSession(node_id, net_id) => {
					
					if let Some(remote) = self.peers.get(node_id) {
						match action {
							NodeAction::Bootstrap => {
								match remote.gen_direct(self.net_id, NodePacket::RequestPings(10)) {
									Ok(packet) => { outgoing.push(packet) },
									Err(RemoteNodeError::NoSessionError {..} ) => { log::error!("No direct session at remote node even though DirectSession condition passed") },
									Err(e) => { log::error!("Direct session condition error: {:?}", e) },
								}
							}
							_ => { log::error!("Invalid NodeAction / NodeActionCondition pair"); },
						}
						
					} else { log::error!("Remote doesn't exist even though DirectSession condition passed"); }
				},
				// Find RouteCoord, init onion route and wait until established
				_ => { log::warn!("Unsupportd NodeActionCondition") }
			}
		}
		self.actions_queue = aq;

		self.ticks += 1;
		
		outgoing
	}
	fn action(&mut self, action: (NodeAction, NodeActionCondition)) {
		self.actions_queue.push(action);
	}
}
#[derive(Error, Debug)]
pub enum PacketParseError {
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
		if packet.dest_addr == self.net_id {
			use NodeEncryption::*;
			let encrypted = NodeEncryption::unpackage(&packet)?;
			log::info!("Node({:?}) Received Packet: {:?}", self.node_id, encrypted);
			match encrypted {
				Handshake { recipient, session_id, signer } => {
					if recipient == self.node_id {
						// If receive a Handshake Request, acknowledge it
						let remote = self.peers.entry(signer).or_insert(RemoteNode::new(recipient));
						let acknowledge_packet = remote.gen_acknowledgement(recipient, session_id, signer);
						self.sessions.insert(session_id, signer); // Register to SessionID index
						remote.assign_net_id(packet.src_addr);
						outgoing.push(acknowledge_packet.package(self.net_id, packet.src_addr));
					} else {
						return Err( PacketParseError::InvalidHandshakeRecipient { node_id: recipient } )
					}
				},
				Acknowledge { session_id, acknowledger } => {
					// If receive an Acknowledge request, validate Handshake previously sent out
					let remote = self.peers.get_mut(&acknowledger).ok_or(PacketParseError::UnknownAcknowledgement { from: acknowledger })?;
					remote.validate_handshake(session_id, acknowledger)?;
					remote.assign_net_id(packet.src_addr);
					self.sessions.insert(session_id, acknowledger); // Register to SessionID index
				},
				Session { session_id, packet: node_packet } => {
					let remote_node_id = self.sessions.get(&session_id).ok_or(PacketParseError::UnknownSession { session_id })?;
					log::info!("Node({}) received NodePacket::{:?} from NodeID({}), InternetID({})", self.node_id, node_packet, remote_node_id, packet.src_addr);
					let remote = self.peers.get(remote_node_id).ok_or(PacketParseError::InvalidRemoteButSessionExists {node_id: remote_node_id.clone()})?;
					match node_packet {
						NodePacket::Ping => {
							outgoing.push(remote.gen_direct(self.net_id, NodePacket::PingResponse)?);
						},
						NodePacket::PingResponse => {
							
							// TODO: Log the time it too between Ping and PingResponse
							//self.ping
						},
						NodePacket::RequestPings(num) => {
							log::trace!("Receieved RequestPings from {:?}", remote_node_id);
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
