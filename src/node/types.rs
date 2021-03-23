use crate::node::NodeError;
use crate::Node;
use crate::internet::{NetAddr, InternetPacket};

pub use crate::node::session::{RemoteSession, SessionError, SessionType, RoutedSession};
use crate::node::session::PingID;

use thiserror::Error;
use nalgebra::Point2;

/// Hash uniquely identifying a node (represents the Multihash of the node's Public Key)
pub type NodeID = u32;
/// Number uniquely identifying a session, represents a Symmetric key
pub type SessionID = u32;
/// Coordinate that represents a position of a node relative to other nodes in 2D space.
pub type RouteScalar = u64;
pub type RouteCoord = Point2<i64>;

/// Data structure that represents a NodeEncryption traversing through the network 
#[derive(Debug, Serialize, Deserialize, Clone)]
pub struct TraversalPacket {
	/// Place to route packet to
	pub destination: RouteCoord,
	/// Encrypted Session Data
	pub encryption: NodeEncryption,
	/// Signed & Assymetrically encrypted return location
	pub origin: Option<RouteCoord>,
}
impl TraversalPacket {
	pub fn new(destination: RouteCoord, encryption: NodeEncryption, origin: Option<RouteCoord>) -> NodePacket {
		NodePacket::Traverse(Box::new( TraversalPacket { destination, encryption, origin } ))
	}
}

/// Packets that are sent between nodes in this protocol.
#[derive(Debug, Serialize, Deserialize, Clone)]
pub enum NodePacket {
	/// ### Connection System
	/// Sent immediately after receiving a an Acknowledgement, allows other node to get a rough idea about the node's latency
	/// Contains list of packets for remote to respond to 
	ConnectionInit(PingID, Vec<NodePacket>),

	/// ### Information Exchange System
	/// Send info to another peer in exchange for their info
	/// * `Option<RouteCoord>`: Tell another node my Route Coordinate if I have it
	/// * `usize`: number of direct connections I have
	/// * `u64`: ping (latency) to remote node
	ExchangeInfo(Option<RouteCoord>, usize, u64), // My Route coordinate, number of peers, remote ping
	/// Send info in response to an ExchangeInfo packet
	/// * `Option<RouteCoord>`: Tell another node my Route Coordinate if I have it
	/// * `usize`: number of direct connections I have
	/// * `u64`: ping (latency) to remote node
	ExchangeInfoResponse(Option<RouteCoord>, usize, u64),
	/// Notify another node of peership
	/// * `usize`: Rank of remote in peer list
	/// * `RouteCoord`: My Route Coordinate
	/// * `usize`: Number of peers I have
	PeerNotify(usize, RouteCoord, usize, u64),
	/// Propose routing coordinates if nobody has any nodes
	ProposeRouteCoords(RouteCoord, RouteCoord), // First route coord = other node, second route coord = myself
	/// Proposed route coords (original coordinates, orientation, bool), bool = true if acceptable
	ProposeRouteCoordsResponse(RouteCoord, RouteCoord, bool), 

	/// ### Self-Organization System
	/// Request a certain number of another node's peers that are closest to this node to make themselves known
	/// * `usize`: Number of peers requested
	/// * `Option<RouteCoord>`: Route Coordinates of the other node if it has one
	RequestPings(usize, Option<RouteCoord>),

	/// Tell a peer that this node wants a ping (implying a potential direct connection)
	WantPing(NodeID, NetAddr),
	/// Sent when node accepts a WantPing Request
	/// * `NodeID`: NodeID of Node who send the request in response to a RequestPings
	/// * `u64`: Distance to that nodeTraversalPacket
	AcceptWantPing(NodeID, u64),

	/// Packet Traversal
	/// Represents a packet that is traversed through the network to it's destination using a RouteCoord
	Traverse(Box<TraversalPacket>),

	/// Request a session that is routed through node to another RouteCoordinate
	RoutedSessionRequest(RouteCoord),
	RoutedSessionAccept(),
}

pub const NUM_NODE_PACKETS: usize = 10;

#[derive(Error, Debug)]
pub enum RemoteNodeError {
    #[error("There is no active session with the node: {node_id:?}")]
	NoSessionError { node_id: NodeID },
	#[error("Session ID passed: {passed:?} does not match existing session ID")]
    UnknownAck { passed: SessionID },
	#[error("Unknown Acknowledgement Recipient: {recipient:?}")]
    UnknownAckRecipient { recipient: NodeID },
	#[error("Received Acknowledgement even though there are no pending handshake requests")]
	NoPendingHandshake,
	#[error("Session Error")]
	SessionError(#[from] SessionError),
}
#[derive(Debug, Derivative)]
#[derivative(Hash, PartialEq, Eq)]
pub struct RemoteNode {
	// The ID of the remote node
	pub node_id: NodeID,
	// Received Route Coordinate of the Remote Node
	#[derivative(PartialEq="ignore", Hash="ignore")]
	pub route_coord: Option<RouteCoord>,
	// If handshake is pending: Some(pending_session_id, time_sent_handshake, packets_to_send)
	#[derivative(PartialEq="ignore", Hash="ignore")]
	pub pending_session: Option<Box< (SessionID, usize, Vec<NodePacket>) >>,
	// If route is pending: Some(search location route coords, NodeIDs found willing to create RoutedSessions in search location)
	#[derivative(PartialEq="ignore", Hash="ignore")]
	pub pending_route: Option<Vec<(RouteCoord, Option<NodeID>)>>,
	// Contains Session details if session is connected
	#[derivative(PartialEq="ignore", Hash="ignore")]
	pub session: Option<RemoteSession>, // Session object, is None if no connection is active
}
impl RemoteNode {
	pub fn new(node_id: NodeID) -> Self {
		Self {
			node_id,
			route_coord: None,
			pending_session: None,
			pending_route: None,
			session: None,
		}
	}
	pub fn session_active(&self) -> bool {
		self.session.is_some() && self.pending_session.is_none()
	}
	pub fn session(&self) -> Result<&RemoteSession, RemoteNodeError> {
		self.session.as_ref().ok_or( RemoteNodeError::NoSessionError { node_id: self.node_id } )
	} 
	pub fn session_mut(&mut self) -> Result<&mut RemoteSession, RemoteNodeError> {
		self.session.as_mut().ok_or( RemoteNodeError::NoSessionError { node_id: self.node_id } )
	}
	/// Check if a peer is viable or not
	// TODO: Create condition that rejects nodes if there is another closer node located in a specific direction
	pub fn is_viable_peer(&self, _self_route_coord: RouteCoord) -> Option<RouteCoord> {
		if let (Some(route_coord), Some(session)) = (self.route_coord, &self.session) {
			//let avg_dist = session.tracker.dist_avg;
			//let route_dist = nalgebra::distance(route_coord.map(|s|s as f64), self_route_coord.map(|s|s as f64));
			if session.direct().is_ok() {
				return Some(route_coord.clone());
			} else { None }
		} else { None }
	}
	
	/// Generate NodeEncryption from NodePacket doing whatever needs to be done to route it through the network securely
	pub fn gen_packet(&self, packet: NodePacket, node: &Node) -> Result<InternetPacket, NodeError> {
		let session = self.session()?;
		let encryption = session.wrap_session(packet);

		Ok(session.gen_packet(encryption, node)?)
	}
}

#[derive(Serialize, Deserialize, Debug, Clone)]
pub enum NodeEncryption {
	/// Handshake is sent from node wanting to establish secure tunnel to another node
	/// session_id and signer are encrypted with recipient's public key
	Handshake { recipient: NodeID, session_id: SessionID, signer: NodeID },
	/// When the other node receives the Handshake, they will send back an Acknowledge
	/// When the original party receives the Acknowledge, that tunnel may now be used for 2-way packet transfer
	/// acknowledger and return_ping_id are symmetrically encrypted with session key
	Acknowledge { session_id: SessionID, acknowledger: NodeID, return_ping_id: PingID },
	/// Symmetrically Encrypted Data transfer (packet is encrypted with session key)
	Session { session_id: SessionID, packet: NodePacket },
	// Asymmetrically Encrypted notification (Data and Sender are encrypted with recipient's public key)
	Notify { recipient: NodeID, data: u64, sender: NodeID },
	// Signed Route Request, treated as a Notify type but requests a return Routed Session from the remote
	Request { recipient: NodeID, requester: NodeID }
}

impl NodeEncryption {
	pub fn package(&self, dest_addr: NetAddr) -> InternetPacket {
		InternetPacket {
			src_addr: 0, // This should get filled in automatically for all outgoing packets
			data: serde_json::to_vec(&self).expect("Failed to encode json"),
			dest_addr,
			request: None,
		}
	}
	pub fn unpackage(packet: &InternetPacket) -> Result<Self, serde_json::Error> {
		serde_json::from_slice(&packet.data)
	}
	/* pub fn wrap_traverse(self, session_id: SessionID, route_coord: RouteCoord) -> NodeEncryption {
		let packet = NodePacket::Traverse(route_coord, Box::new(self));
		NodeEncryption::Session { session_id, packet }
	} */
	pub fn is_for_node(&self, node: &crate::node::Node) -> bool {
		use NodeEncryption::*;
		match *self {
			Handshake { recipient, session_id:_, signer:_ } => node.node_id == recipient,
			Acknowledge { session_id, ref acknowledger, return_ping_id:_ } => {
				node.remote(acknowledger).map_or(false, |r|
					(r.pending_session.as_ref()).map_or(false, |b|b.0 == session_id)
				)
			},
			Session { session_id, packet:_ } => node.sessions.contains_left(&session_id),
			Notify { recipient, data:_, sender:_ } => node.node_id == recipient,
			Request { recipient, requester:_ } => node.node_id == recipient,
		}
	}
}