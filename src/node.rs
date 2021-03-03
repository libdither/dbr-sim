#[allow(unused_variables)]

const TARGET_PEER_COUNT: usize = 5;
// Amount of time to wait to connect to a peer who wants to ping
// const WANT_PING_CONN_TIMEOUT: usize = 300;
const MAX_REQUEST_PINGS: usize = 10;

use std::collections::{HashMap, BTreeMap};
use std::any::Any;

use nalgebra::{DMatrix, SymmetricEigen, Vector2};
use petgraph::graphmap::DiGraphMap;

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
	/// Attempt to calculate self route coordinate
	TryCalcRouteCoord,
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

#[derive(Debug, Default)]
pub struct Node {
	pub node_id: NodeID,
	pub net_id: InternetID,

	pub route_coord: Option<RouteCoord>,
	pub ticks: usize, // Amount of time passed since startup of this node

	pub remotes: HashMap<NodeID, RemoteNode>, // All remotes this node has ever connected to
	pub sessions: HashMap<SessionID, NodeID>, // All sessions that have ever been initialized
	pub node_list: BTreeMap<u64, NodeID>, // All nodes that have been tested, sorted by lowest value
	pub route_map: DiGraphMap<NodeID, u64>, // Bi-directional graph of all locally known nodes and the estimated distances between them
	// pub peered_nodes: PriorityQueue<SessionID, Reverse<RouteScalar>>, // Top subset of all 
	pub actions_queue: Vec<NodeAction>, // Actions will wait here until NodeID session is established
}
impl CustomNode for Node {
	type CustomNodeAction = NodeAction;
	fn net_id(&self) -> InternetID { self.net_id }
	fn tick(&mut self, incoming: Vec<InternetPacket>, _cheat_position: &Option<(i32, i32)>) -> Vec<InternetPacket> {
		let mut outgoing: Vec<InternetPacket> = Vec::new();
		//self.route_coord = cheat_position.map(|c|(c.0 as i64, c.1 as i64));

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
			..Default::default()
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
			NodeAction::TryCalcRouteCoord => {
				self.route_coord = Some(self.calculate_route_coord()?);
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
				self.action(NodeAction::Connect(remote_node_id, net_id, vec![NodePacket::ExchangeInfo(self.route_coord, 0, 0)])); // ExchangeInfo packet will be filled in dynamically
				// Test Direct connection
				//self.action(NodeAction::MaybeTestNode(remote_node_id).gen_condition(NodeActionCondition::Session(remote_node_id)));
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
				let distance = self.remote_mut(&return_node_id)?.session_mut()?.tracker.acknowledge_ping(ping_id, self_ticks)?;
				self.route_map.add_edge(self.node_id, return_node_id, distance);
				self.node_list.insert(distance, return_node_id);
				// Recursively parse packets
				for packet in packets {
					self.parse_node_packet(return_node_id, packet, outgoing)?;
				}
			}
			NodePacket::Ping(ping_id) => {
				self.remote(&return_node_id)?.add_packet(NodePacket::PingResponse(ping_id), outgoing)?;
			},
			NodePacket::PingResponse(ping_id) => {
				let distance = self.remote_mut(&return_node_id)?.session_mut()?.tracker.acknowledge_ping(ping_id, self_ticks)?;
				self.route_map.add_edge(self.node_id, return_node_id, distance);
			},
			NodePacket::ExchangeInfo(remote_route_coord, remote_peer_count, remote_ping) => {
				// Note dual-edge
				self.route_map.add_edge(return_node_id, self.node_id, remote_ping);

				let route_coord = self.route_coord;
				let peer_count = self.remotes.len();
				let remote = self.remote_mut(&return_node_id)?;
				let ping = remote.session()?.tracker.dist_avg;
				remote.route_coord = remote_route_coord; // Make note of routing coordinate if exists

				remote.add_packet(NodePacket::ExchangeInfoResponse(route_coord, peer_count, ping), outgoing)?;
				/*if remote_peer_count > 1 {
					self.action(NodeAction::MaybeTestNode(return_node_id));
				}*/
			},
			NodePacket::ExchangeInfoResponse(remote_route_coord, remote_peer_count, remote_ping) => {
				let self_node_count = self.node_list.len();
				
				// Note dual-edge
				self.route_map.add_edge(return_node_id, self.node_id, remote_ping);
				let remote = self.remote_mut(&return_node_id)?;
				remote.route_coord = remote_route_coord; // Make note of routing coordinate if exists

				let ping = remote.session()?.tracker.dist_avg;
				if remote_peer_count <= 1 && remote_route_coord.is_none() {
					remote.add_packet(NodePacket::ProposeRouteCoords((0,0), (0,ping as i64)), outgoing)?;
				} else {
					
					// If not at target, it is small network, attempt to calculate
					if self_node_count == remote_peer_count && self_node_count < TARGET_PEER_COUNT {
						self.action(NodeAction::TryCalcRouteCoord);
					} else if self_node_count < TARGET_PEER_COUNT { // If not found all targets on small network
						remote.add_packet(NodePacket::RequestPings(TARGET_PEER_COUNT), outgoing)?;
					} else { // It is large network
						self.action(NodeAction::TryCalcRouteCoord);
					}
				}
			},
			NodePacket::ProposeRouteCoords(route_coord_proposal, remote_route_coord_proposal) => {
				if None == self.route_coord {
					self.route_coord = Some(route_coord_proposal);
					let remote = self.remote_mut(&return_node_id)?;
					remote.route_coord = Some(remote_route_coord_proposal);
					remote.add_packet(NodePacket::ProposeRouteCoordsResponse(route_coord_proposal, remote_route_coord_proposal, true), outgoing)?;
				} else {
					let remote = self.remote_mut(&return_node_id)?;
					remote.add_packet(NodePacket::ProposeRouteCoordsResponse(route_coord_proposal, remote_route_coord_proposal, false), outgoing)?;
				}
			},
			NodePacket::ProposeRouteCoordsResponse(initial_remote_proposal, initial_self_proposal, accepted) => {
				if accepted {
					self.route_coord = Some(initial_self_proposal);
					self.remote_mut(&return_node_id)?.route_coord = Some(initial_remote_proposal);
				}
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

				//self.action(NodeAction::MaybeTestNode(return_node_id));
			},
			// Initiate Direct Handshakes with people who want pings
			NodePacket::WantPing(requesting_node_id, requesting_net_id) => {
				if let Some(time) = packet_last_received { if time < 300 { return Ok(()) } }
				let distance_self_to_return = self.remote(&return_node_id)?.session()?.tracker.dist_avg;
				if self.node_id != requesting_node_id && self.route_coord.is_some() {
					// Connect to requested node
					self.action(NodeAction::Connect(requesting_node_id, requesting_net_id, vec![NodePacket::AcceptWantPing(return_node_id, distance_self_to_return)]));
				} else { log::warn!("Node({}) received own WantPing", self.node_id); }
			},
			NodePacket::AcceptWantPing(intermediate_node_id, return_to_intermediate_distance) => {
				self.route_map.add_edge(return_node_id, intermediate_node_id, return_to_intermediate_distance);
				if let Some(time) = packet_last_received { if time < 300 { return Ok(()) } }

				let self_route_coord = self.route_coord;
				let self_node_count = self.node_list.len();
				let remote = self.remote(&return_node_id)?;
				//self.action(NodeAction::MaybeTestNode(return_node_id));
				remote.add_packet(NodePacket::ExchangeInfo(self_route_coord, self_node_count, remote.session()?.tracker.dist_avg), outgoing)?;
				//self.action(NodeAction::TryCalcRouteCoord);
				
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
						// Create session and acknowledge out-of-tracker ping
						let mut session = RemoteSession::from_id(session_id, return_net_id);
						let ping_id = session.tracker.gen_ping(time_sent_handshake);
						let distance = session.tracker.acknowledge_ping(ping_id, self_ticks)?;
						remote.session = Some(session); // update remote

						// Update packets
						let packets_to_send = self.update_connection_packets(acknowledger, packets_to_send)?;

						// Send connection packets
						self.remote_mut(&acknowledger)?.add_packet(NodePacket::ConnectionInit(return_ping_id, packets_to_send), outgoing)?;
						self.sessions.insert(session_id, acknowledger);

						self.node_list.insert(distance, acknowledger);
						self.route_map.add_edge(self.node_id, acknowledger, distance);
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
	fn update_connection_packets(&self, return_node_id: NodeID, packets: Vec<NodePacket>) -> Result<Vec<NodePacket>, NodeError> {
		let distance = self.remote(&return_node_id)?.session()?.tracker.dist_avg;
		Ok(packets.into_iter().map(|packet| match packet {
			NodePacket::ExchangeInfo(_,_,_) => {
				NodePacket::ExchangeInfo(self.route_coord, self.remotes.len(), distance)
			},
			_ => packet,
		}).collect::<Vec<NodePacket>>())
	}
	fn calculate_route_coord(&mut self) -> Result<RouteCoord, NodeError> {
		// TODO: Implement multidimensional scaling to calculate new route coordinates
		println!("node_list: {:?}", self);
		let nodes: Vec<(NodeID, RouteCoord)> = self.node_list.iter().filter_map(|(_,&n)|self.remote(&n).ok().map(|n|n.route_coord).flatten().map(|s|(n,s))).collect();
		let mat_size = nodes.len() + 1;
		
		println!("filtered_node_list: {:?}", nodes);
		let mut proximity_matrix = DMatrix::from_element(mat_size, mat_size, 0f64);
		
		// This is inefficient b.c. multiple vector creation but whatever
		let (mut first_row_insert, node_id_index): (Vec<u64>, Vec<NodeID>) = self.route_map.edges(self.node_id).map(|(_,n,&e)|(e,n)).unzip();
		first_row_insert.insert(0, 0);

		// Fill first row and collumn
		first_row_insert.iter().enumerate().for_each(|(i,&w)| {
			proximity_matrix[(0,i)] = w as f64;
			proximity_matrix[(i,0)] = w as f64;
		});

		node_id_index.iter().enumerate().for_each(|(i_y, id_y)|{
			node_id_index.iter().enumerate().for_each(|(i_x, id_x)|{
				let coord_x = self.remote(id_x).unwrap().route_coord.unwrap();
				let coord_y = self.remote(id_y).unwrap().route_coord.unwrap();
				let dist_vec = Vector2::new(coord_x.0 as f64, coord_x.1 as f64) - Vector2::new(coord_y.0 as f64,coord_y.1 as f64);
				let dist = dist_vec.norm();
				proximity_matrix[(i_y+1, i_x+1)] = dist;
				proximity_matrix[(i_x+1, i_y+1)] = dist;
			});
		});
		/*use nalgebra::Matrix4;
		let mat_size = 4;
		let proximity_matrix = Matrix4::new(
			0.,93.,82.,133.,
			93.,0.,52.,60.,
			82.,52.,0.,111.,
			133.,60.,111.,0.);*/

		println!("Proximity Matrix: {}", proximity_matrix);
		// Adapted from: http://citeseerx.ist.psu.edu/viewdoc/download?doi=10.1.1.495.4629&rep=rep1&type=pdf
		// STUPID STUPID STUPID WHY IS THERE NO POWER FUNCTION IN NALGEBRA LIBRARY WHY???? I SPENT 3 HOURS TRYING TO FIX THIS
		// TODO: use component_mul
		let proximity_squared = DMatrix::from_iterator(mat_size, mat_size, proximity_matrix.iter().map(|v|v*v)); 
		println!("Proximity Squared: {}", proximity_squared);
		
		let j_matrix = DMatrix::from_diagonal_element(mat_size, mat_size, 1.) - DMatrix::from_element(mat_size, mat_size, 1./mat_size as f64);
		
		let b_matrix = -0.5 * j_matrix.clone() * proximity_squared * j_matrix;
		
		// Calculate Eigenvectors and Eigenvalues and choose the 2 biggest ones
		let eigen = SymmetricEigen::try_new(b_matrix.clone(), 0., 0).unwrap();
		let eigenvalues: Vec<f64> = eigen.eigenvalues.data.as_vec().clone();
		let max_eigenvalue = eigenvalues.iter().enumerate().max_by(|(_,&ev1),(_,ev2)|ev1.partial_cmp(ev2).unwrap()).unwrap();
		let second_max_eigenvalue = eigenvalues.iter().enumerate().filter(|(i,_)|*i!=max_eigenvalue.0).max_by(|(_,&ev1),(_,ev2)|ev1.partial_cmp(ev2).unwrap()).unwrap();

		let top_eigenvalues = nalgebra::Matrix2::new(max_eigenvalue.1.abs().sqrt(), 0., 0., second_max_eigenvalue.1.abs().sqrt()); // Eigenvalue matrix
		let top_eigenvectors = DMatrix::from_fn(mat_size, 2, |r,c| if c==0 { eigen.eigenvectors[(r,max_eigenvalue.0)] } else { eigen.eigenvectors[(r,second_max_eigenvalue.0)] });
		let x_matrix = top_eigenvectors.clone() * top_eigenvalues; // Output, index 0 needs to be mapped to virtual routecoord coordinates based on other indices
		println!("{} * {} = {}", top_eigenvectors, top_eigenvalues, x_matrix);
		println!("Index: {:?}", node_id_index);

		let v1_routecoord = self.remote(&node_id_index[0])?.route_coord.unwrap();
		let v1 = Vector2::new(v1_routecoord.0 as f64, v1_routecoord.1 as f64);
		let v2_routecoord = self.remote(&node_id_index[1])?.route_coord.unwrap();
		let v2 = Vector2::new(v2_routecoord.0 as f64, v2_routecoord.1 as f64);
		use nalgebra::{U2, U1};
		let x1 = x_matrix.row(1).clone_owned().reshape_generic(U2,U1);
		println!("x1: {}", x1);
		println!("v1: {}, v2: {}", v1, v2);
		let x_shift = v1 - x1;
		let x1s = x1 + x_shift;
		let x2s = x_matrix.row(2).clone_owned().reshape_generic(U2,U1) + x_shift;
		let x3s = x_matrix.row(0).clone_owned().reshape_generic(U2,U1) + x_shift;
		println!("x1s: {}, x2s: {}", x1s, x2s);

		let xd = x1s - x2s;
		let vd = v1 - v2;
		let cos_a = (vd[1] + vd[0]) / (2. * xd[0]);
		let sin_a = (vd[1] - vd[0]) / (2. * xd[1]);
		//let a = atan2(sin_a, cos_a);
		
		use nalgebra::Matrix2;
		let rot = Matrix2::new(cos_a, -sin_a, sin_a, cos_a);
		//let shift = Vector2::new(0,0) - x_matrix[1];
		let v3_g = rot * x3s;
		println!("v3 guess: {}", v3_g);
		let distance_v2 = v3_g.metric_distance(&v2);
		let distance_v1 = v3_g.metric_distance(&v1);
		println!("dist from v3_guess to v1: {}, v2: {}", distance_v1, distance_v2);
		

		Ok((v3_g[0] as i64, v3_g[1] as i64))
		/* Ok((0,0)) */
	}
}
