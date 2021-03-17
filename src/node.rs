#[allow(unused_imports)]

const TARGET_PEER_COUNT: usize = 5;
// Amount of time to wait to connect to a peer who wants to ping
// const WANT_PING_CONN_TIMEOUT: usize = 300;
const MAX_REQUEST_PINGS: usize = 10;

use std::collections::{HashMap, BTreeMap};
use std::any::Any;

//use nalgebra::{DMatrix, SymmetricEigen, Vector2};
use petgraph::{graphmap::DiGraphMap, graph::Graph};
use bimap::BiHashMap;
use smallvec::SmallVec;
use nalgebra::Point2;

mod types;
mod session;
pub use types::{NodeID, SessionID, RouteCoord, NodePacket, NodeEncryption, RemoteNode, RemoteNodeError, RouteScalar};
use session::{SessionError, RemoteSession};
pub use crate::internet::{CustomNode, InternetID, InternetPacket, PacketVec};
use crate::{internet::InternetRequest, plot::GraphPlottable};

#[derive(Debug, Clone)]
/// A condition that should be satisfied before an action is executed
pub enum NodeActionCondition {
	/// Yields if there is a session of any kind with NodeID
	Session(NodeID),
	/// Yields if passed NodeID has a RouteCoord
	RemoteRouteCoord(NodeID),
	/// Yields if a time in the future has passed
	RunAt(usize), 
}
impl NodeActionCondition {
	// Returns true if condition is satisfied
	fn check(&self, node: &mut Node) -> Result<bool, NodeError> {
		Ok(match *self {
			// Yields None if there is a session active
			NodeActionCondition::Session(node_id) => node.remote(&node_id)?.session_active(),
			// Yields None if a specified amount of time has passed
			NodeActionCondition::RunAt(time) => node.ticks >= time,
			// Yield if this node has a routecoord
			NodeActionCondition::RemoteRouteCoord(node_id) => node.remote(&node_id).ok().map(|r|r.route_coord).flatten().is_some(),
			// Yields None if there is a session and it is direct
			/* NodeActionCondition::PeerSession(node_id) => {
				let remote = node.remote(&node_id)?;
				(remote.session_active() && remote.session()?.is_peer()).then(||self)
			},
			// Yields None if direct session is viable
			NodeActionCondition::PeerTested(node_id) => {
				let remote = node.remote_mut(&node_id)?;
				if remote.session_active() {
					remote.session_mut()?.tracker.is_viable().is_some().then(||self)
				} else { true.then(||self) }
			}, */
		})
	}
}
#[derive(Debug, Clone)]
pub enum NodeAction {
	/// Bootstrap this node onto a specific other network node, starts the self-organization process
	Bootstrap(NodeID, InternetID),
	/// Initiate Handshake with remote NodeID, InternetID and initial packets
	Connect(NodeID, InternetID, Vec<NodePacket>),
	/* /// Ping a node
	Ping(NodeID, usize), // Ping node X number of times
	/// Continually Ping remote until connection is deamed viable or unviable
	/// * `NodeID`: Node to test
	/// * `isize`: Timeout for Testing remotes
	TestNode(NodeID, isize),
	/// Test node if need new nodes
	MaybeTestNode(NodeID), */
	/// Run various functions pertaining to receiving specific information
	/// * `usize`: Number of direct connections a remote node has
	/// * `u64`: Ping from remote to me
	UpdateRemote(NodeID, Option<RouteCoord>, usize, u64),
	/// Request Peers of another node to ping me
	RequestPeers(NodeID, usize),
	/// Try and calculate route coordinate using Principle Coordinate Analysis of closest nodes (MDS)
	CalcRouteCoord,
	/// Exchange Info with another node
	ExchangeInformation(NodeID),
	/// Organize and set/unset known nodes as peers for Routing
	CalculatePeers,
	/// Sends a packet out onto the network for a specific recipient
	Traverse(NodeID, u64),
	/// Send DHT request for Route Coordinate
	RequestRouteCoord(NodeID),
	/// Establishes Routed session with remote NodeID
	/// Looks up remote node's RouteCoord on DHT and runs CalculateRoute after RouteCoord is received
	/// * `usize`: Number of intermediate nodes to route through
	/// * `f64`: Random intermediate offset (high offset is more anonymous but less efficient, very high offset is random routing strategy)
	ConnectRouted(NodeID, usize),
	/// Send specific packet to node
	Packet(NodeID, NodePacket),
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
type ActionVec = SmallVec<[NodeAction; 8]>;
#[derive(Default, Derivative)]
#[derivative(Debug)]
pub struct Node {
	pub node_id: NodeID,
	pub net_id: InternetID,

	pub route_coord: Option<RouteCoord>, // This node's route coordinate (None if not yet calculated)
	#[derivative(Debug="ignore")]
	deux_ex_data: Option<RouteCoord>,
	pub is_public: bool, // Does this node publish it's RouteCoord to the DHT?
	#[derivative(Debug="ignore")]
	public_route: Option<RouteCoord>,
	pub ticks: usize, // Amount of time passed since startup of this node

	pub remotes: HashMap<NodeID, RemoteNode>, // All remotes this node has ever connected to
	pub sessions: BiHashMap<SessionID, NodeID>, // Each SessionID links to a unique NodeID
	pub node_list: BTreeMap<u64, NodeID>, // All nodes that have been tested, sorted by lowest value
	pub peer_list: BiHashMap<NodeID, RouteCoord>, // Used for routing and peer management, peer count should be no more than TARGET_PEER_COUNT
	#[derivative(Debug="ignore")]
	pub route_map: DiGraphMap<NodeID, u64>, // Bi-directional graph of all locally known nodes and the estimated distances between them
	// pub peered_nodes: PriorityQueue<SessionID, Reverse<RouteScalar>>, // Top subset of all 
	pub action_list: ActionVec, // Actions will wait here until NodeID session is established
}
impl CustomNode for Node {
	type CustomNodeAction = NodeAction;
	fn net_id(&self) -> InternetID { self.net_id }
	fn tick(&mut self, incoming: PacketVec) -> PacketVec {
		let mut outgoing = PacketVec::new();

		// Parse Incoming Packets
		for packet in incoming {
			let (src_addr, dest_addr) = (packet.src_addr, packet.dest_addr);
			match self.parse_packet(packet, &mut outgoing) {
				Ok(Some((return_node_id, node_packet))) => {
					if let Err(err) = self.parse_node_packet(return_node_id, node_packet, &mut outgoing) {
						log::error!("Error in parsing NodePacket from NodeID({}) to NodeID({}): {:?}", return_node_id, self.node_id, anyhow::Error::new(err));
					}
				},
				Ok(None) => {},
				Err(err) => { log::error!("Error in parsing InternetPacket from InternetID({}) to InternetID({}): {:?}", src_addr, dest_addr, anyhow::Error::new(err)); println!("{:?}", self); }
			}
		}
		
		let mut new_actions = ActionVec::new(); // Create buffer for new actions
		let aq = std::mem::replace(&mut self.action_list, Default::default()); // Move actions out of action_list
		// Execute and collect actions back into action_list
		self.action_list = aq.into_iter().filter_map(|action|{
			let action_clone = action.clone();
			self.parse_action(action, &mut outgoing, &mut new_actions).unwrap_or_else(|err|{
				log::error!("NodeID({}), Action {:?} errored: {:?}", self.node_id, action_clone, err); None
			})
		}).collect();
		self.action_list.append(&mut new_actions); // Record new actions
		
		self.ticks += 1;
		outgoing
	}
	fn action(&mut self, action: NodeAction) { self.action_list.push(action); }
	fn as_any(&self) -> &dyn Any { self }
	fn set_deus_ex_data(&mut self, data: Option<RouteCoord>) { self.deux_ex_data = data; }
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
	#[error("There is no calculated route coordinate for this node")]
	NoCalculatedRouteCoord,
	#[error("There is no remote RouteCoord recorded for NodeID({remote:?})")]
	NoRemoteRouteCoord { remote: NodeID },
	#[error("Triggered RemoteNodeError")]
	RemoteNodeError(#[from] RemoteNodeError),
	#[error("Remote Session Error")]
	SessionError(#[from] SessionError),
	#[error("Failed to decode packet data")]
	SerdeDecodeError(#[from] serde_json::Error),
	#[error(transparent)]
    Other(#[from] anyhow::Error),
}

impl Node {
	pub fn new(node_id: NodeID, net_id: InternetID) -> Node {
		Node {
			node_id,
			net_id,
			is_public: true,
			..Default::default()
		}
	}
	pub fn with_action(mut self, action: NodeAction) -> Self { self.action_list.push(action); self }
	pub fn remote(&self, node_id: &NodeID) -> Result<&RemoteNode, NodeError> { self.remotes.get(node_id).ok_or(NodeError::NoRemoteError{node_id: *node_id}) }
	pub fn remote_mut(&mut self, node_id: &NodeID) -> Result<&mut RemoteNode, NodeError> { self.remotes.get_mut(node_id).ok_or(NodeError::NoRemoteError{node_id: *node_id}) }

	// Returns true if action should be deleted and false if it should not be
	pub fn parse_action(&mut self, action: NodeAction, outgoing: &mut PacketVec, out_actions: &mut ActionVec) -> Result<Option<NodeAction>, NodeError> {
		match action {
			// Bootstrap node onto the network
			NodeAction::Bootstrap(remote_node_id, net_id) => {
				out_actions.push(NodeAction::Connect(remote_node_id, net_id, vec![NodePacket::ExchangeInfo(self.route_coord, 0, 0)])); // ExchangeInfo packet will be filled in dynamically
			},
			// Connect to remote node
			NodeAction::Connect(remote_node_id, remote_net_id, ref packets) => {
				self.direct_connect(remote_node_id, remote_net_id, packets.clone(), outgoing);
			},
			NodeAction::UpdateRemote(remote_node_id, remote_route_coord, remote_direct_count, remote_ping) => {
				self.route_map.add_edge(remote_node_id, self.node_id, remote_ping);

				let self_route_coord = self.route_coord;
				
				// Record Remote Coordinate
				let remote = self.remote_mut(&remote_node_id)?;
				let mut did_route_change = remote.route_coord != remote_route_coord;
				remote.route_coord = remote_route_coord;

				// If this node has coord,
				if let None = self.route_coord {
					out_actions.push(NodeAction::CalcRouteCoord);
					did_route_change = false;
				}
				if did_route_change {
					out_actions.push(NodeAction::CalculatePeers);
				}
				// If need more peers & remote has a peer, request pings
				if self.node_list.len() < TARGET_PEER_COUNT && remote_direct_count >= 2 {
					self.remote_mut(&remote_node_id)?.add_packet(NodePacket::RequestPings(TARGET_PEER_COUNT, self_route_coord), outgoing)?;
				}
			}
			NodeAction::CalcRouteCoord => {
				self.route_coord = Some(self.calculate_route_coord()?);
				out_actions.push(NodeAction::CalculatePeers);
			},
			NodeAction::ExchangeInformation(remote_node_id) => {
				let remote = self.remote(&remote_node_id)?;
				remote.add_packet(NodePacket::ExchangeInfo(self.route_coord, self.peer_list.len(), remote.session()?.tracker.dist_avg), outgoing)?;
			},
			// Calculate peer_list, the somewhat permanent list of peers to connect to
			NodeAction::CalculatePeers => {
				// Collect the viable peers
				let self_route_coord = self.route_coord.ok_or(NodeError::NoCalculatedRouteCoord)?;
				let direct_nodes = self.node_list.iter().map(|s|*s.1).collect::<Vec<NodeID>>();
				self.peer_list = direct_nodes.iter().filter_map(|node_id| {
					let remote = self.remote(node_id).unwrap();
					// Decides whether remote should be added to peer list
					if let Some(route_coord) = remote.is_viable_peer(self_route_coord) { Some((*node_id, route_coord)) } else { None }
				}).take(TARGET_PEER_COUNT).collect();
				
				// Notify Peers if just became peer
				let num_peers = self.peer_list.len();
				for node_id in direct_nodes {
					let toggle = self.peer_list.contains_left(&node_id);
					let remote = self.remote_mut(&node_id)?;
					if !remote.session()?.is_peer() && toggle {
						let dist = remote.session()?.tracker.dist_avg;
						remote.add_packet(NodePacket::PeerNotify(0, self_route_coord, num_peers, dist), outgoing)?;
					} else {  }
					remote.session_mut()?.set_peer(toggle);
				}
				
				// If have enough peers & want to host node as public, write RouteCoord to DHT
				if self.peer_list.len() >= TARGET_PEER_COUNT && self.is_public && self.public_route != self.route_coord {
					self.public_route = self.route_coord;
					outgoing.push( InternetPacket::gen_request(self.net_id, InternetRequest::RouteCoordDHTWrite(self.node_id, self_route_coord)) );
				}
			},
			NodeAction::Traverse(remote_node_id, data) => {
				if let Ok(Some(remote_route_coord)) = self.remote(&remote_node_id).map(|n|n.route_coord) {
					let encryption = NodeEncryption::Traversal { recipient: remote_node_id, data, sender: self.node_id };
					let remote_route_coord_f64 = remote_route_coord.map(|s|s as f64);
					if let Some((min_node_id, _)) = self.peer_list.iter().min_by_key(|(_,p)|nalgebra::distance_squared(&p.map(|s|s as f64), &remote_route_coord_f64) as i64) {
						self.remote(min_node_id)?.add_packet(NodePacket::Traverse(remote_route_coord, Box::new(encryption)), outgoing)?;
					} else { log::error!("Could not find close node for Traverse action"); }
					return Ok(None);
				} else {
					out_actions.push(NodeAction::RequestRouteCoord(remote_node_id));
					out_actions.push(NodeAction::Traverse(remote_node_id, data).gen_condition(NodeActionCondition::RemoteRouteCoord(remote_node_id)));
				}
			},
			NodeAction::RequestRouteCoord(remote_node_id) => {
				outgoing.push(InternetPacket::gen_request(self.net_id, InternetRequest::RouteCoordDHTRead(remote_node_id)));
			},
			NodeAction::ConnectRouted(remote_node_id, hops) => {
				let self_route_coord = self.route_coord.ok_or(NodeError::NoCalculatedRouteCoord)?;
				// Check if Remote Route Coord was allready requested
				if let Ok(Some(remote_route_coord)) = self.remote(&remote_node_id).map(|n|n.route_coord) {
					let self_route_coord = self_route_coord.map(|s|s as f64);
					let remote_route_coord = remote_route_coord.map(|s|s as f64);
					let diff = (remote_route_coord - self_route_coord) / hops as f64;
					let mut routes = Vec::with_capacity(hops);
					for i in 1..hops {
						routes.push(self_route_coord + diff * i as f64);
					}
					println!("Routes: {:?}", routes);
					self.routed_connect(remote_node_id, outgoing);
					//self.remote_mut(&remote_node_id)?.connect_routed(routes);
				} else { // Otherwise, Request it and await Condition for next ConnectRouted
					out_actions.push(NodeAction::RequestRouteCoord(remote_node_id));
					out_actions.push(NodeAction::ConnectRouted(remote_node_id, hops).gen_condition(NodeActionCondition::RemoteRouteCoord(remote_node_id)));
				}
			},
			NodeAction::Packet(remote_node_id, ref packet) => {
				self.remote(&remote_node_id)?.add_packet(packet.clone(), outgoing)?;
			},
			NodeAction::Condition(condition, embedded_action) => {
				// Returns embedded action if condition is satisfied (e.g. check() returns true), else returns false to prevent action from being deleted
				if condition.check(self)? { return Ok(Some(*embedded_action)); } else { return Ok(Some(NodeAction::Condition(condition, embedded_action))); }
			}
			_ => { unimplemented!("Unimplemented Action") },
		}
		log::trace!("[{: >6}] NodeID({}) Completed Action: {:?}", self.ticks, self.node_id, action);
		Ok(None) // By default don't return action
	}
	pub fn parse_node_packet(&mut self, return_node_id: NodeID, received_packet: NodePacket, outgoing: &mut PacketVec) -> Result<(), NodeError> {
		log::debug!("[{: >6}] Node({}) received NodePacket::{:?} from NodeID({})", self.ticks, self.node_id, received_packet, return_node_id);
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
			NodePacket::ExchangeInfo(remote_route_coord, _remote_direct_count, remote_ping) => {
				if self.node_id == 0 && self.node_list.len() == 1 && self.route_coord.is_none() { self.route_coord = Some(self.calculate_route_coord()?); }

				// Note Data, Update Remote
				self.action(NodeAction::UpdateRemote(return_node_id, remote_route_coord, _remote_direct_count, remote_ping));

				// Send Return Packet
				let route_coord = self.route_coord;
				let peer_count = self.node_list.len();
				let remote = self.remote_mut(&return_node_id)?;
				let ping = remote.session()?.tracker.dist_avg;
				remote.add_packet(NodePacket::ExchangeInfoResponse(route_coord, peer_count, ping), outgoing)?;
			},
			NodePacket::ExchangeInfoResponse(remote_route_coord, remote_direct_count, remote_ping) => {
				self.action(NodeAction::UpdateRemote(return_node_id, remote_route_coord, remote_direct_count, remote_ping));
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
			NodePacket::RequestPings(requests, requester_route_coord) => {
				if let Some(time) = packet_last_received { if time < 2000 { return Ok(()) } } // Nodes should not be spamming this multiple times
				// Loop through first min(N,MAX_REQUEST_PINGS) items of priorityqueue
				let num_requests = usize::min(requests, MAX_REQUEST_PINGS); // Maximum of 10 requests

				self.remote_mut(&return_node_id)?.route_coord = requester_route_coord;
				let closest_nodes = if let Some(route_coord) = requester_route_coord {
					let point_target = route_coord.map(|s|s as f64);
					let mut sorted = self.node_list.iter().filter_map(|(&_,&id)|{
						if let Some(p) = self.remote(&id).unwrap().route_coord {
							Some((id, nalgebra::distance_squared(&p.map(|s|s as f64), &point_target) as u64))
						} else { None }
					}).collect::<Vec<(NodeID, u64)>>();
					sorted.sort_unstable_by_key(|k|k.1);
					sorted.iter().map(|s|s.0).take(num_requests).collect()
				} else {
					self.node_list.iter().map(|(_,&id)|id).take(num_requests).collect::<Vec<NodeID>>()
				};

				// Locate nearest peers to requester_route_coord
				
				// Send WantPing packet to first num_requests of those peers
				let want_ping_packet = NodePacket::WantPing(return_node_id, self.remote(&return_node_id)?.session()?.direct()?.net_id);
				for node_id in closest_nodes {
					let remote = self.remote(&node_id)?;
					if remote.node_id != return_node_id {
						remote.add_packet(want_ping_packet.clone(), outgoing)?;
					}
				}
			},
			// Initiate Direct Handshakes with people who want pings
			NodePacket::WantPing(requesting_node_id, requesting_net_id) => {
				// Only send WantPing if this node is usedful
				if self.node_id == requesting_node_id || self.route_coord.is_none() { return Ok(()) }
				let distance_self_to_return = self.remote(&return_node_id)?.session()?.tracker.dist_avg;

				let request_remote = self.remotes.entry(requesting_node_id).or_insert(RemoteNode::new(requesting_node_id));
				if let Ok(_request_session) = request_remote.session() { // If session, ignore probably
					return Ok(())
				} else { // If no session, send request
					if request_remote.pending_session.is_none() {
						self.action(NodeAction::Connect(requesting_node_id, requesting_net_id, vec![NodePacket::AcceptWantPing(return_node_id, distance_self_to_return)]));
					}
				}
			},
			NodePacket::AcceptWantPing(intermediate_node_id, return_to_intermediate_distance) => {
				self.route_map.add_edge(return_node_id, intermediate_node_id, return_to_intermediate_distance);
				if let Some(time) = packet_last_received { if time < 300 { return Ok(()) } }

				let self_route_coord = self.route_coord;
				let self_node_count = self.node_list.len();
				let remote = self.remote(&return_node_id)?;
				remote.add_packet(NodePacket::ExchangeInfo(self_route_coord, self_node_count, remote.session()?.tracker.dist_avg), outgoing)?;
			},
			// Receive notification that another node has found me it's closest
			NodePacket::PeerNotify(_rank, route_coord, peer_count, peer_distance) => {
				// Record peer rank
				//let session = self.remote_mut(&return_node_id)?.session_mut()?;
				//session.record_peer_notify(rank);
				// Update remote
				self.action(NodeAction::UpdateRemote(return_node_id, Some(route_coord), peer_count, peer_distance));
			},
			NodePacket::Traverse(ref target_route_coord, ref encrypted_data) => {
				let float_target_route_coord = target_route_coord.map(|s|s as f64);
				if let Some((&min_node_id, _)) = self.peer_list.iter().min_by_key(|(_,p)|nalgebra::distance_squared(&p.map(|s|s as f64), &float_target_route_coord) as i64) {
					if match **encrypted_data {
						NodeEncryption::Traversal { recipient, data, sender } => {
							if recipient == self.node_id {
								// If packet meant for me, log it
								log::info!("NodeID({}) Received Traverse packet with data: {} from NodeID({})", self.node_id, data, sender); false
							} else { true }
						}
						_ => { unimplemented!("Traverse doesn't support this NodeEncryption variant") }
					} {
						if min_node_id != return_node_id {
							self.remote(&min_node_id)?.add_packet(received_packet, outgoing)?; // Forward packet to nearest to destination
						} else { log::error!("Shoudn't sent Traverse back to sending node") }
					}
				} else { log::error!("Failed to find Minimum distance node"); }
			},
			_ => { },
		}
		Ok(())
	}

	/// Initiate handshake process and send packets when completed
	fn direct_connect(&mut self, dest_node_id: NodeID, dest_addr: InternetID, initial_packets: Vec<NodePacket>, outgoing: &mut PacketVec) {
		let session_id: SessionID = rand::random(); // Create random session ID
		//let self_node_id = self.node_id;
		let self_ticks = self.ticks;
		let remote = self.remotes.entry(dest_node_id).or_insert(RemoteNode::new(dest_node_id));
		remote.pending_session = Some(Box::new((session_id, self_ticks, initial_packets)));
		// TODO: public key encryption
		let encryption = NodeEncryption::Handshake { recipient: dest_node_id, session_id, signer: self.node_id };
		outgoing.push(encryption.package(dest_addr))
	}
	// Create multiple Routed Sessions that sequentially resolve their pending_route fields as Traversal Packets are acknowledged
	fn routed_connect(&mut self, dest_node_id: NodeID, outgoing: &mut PacketVec) {
		/*let session_id: SessionID = rand::random();
		let remote = self.remotes.entry(dest_node_id).or_insert(RemoteNode::new(dest_node_id));
		remote.pending_session = Some((session_id, usize::MAX, initial_packets));*/

	}
	/// Parses handshakes, acknowledgments and sessions, Returns Some(remote_net_id, packet_to_parse) if session or handshake finished
	fn parse_packet(&mut self, received_packet: InternetPacket, outgoing: &mut PacketVec) -> Result<Option<(NodeID, NodePacket)>, NodeError> {
		if received_packet.dest_addr != self.net_id { return Err(NodeError::InvalidNetworkRecipient { from: received_packet.src_addr, intended_dest: received_packet.dest_addr }) }

		if let Some(request) = received_packet.request {
			match request {
				InternetRequest::RouteCoordDHTReadResponse(query_node_id, route_option) => {
					if let Some(query_route_coord) = route_option {
						let remote = self.remotes.entry(query_node_id).or_insert(RemoteNode::new(query_node_id));
						remote.route_coord.get_or_insert(query_route_coord);
					} else {
						log::warn!("No Route Coordinate found for: {:?}", query_node_id);
					}
				},
				InternetRequest::RouteCoordDHTWriteResponse(_) => {},
				_ => { log::warn!("Not a InternetRequest Response variant") }
			}
			return Ok(None);
		}

		let return_net_id = received_packet.src_addr;
		let encrypted = NodeEncryption::unpackage(&received_packet)?;
		let self_ticks = self.ticks;
		let self_node_id = self.node_id;
		Ok(match encrypted {
			NodeEncryption::Handshake { recipient, session_id, signer } => {
				if recipient != self.node_id { Err(RemoteNodeError::UnknownAckRecipient { recipient })?; }
				let remote = self.remotes.entry(signer).or_insert(RemoteNode::new(signer));
				if remote.pending_session.is_some() {
					if self_node_id < remote.node_id { remote.pending_session = None }
				}
				let mut session = RemoteSession::from_address(session_id, return_net_id);
				let return_ping_id = session.tracker.gen_ping(self_ticks);
				remote.session = Some(session);
				outgoing.push(NodeEncryption::Acknowledge { session_id, acknowledger: recipient, return_ping_id }.package(return_net_id));
				self.sessions.insert(session_id, signer);
				log::debug!("[{: >6}] Node({:?}) Received Handshake: {:?}", self_ticks, self_node_id, encrypted);
				None
			},
			NodeEncryption::Acknowledge { session_id, acknowledger, return_ping_id } => {
				let mut remote = self.remote_mut(&acknowledger)?;
				if let Some(boxed_pending) = remote.pending_session.take() {
					let (pending_session_id, time_sent_handshake, packets_to_send) = *boxed_pending;
					
					if pending_session_id == session_id {
						// Create session and acknowledge out-of-tracker ping
						let mut session = RemoteSession::from_address(session_id, return_net_id);
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
						log::debug!("[{: >6}] Node({:?}) Received Acknowledgement: {:?}", self_ticks, self_node_id, encrypted);
						None
					} else { Err( RemoteNodeError::UnknownAck { passed: session_id } )? }
				} else { Err(RemoteNodeError::NoPendingHandshake)? }
			},
			NodeEncryption::Session { session_id, packet } => {
				let return_node_id = self.sessions.get_by_left(&session_id).ok_or(NodeError::UnknownSession {session_id} )?;
				Some((*return_node_id, packet))
			},
			_ => { unimplemented!(); }
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
		let route_coord = self.deux_ex_data.ok_or(NodeError::Other(anyhow!("no deus ex machina data")))?;
		log::debug!("NodeID({}) Calculated RouteCoord({})", self.node_id, route_coord);
		return Ok(route_coord);

		/* // TODO: Refactor this implementation of multidimensional scaling
		// println!("node_list: {:?}", self.remotes.iter().map(|(&id,n)|(id,n.route_coord)).collect::<Vec<(NodeID,Option<RouteCoord>)>>() );
		let nodes: Vec<(NodeID, RouteCoord)> = self.node_list.iter().filter_map(|(_,&node_id)|self.remote(&node_id).ok().map(|node|node.route_coord.map(|s|(node_id,s))).flatten()).collect();
		let mat_size = nodes.len() + 1;
		
		/* println!("filtered_node_list: {:?}", nodes); */
		let mut proximity_matrix = DMatrix::from_element(mat_size, mat_size, 0f64);
		
		// This is inefficient b.c. multiple vector creation but whatever
		let (mut first_row_insert, node_id_index): (Vec<u64>, Vec<NodeID>) = self.route_map.edges(self.node_id).filter_map(|(_,n,&e)|(e!=0).then(||(e,n))).unzip();
		first_row_insert.insert(0, 0);

		/* println!("first_row_insert: {:?}", first_row_insert);
		println!("node: {:?}", self);
		println!("route_map: {:#?}", self.route_map); */
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
		println!("Proximity Matrix: {}", proximity_matrix);
		// Algorithm for Multidimensional Scaling (MDS) Adapted from: http://citeseerx.ist.psu.edu/viewdoc/download?doi=10.1.1.495.4629&rep=rep1&type=pdf
		let proximity_squared = proximity_matrix.component_mul(&proximity_matrix); 
		
		let j_matrix = DMatrix::from_diagonal_element(mat_size, mat_size, 1.) - DMatrix::from_element(mat_size, mat_size, 1./mat_size as f64);
		
		let b_matrix = -0.5 * j_matrix.clone() * proximity_squared * j_matrix;
		
		// Calculate Eigenvectors and Eigenvalues and choose the 2 biggest ones
		let eigen = SymmetricEigen::try_new(b_matrix.clone(), 0., 0).unwrap();
		let eigenvalues: Vec<f64> = eigen.eigenvalues.data.as_vec().clone();
		let max_eigenvalue = eigenvalues.iter().enumerate().max_by(|(_,&ev1),(_,ev2)|ev1.partial_cmp(ev2).unwrap()).unwrap();
		let second_max_eigenvalue = eigenvalues.iter().enumerate().filter(|(i,_)|*i!=max_eigenvalue.0).max_by(|(_,&ev1),(_,ev2)|ev1.partial_cmp(ev2).unwrap()).unwrap();

		let top_eigenvalues = nalgebra::Matrix2::new(max_eigenvalue.1.abs().sqrt(), 0., 0., second_max_eigenvalue.1.abs().sqrt()); // Eigenvalue matrix
		let top_eigenvectors = DMatrix::from_fn(mat_size, 2, |r,c| if c==0 { eigen.eigenvectors[(r,max_eigenvalue.0)] } else { eigen.eigenvectors[(r,second_max_eigenvalue.0)] });
		let mut x_matrix = top_eigenvectors.clone() * top_eigenvalues; // Output, index 0 needs to be mapped to virtual routecoord coordinates based on other indices
		log::trace!("NodeID({}) x_matrix prediction = {}", self.node_id, x_matrix);
		/* if mat_size == 3 {
			x_matrix.row_iter_mut().for_each(|mut r|r[1] = -r[1]);
		}
		log::trace!("NodeID({}) x_matrix prediction flip = {}", self.node_id, x_matrix); */

		// Map MDS output to 2 RouteCoordinates
		// TODO: Refactor this messy code
		let v1_routecoord = self.remote(&node_id_index[0])?.route_coord.unwrap();
		let v1 = Vector2::new(v1_routecoord.0 as f64, v1_routecoord.1 as f64);
		let v2_routecoord = self.remote(&node_id_index[1])?.route_coord.unwrap();
		let v2 = Vector2::new(v2_routecoord.0 as f64, v2_routecoord.1 as f64);
		use nalgebra::{U2, U1};
		let x1 = x_matrix.row(1).clone_owned().reshape_generic(U2,U1);
		//println!("x1: {}", x1);
		//println!("v1: {}, v2: {}", v1, v2);
		let x_shift = v1 - x1;
		println!("x_shift: {}", x_shift);
		let x1s = x1 + x_shift;
		let x2s = x_matrix.row(2).clone_owned().reshape_generic(U2,U1) + x_shift;
		let x3s = x_matrix.row(0).clone_owned().reshape_generic(U2,U1) + x_shift;
		//println!("x1s: {}, x2s: {}, x3s: {}", x1s, x2s, x3s);

		let xd = x1s - x2s;
		let vd = v1 - v2;
		let cos_a = (vd[1] + vd[0]) / (2. * xd[0]);
		let sin_a = (vd[1] - vd[0]) / (2. * xd[1]);
		//println!("cos_a: {}, sin_a: {}", cos_a, sin_a);
		let a = f64::atan2(sin_a, cos_a);
		log::debug!("a = {}", a.to_degrees());
		
		use nalgebra::Matrix2;
		let rot = Matrix2::new(a.cos(), -a.sin(), a.sin(), a.cos());
		println!("matrix layout: {}", Matrix2::new(0,1,2,3));
		let v3_g = rot * x3s;
		
		log::info!("RouteCoord generated: {}", v3_g);
		Ok((v3_g[0] as i64, v3_g[1] as i64)) */
	}
}

use plotters::style::RGBColor;
impl GraphPlottable for Node {
	fn gen_graph(&self) -> Graph<(String, Point2<i32>), RGBColor> {
		for _node in self.route_map.nodes() {


		}
		/* let node_index self.node_list.iter().map(|(_, id)|self.remotes[id].)
		self.route_map.clone().into_graph().filter_map(|idx, node_id|{
			let remote = self.remote(&id).ok();
			remote.map(|r|r.route_coord.map(|c|(id, c)))
		}, |idx, _|{
			
		}) */
		Graph::with_capacity(0, 0)
	}
}