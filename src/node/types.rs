use crate::internet::{InternetID, InternetPacket, PacketVec};

pub use crate::node::session::{RemoteSession, SessionError, SessionType};
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

/// Packets that are sent between nodes in this protocol.
#[derive(Serialize, Deserialize, Debug, Clone)]
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
	ExchangeInfoResponse(Option<RouteCoord>, usize, u64),

	/// Propose routing coordinates if nobody has any nodes
	ProposeRouteCoords(RouteCoord, RouteCoord), // First route coord = other node, second route coord = myself
	ProposeRouteCoordsResponse(RouteCoord, RouteCoord, bool), // Proposed route coords (original coordinates, orientation), bool = true if acceptable

	/// ### Self-Organization System
	/// Request a certain number of another node's peers that are closest to this node to make themselves known
	/// * `usize`: Number of peers requested
	/// * `Option<RouteCoord>`: Route Coordinates of the other node if it has one
	RequestPings(usize, Option<RouteCoord>),

	/// Tell a peer that this node wants a ping (implying a potential direct connection)
	WantPing(NodeID, InternetID),
	/// Sent when node accepts a WantPing Request
	/// * `NodeID`: NodeID of Node who send the request in response to a RequestPings
	/// * `u64`: Distance to that node
	AcceptWantPing(NodeID, u64),

	/// Notify another node of peership
	/// * `usize`: Rank of how close peer is compared to other nodes, usize::MAX signifies no longer consider node
	PeerNotify(usize),

	/// Packet Traversal
	/// Represents a network traversal packet, It is routed through the network via it's RouteCoord
	Traverse(RouteCoord, Box<NodeEncryption>),

	/// Request a session that is routed through node to another RouteCoordinate
	RoutedSessionRequest(RouteCoord),
	RoutedSessionAccept(),

	/* /// Request to establish a peer as an intermediate node
	/// RouteCoord: Area where intermediate node is requested
	/// u64: Radius of request (how far away can request be deviated)
	/// RouteCoord: Requester's coordinates
	/// NodeID: Requester's NodeID (signed)
	//RouteRequest(RouteCoord, u64, RouteCoord, NodeID),
	/// Node that accepts request returns this and a RouteSession is established
	/// RouteCoord: Accepting node's coordinates
	/// NodeID: Accepting node's public key (signed and encrypted with requesting node's public key)
	// RouteAcccept(RouteCoord, NodeID), */
}
pub const NUM_NODE_PACKETS: usize = 10;

impl NodePacket {
	pub fn encrypt(self, session_id: SessionID) -> NodeEncryption { NodeEncryption::Session { session_id, packet: self } }
}
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
#[derive(Debug)]
pub struct RemoteNode {
	pub node_id: NodeID, // The ID of the remote node
	pub route_coord: Option<RouteCoord>,
	pub handshake_pending: Option<(SessionID, usize, Vec<NodePacket>)>, // is Some(pending_session_id, time_sent_handshake, packets_to_send)
	pub session: Option<RemoteSession>, // Session object, is None if no connection is active
}
impl RemoteNode {
	pub fn new(node_id: NodeID) -> Self {
		Self {
			node_id,
			route_coord: None,
			handshake_pending: None,
			session: None,
		}
	}
	pub fn session_active(&self) -> bool {
		self.session.is_some() && self.handshake_pending.is_none()
	}
	pub fn session(&self) -> Result<&RemoteSession, RemoteNodeError> {
		self.session.as_ref().ok_or( RemoteNodeError::NoSessionError { node_id: self.node_id } )
	} 
	pub fn session_mut(&mut self) -> Result<&mut RemoteSession, RemoteNodeError> {
		self.session.as_mut().ok_or( RemoteNodeError::NoSessionError { node_id: self.node_id } )
	}
	/// Wrap packet and push to `outgoing` Vec
	pub fn add_packet(&self, packet: NodePacket, outgoing: &mut PacketVec) -> Result<(), RemoteNodeError> {
		Ok(outgoing.push(self.session()?.gen_packet(packet)?))
	}
	/// Check if a peer is viable or not
	pub fn is_viable_peer(&self, _self_route_coord: RouteCoord) -> Option<RouteCoord> {
		if let (Some(route_coord), Some(_)) = (self.route_coord, &self.session) {
			//let avg_dist = session.tracker.dist_avg;
			//let route_dist = nalgebra::distance(route_coord.map(|s|s as f64), self_route_coord.map(|s|s as f64));
			return Some(route_coord.clone());
			//if self.session.is_some() && ()
		} else { None }
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
	Traversal { recipient: NodeID, data: u64, sender: NodeID },
}

impl NodeEncryption {
	pub fn package(&self, dest_addr: InternetID) -> InternetPacket {
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
}