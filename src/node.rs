#[allow(dead_code)]

const MAX_DIRECT_NODES: usize = 10;
// Amount of time to wait to connect to a peer who wants to ping
const WANT_PING_CONN_TIMEOUT: usize = 300;
const MAX_REQUEST_PINGS: usize = 10;

use std::collections::HashMap;
use std::cmp::Reverse;

use priority_queue::PriorityQueue;

pub use crate::internet::{CustomNode, InternetID, InternetPacket};

mod types;
mod session;
pub use types::{NodeID, SessionID, RouteCoord, NodePacket, NodeEncryption, RemoteNode, RemoteNodeError, RouteScalar};
use session::{RemoteSession, SessionError};

#[derive(Debug, Clone)]
/// A condition that should be satisfied before an action is executed
pub enum NodeActionCondition {
	Session(NodeID), // Has a session (Direct or Routed)
	DirectSession(NodeID), // Has direct Internet Connection
	PeerTested(NodeID), // Has node been considered as candidate for self.direct_node
	RunAfter(usize), // Has time elapsed enough
}
#[derive(Error, Debug)]
pub enum NodeActionConditionError {
    #[error("Node Error")]
	NodeError(#[from] NodeError),
	#[error("RemoteNode Error")]
	RemoteNodeError(#[from] RemoteNodeError),
}
impl NodeActionCondition {
	// Returns Some(Self) if condition should be tested again, else returns None if condition is passed
	fn test(self, node: &mut Node) -> Result<Option<Self>, NodeActionConditionError> {
		Ok(match self {
			// Yields if there is a session
			NodeActionCondition::Session(node_id) => node.remote(&node_id)?.session_active().then(||self),
			// Yields if there is a session and it is direct
			NodeActionCondition::DirectSession(node_id) => {
				let remote = node.remote(&node_id)?;
				(remote.session()?.is_direct() && remote.session_active()).then(||self)
			},
			// Yields if direct session is viable
			NodeActionCondition::PeerTested(node_id) => {
				(node.remote_mut(&node_id)?.session_mut()?.tracker.is_viable().is_some()).then(||self)
			},
			// Yields if a specified amount of time has passed
			NodeActionCondition::RunAfter(time) => (node.ticks >= time).then(||self),
			// Yields and runs nested action
		})
	}
}
#[derive(Debug, Clone)]
pub enum NodeAction {
	/// Initiate Handshake with remote NodeID, InternetID
	ConnectDirect(NodeID, InternetID),
	/// Ping a node
	Ping(NodeID, usize), // Ping node X number of times
	/// Continually Ping remote until connection is deamed viable or unviable
	TestDirect(NodeID),
	/// Send specific packet to node
	Packet(NodeID, NodePacket),
	/// Request Peers of another node to ping me
	RequestPeers(NodeID, usize),
	/// Request another nodes peers to make themselves known
	Bootstrap(NodeID, InternetID),
	/// Establish a dynamic routed connection
	Route(NodeID, RouteCoord),
	/// Condition for a condition to be fulfilled before running imbedded Action
	Condition(NodeActionCondition, Box<NodeAction>),
}
impl NodeAction {
	pub fn gen_condition(self, condition: NodeActionCondition) -> NodeAction {
		NodeAction::Condition(condition, Box::new(self))
	}
}

#[derive(Debug)]
pub struct Node {
	pub node_id: NodeID,
	pub net_id: InternetID,

	my_route: Vec<u16>,
	ticks: usize, // Amount of time passed since startup of this node

	pub remotes: HashMap<NodeID, RemoteNode>,
	pub sessions: HashMap<SessionID, NodeID>,
	pub direct_nodes: PriorityQueue<SessionID, Reverse<RouteScalar>>, // Sort Queue SessionID by distance (use Reverse to access shortest index)
	pub actions_queue: Vec<NodeAction>, // Actions will wait here until NodeID session is established
}
impl CustomNode for Node {
	type CustomNodeAction = NodeAction;
	fn net_id(&self) -> InternetID {
		self.net_id
	}
	fn tick(&mut self, incoming: Vec<InternetPacket>) -> Vec<InternetPacket> {
		let mut outgoing: Vec<InternetPacket> = Vec::new();

		// Parse Incoming Packets
		for packet in incoming {
			//let mut noise = builder.local_private_key(self.keypair.)
			if let Err(err) = self.parse_packet(packet, &mut outgoing) {
				println!("Failed to parse packet: {:?}", err);
			}
		}
		
		// Run actions in queue 
		// This is kinda inefficient
		let mut aq = self.actions_queue.clone();
		self.actions_queue.clear();
		let generated_actions = aq.drain_filter(|action| {
			match self.parse_action(&action, &mut outgoing) {
				Ok(resolved) => resolved,
				Err(err) => { log::info!("Action {:?} errored: {:?}", action, err); false },
			}
		}).collect::<Vec<_>>();
		self.actions_queue.append(&mut aq);
		// Check for Yielded NodeAction::Condition and list embedded action in queue
		for action in generated_actions.into_iter() {
			match action {
				NodeAction::Condition(_, action) => self.actions_queue.push(*action),
				_ => { println!("Done Action: {:?}", action); },
			}
		}

		self.ticks += 1;
		
		outgoing
	}
	fn action(&mut self, action: NodeAction) {
		self.actions_queue.push(action);
	}
}
#[derive(Error, Debug)]
pub enum PacketParseError {
	#[error("Node Error")]
	NodeError(#[from] NodeError),
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
	#[error("Remote Session Error")]
	SessionError(#[from] SessionError),
	#[error("Failed to decode packet data")]
	SerdeDecodeError(#[from] serde_json::Error),
	#[error("There are no known directly connected nodes")]
	NoDirectNodes,
}
#[derive(Error, Debug)]
pub enum ActionError {
    #[error("Node Error")]
	NodeError(#[from] NodeError),
	#[error("RemoteNode Error")]
	RemoteNodeError(#[from] RemoteNodeError),
	#[error("Session Error")]
	SessionError(#[from] SessionError),
	#[error("NodeActionCondition Error")]
	NodeActionConditionError(#[from] NodeActionConditionError),
}
#[derive(Error, Debug)]
pub enum NodeError {
    #[error("There is no known remote: {node_id:?}")]
	NoRemoteError { node_id: NodeID },
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

			remotes: Default::default(),
			sessions: Default::default(),
			direct_nodes: Default::default(),
			actions_queue: Default::default(),
		}
	}
	pub fn with_action(mut self, action: NodeAction) -> Self {
		self.actions_queue.push(action);
		self
	}
	pub fn remote(&self, node_id: &NodeID) -> Result<&RemoteNode, NodeError> { self.remotes.get(node_id).ok_or(NodeError::NoRemoteError{node_id: *node_id}) }
	pub fn remote_mut(&mut self, node_id: &NodeID) -> Result<&mut RemoteNode, NodeError> { self.remotes.get_mut(node_id).ok_or(NodeError::NoRemoteError{node_id: *node_id}) }

	pub fn parse_action(&mut self, action: &NodeAction, outgoing: &mut Vec<InternetPacket>) -> Result<bool, ActionError> {
		match action.clone() {
			// ConnectDirect to remote node
			NodeAction::ConnectDirect(remote_node_id, remote_net_id) => {
				// Insert RemoteNode if doesn't exist
				let remote = self.remotes.entry(remote_node_id).or_insert(RemoteNode::new(remote_node_id));
				// Run Handshake if no active session
				if !remote.session_active() {
					let packet = remote.gen_handshake(self.node_id, RemoteSession::with_direct(remote_net_id));
					let packet = packet.package(self.net_id, remote_net_id);
					outgoing.push(packet);
				}
			},
			NodeAction::Ping(remote_node_id, num_pings) => {
				let self_ticks = self.ticks;
				let session = self.remote_mut(&remote_node_id)?.session_mut()?;
				for _ in 0..num_pings {
					let packet = NodePacket::Ping(session.tracker.gen_ping(self_ticks));
					let packet: InternetPacket = session.gen_packet(packet)?;
					outgoing.push(packet);
				}
			},
			NodeAction::TestDirect(remote_node_id) => {
				let session = self.remote_mut(&remote_node_id)?.session_mut()?;
				let pending_pings = session.tracker.pending_pings();
				let is_viable = session.tracker.is_viable();
				match is_viable {
					None => {
						if pending_pings < 4 {
							self.action(NodeAction::Ping(remote_node_id, 3).gen_condition(NodeActionCondition::DirectSession(remote_node_id)));
						}
						self.action(NodeAction::TestDirect(remote_node_id).gen_condition(NodeActionCondition::RunAfter(30)));
					},
					Some(status) => {
						if status && !session.direct_mut()?.was_requested { self.action(NodeAction::Packet(remote_node_id, NodePacket::DirectRequest)); }
					}
				}
			},
			NodeAction::Packet(remote_node_id, packet) => {
				// Send packet to remote
				self.remote(&remote_node_id)?.add_packet(packet, outgoing)?;
			},
			NodeAction::RequestPeers(remote_node_id, num_peers) => {
				self.action(NodeAction::Packet(remote_node_id, NodePacket::RequestPings(num_peers)));
			}
			NodeAction::Bootstrap(remote_node_id, net_id) => {
				// Connect directly to node
				self.action(NodeAction::ConnectDirect(remote_node_id, net_id));
				// Test Direct connection
				self.action(NodeAction::TestDirect(remote_node_id).gen_condition(NodeActionCondition::DirectSession(remote_node_id)));
				// Ask for Pings
				self.action(NodeAction::RequestPeers(remote_node_id, 10).gen_condition(NodeActionCondition::PeerTested(remote_node_id)));
			},
			NodeAction::Route(_remote_node_id, _remote_route_coord ) => {},
			// Embedded action is run in main loop
			NodeAction::Condition(condition, _) => {
				return Ok(condition.test(self)?.is_some());
			}
			// _ => { log::error!("Invalid NodeAction / NodeActionCondition pair"); },
		}
		Ok(true)
	}
	pub fn parse_packet(&mut self, received_packet: InternetPacket, outgoing: &mut Vec<InternetPacket>) -> Result<(), PacketParseError> {
		if received_packet.dest_addr == self.net_id {
			use NodeEncryption::*;
			let encrypted = NodeEncryption::unpackage(&received_packet)?;
			match encrypted {
				Handshake { recipient, session_id, signer } => {
					log::debug!("Node({:?}) Received Handshake: {:?}", self.node_id, encrypted);
					if recipient == self.node_id {
						// If receive a Handshake Request, acknowledge it
						let remote = self.remotes.entry(signer).or_insert(RemoteNode::new(signer));
						let acknowledge_packet = remote.gen_acknowledgement(recipient, session_id);
						self.sessions.insert(session_id, signer); // Register to SessionID index
						outgoing.push(acknowledge_packet.package(self.net_id, received_packet.src_addr));
					} else {
						return Err( PacketParseError::InvalidHandshakeRecipient { node_id: recipient } )
					}
				},
				Acknowledge { session_id, acknowledger } => {
					log::debug!("Node({:?}) Received Acknowledgement: {:?}", self.node_id, encrypted);
					// If receive an Acknowledge request, validate Handshake previously sent out
					let remote = self.remote_mut(&acknowledger)?;
					remote.validate_handshake(session_id, acknowledger)?;
					self.sessions.insert(session_id, acknowledger); // Register to SessionID index
				},
				Session { session_id, packet: node_packet } => {
					let return_node_id = *self.sessions.get(&session_id).ok_or(PacketParseError::UnknownSession { session_id })?;
					log::debug!("Node({}) received NodePacket::{:?} from NodeID({}), InternetID({})", self.node_id, node_packet, return_node_id, received_packet.src_addr);
					//let return_remote = self.remote_mut(&return_node_id)?;
					match node_packet {
						NodePacket::Ping(ping_id) => {
							// Return ping
							self.remote(&return_node_id)?.add_packet(NodePacket::PingResponse(ping_id), outgoing)?;
						},
						NodePacket::PingResponse(ping_id) => {
							// Acknowledge ping
							let ticks = self.ticks;
							let session = self.remote_mut(&return_node_id)?.session_mut()?;
							session.tracker.acknowledge_ping(ping_id, ticks)?;
							
							// Add or remove from direct_nodes list depending on if it is a viable connection or not
							/* match direct_session.is_viable() {
								Some(false) => self.action(NodeAction::P)
							}
							if Some(true) == direct_session.is_viable() {
								// Log direct nodes
								let dist = direct_session.distance();
								self.direct_nodes.push(session_id, Reverse(dist));
							} else {
								self.direct_nodes.remove(&session_id);
							} */
						},
						// Request another node to reciprocate a connection test
						NodePacket::DirectRequest => {
							// Make sure Session is direct
							let session = self.remote_mut(&return_node_id)?.session_mut()?;
							session.request_direct(received_packet.src_addr);
							
							let distance = session.tracker.distance();
							match session.tracker.is_viable() {
								None => self.action(NodeAction::TestDirect(return_node_id)),
								Some(true) => { self.direct_nodes.push(session_id, Reverse(distance)); },
								Some(false) => {},
							};
						}
						NodePacket::RequestPings(requests) => {
							// Loop through first min(N,MAX_REQUEST_PINGS) items of priorityqueue
							let num_requests = usize::min(requests, MAX_REQUEST_PINGS); // Maximum of 10 requests
							for (session_id, _) in self.direct_nodes.iter().take(num_requests) {
								// Try get node
								let node_id = self.sessions.get(session_id).ok_or(PacketParseError::UnknownSession { session_id: *session_id })?;
								// Try get remote
								let remote = self.remote(node_id)?;
								// Generate packet sent to nearby remotes that this node wants to be pinged
								remote.add_packet(NodePacket::WantPing(return_node_id, received_packet.dest_addr), outgoing)?;
							}
							// TODO: Find nodes that might be close to requester and ask them to ping requester
						},
						// Initiate Direct Handshakes with people who want pings
						NodePacket::WantPing(requesting_node_id, requesting_net_id) => {
							// Connect to requested node
							self.action(NodeAction::ConnectDirect(requesting_node_id, requesting_net_id));
							// Attempt to send AcceptWantPing Packet WANT_PING_CONN_TIMEOUT ticks after initial connection request
							// This will fail if there is no session created within WANT_PING_CONN_TIMEOUT ticks (this is to prevent connections with very faraway nodes)
							let packet_action = NodeAction::Packet(requesting_node_id, NodePacket::AcceptWantPing(return_node_id))
								.gen_condition(NodeActionCondition::RunAfter(self.ticks + WANT_PING_CONN_TIMEOUT));
							self.action(packet_action);
						},
						/*NodePacket::RouteRequest(target_coord, max_distance, requester_coord, requester_node_id) => {
							// outgoing.push(value)
						},*/
						_ => { },
					}
				}
			}
		} else {
			return Err( PacketParseError::InvalidNetworkRecipient { from: received_packet.src_addr, intended_dest: received_packet.dest_addr } )
		}
		Ok(())
	}
}
