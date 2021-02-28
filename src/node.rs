#[allow(dead_code)]

const TARGET_PEER_COUNT: usize = 5;
// Amount of time to wait to connect to a peer who wants to ping
// const WANT_PING_CONN_TIMEOUT: usize = 300;
const MAX_REQUEST_PINGS: usize = 10;

use std::collections::{HashMap, BTreeMap};
//use std::cmp::Reverse;
use std::any::Any;

//use priority_queue::PriorityQueue;

pub use crate::internet::{CustomNode, InternetID, InternetPacket};

mod types;
mod session;
pub use types::{NodeID, SessionID, RouteCoord, NodePacket, NodeEncryption, RemoteNode, RemoteNodeError, RouteScalar};
use session::{SessionError, RemoteSession};

#[derive(Debug, Clone)]
/// A condition that should be satisfied before an action is executed
pub enum NodeActionCondition {
	/// Yields if there is a session of any kind with NodeID
	Session(NodeID),
	/// Yields if there is a PeerSession with NodeID
	PeerSession(NodeID), 
	/// Yields if node been considered as candidate for self.direct_node
	PeerTested(NodeID), 
	/// Yields if a time in the future has passed
	RunAt(usize), 
}
#[derive(Error, Debug)]
pub enum NodeActionConditionError {
    #[error("Node Error")]
	NodeError(#[from] NodeError),
	#[error("RemoteNode Error")]
	RemoteNodeError(#[from] RemoteNodeError),
}
impl NodeActionCondition {
	// Returns None if condition should be tested again, else returns Some(Self) if condition is passed
	fn test(self, node: &mut Node) -> Result<Option<Self>, NodeActionConditionError> {
		Ok(match self {
			// Yields if there is a session
			NodeActionCondition::Session(node_id) => node.remote(&node_id)?.session_active().then(||self),
			// Yields if there is a session and it is direct
			NodeActionCondition::PeerSession(node_id) => {
				let remote = node.remote(&node_id)?;
				(remote.session_active() && remote.session()?.is_peer()).then(||self)
			},
			// Yields if direct session is viable
			NodeActionCondition::PeerTested(node_id) => {
				let remote = node.remote_mut(&node_id)?;
				if remote.session_active() {
					remote.session_mut()?.tracker.is_viable().is_none().then(||self)
				} else { false.then(||self) }
			},
			// Yields if a specified amount of time has passed
			NodeActionCondition::RunAt(time) => (node.ticks >= time).then(||self),
			// Yields and runs nested action
		})
	}
}
#[derive(Debug, Clone)]
pub enum NodeAction {
	/// Initiate Handshake with remote NodeID, InternetID and initial packets
	Connect(NodeID, InternetID, Vec<NodePacket>),
	/// Ping a node
	Ping(NodeID, usize), // Ping node X number of times
	/// Continually Ping remote until connection is deamed viable or unviable
	/// * `NodeID`: Node to test
	/// * `isize`: Timeout for Testing remotes
	TestNode(NodeID, isize),
	/// Test node if need new nodes
	MaybeTestNode(NodeID),
	/// Request Peers of another node to ping me
	RequestPeers(NodeID, usize),
	/// Attempt to establish peership with another node
	/// Will not sent NotifyPeer if node list rank > TARGET_PEER_COUNT
	TryNotifyPeer(NodeID),
	/// Send specific packet to node
	Packet(NodeID, NodePacket),
	/// Request another nodes peers to make themselves known
	Bootstrap(NodeID, InternetID),
	/// Establish a dynamic routed connection
	// Route(NodeID, RouteCoord),
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

	pub route_coord: RouteCoord,
	pub ticks: usize, // Amount of time passed since startup of this node

	pub remotes: HashMap<NodeID, RemoteNode>, // All the remotes this node knows
	pub sessions: HashMap<SessionID, NodeID>, // All the sessions currently active
	pub node_list: BTreeMap<RouteScalar, NodeID>, // All tested nodes sorted by distance
	// pub peered_nodes: PriorityQueue<SessionID, Reverse<RouteScalar>>, // Top subset of all 
	pub actions_queue: Vec<NodeAction>, // Actions will wait here until NodeID session is established
}
impl CustomNode for Node {
	type CustomNodeAction = NodeAction;
	fn net_id(&self) -> InternetID { self.net_id }
	fn tick(&mut self, incoming: Vec<InternetPacket>) -> Vec<InternetPacket> {
		let mut outgoing: Vec<InternetPacket> = Vec::new();

		// Parse Incoming Packets
		for packet in incoming {
			//let mut noise = builder.local_private_key(self.keypair.)
			let (src_addr, dest_addr) = (packet.src_addr, packet.dest_addr);
			match self.parse_packet(packet, &mut outgoing) {
				Ok(Some((return_node_id, node_packet))) => {
					if let Err(err) = self.parse_node_packet(return_node_id, node_packet, &mut outgoing) {
						log::error!("Error in parsing NodePacket from NodeID({}) to NodeID({}): {:?}", return_node_id, self.node_id, err);
					}
				},
				Ok(None) => {},
				Err(err) => log::error!("Error in parsing InternetPacket from InternetID({}) to InternetID({}): {:?}", src_addr, dest_addr, err),
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
				_ => { log::trace!("[{: >4}] Node {} Done Action: {:?}", self.ticks, self.node_id, action); },
			}
		}
		
		self.ticks += 1;
		outgoing
	}
	fn action(&mut self, action: NodeAction) { self.actions_queue.push(action); }
	fn as_any(&self) -> &dyn Any { self }
}
#[derive(Error, Debug)]
pub enum NodeError {
    #[error("There is no known remote: {node_id:?}")]
	NoRemoteError { node_id: NodeID },
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


impl Node {
	pub fn new(node_id: NodeID, net_id: InternetID) -> Node {
		Node {
			node_id,
			net_id,

			route_coord: Default::default(),
			ticks: Default::default(),

			remotes: Default::default(),
			sessions: Default::default(),
			node_list: Default::default(),
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
			// Connect to remote node
			NodeAction::Connect(remote_node_id, remote_net_id, packets) => {
				// Insert RemoteNode if doesn't exist
				self.direct_connect(remote_node_id, remote_net_id, packets, outgoing);
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
			NodeAction::MaybeTestNode(remote_node_id) => {
				// If have active session
				if let Ok(session) = self.remote(&remote_node_id)?.session() {
					// If node is not currently being tested, and this node is not already tested
					if !session.is_testing && self.node_list.iter().find(|(_, &id)|id==remote_node_id).is_none() {
						// Test the node!
						self.action(NodeAction::TestNode(remote_node_id, 3000));
					}
				}
				
			}
			NodeAction::TestNode(remote_node_id, timeout) => {
				let self_node_id = self.node_id;

				let session = self.remote_mut(&remote_node_id)?.session_mut()?;

				let pending_pings = session.tracker.pending_pings();
				let test_results = session.test_direct();
				log::trace!("Node({}) Testing Node({}). Is viable: {:?},  pending pings: {:?}, ping_count: {:?}", self_node_id, remote_node_id, test_results, pending_pings, session.tracker.ping_count);
				
				let distance = session.tracker.distance();
				match test_results {
					// Need to ping more to get better test result
					None => {
						if pending_pings < 2 {
							self.action(NodeAction::Ping(remote_node_id, 2).gen_condition(NodeActionCondition::Session(remote_node_id)));
						}
						if timeout > 0 {
							self.action(NodeAction::TestNode(remote_node_id, timeout - 300).gen_condition(NodeActionCondition::RunAt(self.ticks + 300)));
						} else { log::warn!("Direct Test timed out: {:?}", action) }
					},
					// Test result comes back true or false. true 
					Some(status) => {
						if status {
							self.node_list.insert(distance, remote_node_id);
							// If close, send peer request
							if self.node_list.iter().take(TARGET_PEER_COUNT).find(|(_,&id)|id == remote_node_id).is_some() {
								self.action(NodeAction::TryNotifyPeer(remote_node_id));
								if let Some(node) = self.node_list.values().nth(TARGET_PEER_COUNT) {
									if self.remote(node)?.session()?.is_peer() {
										self.action(NodeAction::TryNotifyPeer(u32::MAX)); // Notify removal of old peers
									}
								}
								self.action(NodeAction::RequestPeers(remote_node_id, TARGET_PEER_COUNT))
							}
						}
						return Ok(true);
					}
				}
			},
			NodeAction::RequestPeers(remote_node_id, num_peers) => {
				self.remote_mut(&remote_node_id)?.add_packet(NodePacket::RequestPings(num_peers), outgoing)?;
			},
			NodeAction::TryNotifyPeer(remote_node_id) => {
				if let Some(rank) = self.node_list.iter().take(TARGET_PEER_COUNT).position(|(_,&id)|id == remote_node_id) {
					self.remote(&remote_node_id)?.add_packet(NodePacket::PeerNotify(rank), outgoing)?;
				}
			},
			NodeAction::Packet(remote_node_id, packet) => {
				// Send packet to remote
				self.remote(&remote_node_id)?.add_packet(packet, outgoing)?;
			},
			NodeAction::Bootstrap(remote_node_id, net_id) => {
				// Initiate secure connection
				self.action(NodeAction::Connect(remote_node_id, net_id, vec![]));
				// Test Direct connection
				self.action(NodeAction::MaybeTestNode(remote_node_id).gen_condition(NodeActionCondition::Session(remote_node_id)));
				// Ask for Pings
				// self.action(NodeAction::RequestPeers(remote_node_id, TARGET_PEER_COUNT/2).gen_condition(NodeActionCondition::PeerTested(remote_node_id)));
			},
			// NodeAction::Route(_remote_node_id, _remote_route_coord ) => {},
			// Embedded action is run in main loop
			NodeAction::Condition(condition, _) => {
				return Ok(condition.test(self)?.is_some());
			}
			//_ => { log::error!("Invalid NodeAction / NodeActionCondition pair"); },
		}
		Ok(true)
	}
	pub fn parse_node_packet(&mut self, return_node_id: NodeID, received_packet: NodePacket, outgoing: &mut Vec<InternetPacket>) -> Result<(), NodeError> {
		log::debug!("[{: >4}] Node({}) received NodePacket::{:?} from NodeID({})", self.ticks, self.node_id, received_packet, return_node_id);
		//let return_remote = self.remote_mut(&return_node_id)?;
		let self_ticks = self.ticks;
		let packet_last_received  = self.remote_mut(&return_node_id)?.session_mut()?.check_packet_time(&received_packet, return_node_id, self_ticks);
		match received_packet {
			NodePacket::ConnectionInit(ping_id, packets) => {
				// Acknowledge ping
				self.remote_mut(&return_node_id)?.session_mut()?.tracker.acknowledge_ping(ping_id, self_ticks)?;
				// Parse embedded packets
				for packet in packets {
					self.parse_node_packet(return_node_id, packet, outgoing)?;
				}
			}
			NodePacket::Ping(ping_id) => {
				// Return ping
				self.remote(&return_node_id)?.add_packet(NodePacket::PingResponse(ping_id), outgoing)?;
				
			},
			NodePacket::PingResponse(ping_id) => {
				// Acknowledge ping
				let session = self.remote_mut(&return_node_id)?.session_mut()?;
				session.tracker.acknowledge_ping(ping_id, self_ticks)?;
			},
			NodePacket::RequestPings(requests) => {

				if let Some(time) = packet_last_received { if time < 300 { return Ok(()) } }
				// Loop through first min(N,MAX_REQUEST_PINGS) items of priorityqueue
				let num_requests = usize::min(requests, MAX_REQUEST_PINGS); // Maximum of 10 requests

				let want_ping_packet = NodePacket::WantPing(return_node_id, self.remote(&return_node_id)?.session()?.return_net_id);
				for (_, node_id) in self.node_list.iter().take(num_requests) {
					// Generate packet sent to nearby remotes that this node wants to be pinged (excluding requester)
					let remote = self.remote(node_id)?;
					if remote.node_id != return_node_id {
						remote.add_packet(want_ping_packet.clone(), outgoing)?;
					}
				}

				self.action(NodeAction::MaybeTestNode(return_node_id));
			},
			// Initiate Direct Handshakes with people who want pings
			NodePacket::WantPing(requesting_node_id, requesting_net_id) => {
				if let Some(time) = packet_last_received { if time < 300 { return Ok(()) } }
				if self.node_id != requesting_node_id {
					// Connect to requested node
					self.action(NodeAction::Connect(requesting_node_id, requesting_net_id, vec![NodePacket::AcceptWantPing(return_node_id)]));
				} else { log::warn!("Node({}) received own WantPing", self.node_id) }
				
			},
			NodePacket::AcceptWantPing(_intermediate_node_id) => {
				if let Some(time) = packet_last_received { if time < 300 { return Ok(()) } }
				self.action(NodeAction::MaybeTestNode(return_node_id));
			},
			// Receive notification that another node has found me it's closest
			NodePacket::PeerNotify(rank) => {
				// Record peer rank
				let session = self.remote_mut(&return_node_id)?.session_mut()?;
				session.record_peer_notify(rank);
			}
			/*NodePacket::Traverse(target_route_coord, encrypted_data) => {
				// outgoing.push(value)
			},*/
			_ => { },
		}
		Ok(())
	}

	/// Initiate handshake process and send packets when completed
	fn direct_connect(&mut self, dest_node_id: NodeID, dest_addr: InternetID, packets: Vec<NodePacket>, outgoing: &mut Vec<InternetPacket>) {
		let session_id: SessionID = rand::random(); // Create random session ID
		//let self_node_id = self.node_id;
		let self_ticks = self.ticks;
		let remote = self.remotes.entry(dest_node_id).or_insert(RemoteNode::new(dest_node_id));
		remote.handshake_pending = Some((session_id, self_ticks, packets));
		// TODO: public key encryption
		let encryption = NodeEncryption::Handshake { recipient: dest_node_id, session_id, signer: self.node_id };
		outgoing.push(encryption.package(dest_addr))
	}
	/// Parses handshakes, acknowledgments and sessions, Returns Some(remote_net_id, packet_to_parse) if session or handshake finished
	fn parse_packet(&mut self, received_packet: InternetPacket, outgoing: &mut Vec<InternetPacket>) -> Result<Option<(NodeID, NodePacket)>, NodeError> {
		if received_packet.dest_addr != self.net_id { return Err(NodeError::InvalidNetworkRecipient { from: received_packet.src_addr, intended_dest: received_packet.dest_addr }) }

		let return_net_id = received_packet.src_addr;
		let encrypted = NodeEncryption::unpackage(&received_packet)?;
		let self_ticks = self.ticks;
		let self_node_id = self.node_id;
		Ok(match encrypted {
			NodeEncryption::Handshake { recipient, session_id, signer } => {
				if recipient != self.node_id { Err(RemoteNodeError::UnknownAckRecipient { recipient })?; }
				let remote = self.remotes.entry(signer).or_insert(RemoteNode::new(signer));
				if remote.handshake_pending.is_some() {
					if self_node_id < remote.node_id { remote.handshake_pending = None }
				}
				let mut session = RemoteSession::from_id(session_id, return_net_id);
				let return_ping_id = session.tracker.gen_ping(self_ticks);
				remote.session = Some(session);
				outgoing.push(NodeEncryption::Acknowledge { session_id, acknowledger: recipient, return_ping_id }.package(return_net_id));
				self.sessions.insert(session_id, signer);
				log::debug!("[{: >4}] Node({:?}) Received Handshake: {:?}", self_ticks, self_node_id, encrypted);
				None
			},
			NodeEncryption::Acknowledge { session_id, acknowledger, return_ping_id } => {
				let mut remote = self.remote_mut(&acknowledger)?;
				if let Some((pending_session_id, time_sent_handshake, packets_to_send)) = remote.handshake_pending.take() {
					if pending_session_id == session_id {
						remote.handshake_pending = None;
						let mut session = RemoteSession::from_id(session_id, return_net_id);
						// Note ping
						let ping_id = session.tracker.gen_ping(time_sent_handshake);
						session.tracker.acknowledge_ping(ping_id, self_ticks)?;
						remote.session = Some(session);
						// Send return packets
						remote.add_packet(NodePacket::ConnectionInit(return_ping_id, packets_to_send), outgoing)?;
						self.sessions.insert(session_id, acknowledger);
						log::debug!("[{: >4}] Node({:?}) Received Acknowledgement: {:?}", self_ticks, self_node_id, encrypted);
						None
					} else { Err( RemoteNodeError::UnknownAck { passed: session_id } )? }
				} else { Err(RemoteNodeError::NoPendingHandshake)? }
			},
			NodeEncryption::Session { session_id, packet } => {
				let return_node_id = self.sessions.get(&session_id).ok_or(NodeError::UnknownSession {session_id} )?;
				Some((*return_node_id, packet))
			},
		})
	}
}
