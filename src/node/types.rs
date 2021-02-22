use crate::internet::{InternetID, InternetPacket};

use std::cmp::Reverse;

use ta::{indicators::SimpleMovingAverage, Next};
use thiserror::Error;
use priority_queue::PriorityQueue;

/// Hash uniquely identifying a node (represents the Multihash of the node's Public Key)
pub type NodeID = u32;
/// Number uniquely identifying a session, represents a Symmetric key
pub type SessionID = u32;
/// Coordinate that represents a position of a node relative to other nodes in 2D space.
pub type RouteScalar = u64;
pub type RouteCoord = (RouteScalar, RouteScalar);
/// Number that uniquely identifies a ping request so that multiple Pings may be sent at the same time
type PingID = u64;

const MAX_PENDING_PINGS: usize = 25;

/// Packets that are sent between nodes in this protocol.
#[derive(Serialize, Deserialize, Debug, Clone)]
pub enum NodePacket {
	/// Sent to other Nodes. Expects PingResponse returned
	Ping(PingID), // Random number uniquely identifying this ping request
	/// PingResponse packet, time between Ping and PingResponse is measured
	PingResponse(PingID), // Acknowledge Ping(u64), sends back originally sent number

	/// Request Direct Connection
	DirectRequest,
	/// Direct Connection Response
	DirectResponse,

	/// Request to a peer for them to request their peers to ping me
	RequestPings(usize), // usize: max number of pings

	/// Tell a peer that this node wants a ping (implying a potential direct connection)
	WantPing(NodeID, InternetID),
	/// Sent when node accepts a WantPing Request
	/// NodeID: NodeID of Node who send the request in response to a RequestPings
	AcceptWantPing(NodeID),

	/// Represents a network traversal packet, It is routed through the network via it's RouteCoord
	/// Vec<u8>: Represents encrypted data meant for a specific node
	Traverse(RouteCoord, Vec<u8>),

	/// Request to establish a peer as an intermediate node
	/// RouteCoord: Area where intermediate node is requested
	/// u64: Radius of request (how far away can request be deviated)
	/// RouteCoord: Requester's coordinates
	/// NodeID: Requester's NodeID (signed)
	RouteRequest(RouteCoord, u64, RouteCoord, NodeID),
	/// Node that accepts request returns this and a RouteSession is established
	/// RouteCoord: Accepting node's coordinates
	/// NodeID: Accepting node's public key (signed and encrypted with requesting node's public key)
	RouteAccept(RouteCoord, NodeID),
}
impl NodePacket {
	pub fn encrypt(self, session_id: SessionID) -> NodeEncryption { NodeEncryption::Session { session_id, packet: self } }
}

#[derive(Error, Debug)]
pub enum SessionError {
    #[error("There is no previous ping sent out with ID: {ping_id:?} or ping was forgotten")]
	UnknownPingID { ping_id: PingID },
}

/// Represents session that is routed directly (through the internet)
#[derive(Derivative)]
#[derivative(Debug)]
pub struct DirectSession {
	pub net_id: InternetID, // Internet Address
	#[derivative(Debug="ignore")]
	ping_queue: PriorityQueue<PingID, Reverse<usize>>, // Tuple represents (ID of ping, priority by reversed time sent) 
	dist_avg: RouteScalar,
	dist_dev: RouteScalar,
	#[derivative(Debug="ignore")]
	ping_avg: SimpleMovingAverage, // Moving average of ping times
	#[derivative(Debug="ignore")]
	ping_dev: ta::indicators::StandardDeviation,
}
impl DirectSession {
	fn new(net_id: InternetID) -> Self {
		Self {
			net_id,
			ping_queue: PriorityQueue::with_capacity(MAX_PENDING_PINGS),
			dist_avg: 0,
			dist_dev: 0,
			ping_avg: SimpleMovingAverage::new(10).unwrap(),
			ping_dev: ta::indicators::StandardDeviation::new(10).unwrap(),
		}
	}
	// Generate Ping Packet
	pub fn gen_ping(&mut self, gen_time: usize) -> NodePacket {
		let ping_id: PingID = rand::random();
		self.ping_queue.push(ping_id, Reverse(gen_time));
		// There shouldn't be more than 25 pings pending
		if self.ping_queue.len() >= MAX_PENDING_PINGS {
			self.ping_queue.pop();
		}
		NodePacket::Ping(ping_id)
	}
	// Acknowledge Ping Response packet
	pub fn acknowledge_ping(&mut self, ping_id: PingID, current_time: usize) -> Result<RouteScalar, SessionError> {
		if let Some(( _, Reverse(time_sent) )) = self.ping_queue.remove(&ping_id) {
			let round_trip_time = current_time - time_sent;
			let distance = round_trip_time as f64 / 2.0;
			self.dist_avg = self.ping_avg.next(distance) as RouteScalar;
			self.dist_dev = self.ping_dev.next(distance) as RouteScalar;
			Ok(self.dist_avg)
		} else { Err(SessionError::UnknownPingID { ping_id }) }
	}
	pub fn distance(&self) -> RouteScalar {
		self.dist_avg
	}
	pub fn is_viable(&self) -> Option<bool> {
		if self.ping_queue.len() >= 5 {
			Some(self.dist_dev < 1)
		} else { None }
	}
	pub fn pending_pings(&self) -> usize { self.ping_queue.len() }
}
/// Represents session that is routed through alternate nodes
#[derive(Debug)]
pub struct RoutedSession {
	remote_route: RouteCoord,
	intermediate_nodes: Vec<(NodeID, RouteCoord)>,
}

#[derive(Debug)]
pub enum SessionType {
	// Directly connected
	Direct(DirectSession),
	// Routed session
	Routed(RoutedSession),
	// Return to sender connection
	Return,
}

#[derive(Debug)]
pub struct RemoteSession {
	pub session_id: SessionID, // All connections must have a SessionID for encryption
	pub session_type: SessionType, //  Sessions can either be Routed through other nodes or Directly Connected
}
impl RemoteSession {
	pub fn default() -> Self { Self {session_id: rand::random(), session_type: SessionType::Return } }
	pub fn with_direct(net_id: InternetID) -> Self { Self { session_id: rand::random(), session_type: SessionType::Direct(DirectSession::new(net_id))} }
	pub fn from_session(session_id: SessionID) -> Self { Self { session_id, session_type: SessionType::Return } }

	pub fn encrypt(&self, packet: NodePacket) -> NodeEncryption {
		NodeEncryption::Session { session_id: self.session_id, packet }
	}
	pub fn upgrade_direct(&mut self, net_id: InternetID) {
		if let SessionType::Return = self.session_type {
			self.session_type = SessionType::Direct(DirectSession::new(net_id));
		}
	}
	pub fn is_direct(&self) -> bool { match self.session_type { SessionType::Direct(_) => true, _ => false} }
	pub fn direct(&self) -> Result<&DirectSession, RemoteNodeError> { match &self.session_type { SessionType::Direct(direct) => Ok(direct), _ => Err(RemoteNodeError::NoDirectSessionError) } }
	pub fn direct_mut(&mut self) -> Result<&mut DirectSession, RemoteNodeError> { match &mut self.session_type { SessionType::Direct(direct) => Ok(direct), _ => Err(RemoteNodeError::NoDirectSessionError), } }
}

#[derive(Debug)]
pub struct RemoteNode {
	pub node_id: NodeID, // The ID of the remote node
	handshake_pending: bool,
	session: Option<RemoteSession>, // Session object, is None if no connection is active
}

#[derive(Error, Debug)]
pub enum RemoteNodeError {
    #[error("There is no active session with the node: {node_id:?}")]
	NoSessionError { node_id: NodeID },
	#[error("Session ID passed: {passed:?} does not match existing session ID")]
    InvalidSessionError { passed: SessionID },
    #[error("Cannot package packet because RemoteSession type is not DirectSession")]
	NoDirectSessionError,
}
impl RemoteNode {
	pub fn new(node_id: NodeID) -> Self {
		Self {
			node_id,
			handshake_pending: false,
			session: None,
		}
	}
	pub fn session_active(&self) -> bool {
		self.session.is_some() && !self.handshake_pending
	}
	pub fn session(&self) -> Result<&RemoteSession, RemoteNodeError> {
		self.session.as_ref().ok_or( RemoteNodeError::NoSessionError { node_id: self.node_id } )
	} 
	pub fn session_mut(&mut self) -> Result<&mut RemoteSession, RemoteNodeError> {
		self.session.as_mut().ok_or( RemoteNodeError::NoSessionError { node_id: self.node_id } )
	}
	/// This function creates a NodeEncryption::Handshake object to be sent to a peer that secure communication should be established with
	pub fn gen_handshake(&mut self, my_node_id: NodeID, session: RemoteSession) -> NodeEncryption {
		self.handshake_pending = true;
		let session_id = session.session_id;
		self.session = Some(session);
		NodeEncryption::Handshake { recipient: self.node_id, session_id, signer: my_node_id }
	}
	/// Acknowledge a NodeEncryption::Handshake and generate a NodeEncryption::Acknowledge to send back
	pub fn gen_acknowledgement(&mut self, recipient: NodeID, session_id: SessionID) -> NodeEncryption {
		self.session = Some(RemoteSession::from_session(session_id));
		NodeEncryption::Acknowledge { session_id, acknowledger: recipient }
	}
	/// Receive Acknowledgement of previously sent handshake and enable RemoteSession
	pub fn validate_handshake(&mut self, session_id: SessionID, acknowledger: NodeID) -> Result<(), RemoteNodeError> {
		let session = self.session()?;
		if session.session_id == session_id && self.node_id == acknowledger {
			self.handshake_pending = false;
			Ok(())
		} else {
			Err( RemoteNodeError::InvalidSessionError { passed: session_id } )
		}
	}
	/// Encrypt and generate packet if SessionType is Direct
	pub fn gen_packet(&self, my_net_id: InternetID, packet: NodePacket) -> Result<InternetPacket, RemoteNodeError> {
		let session = self.session()?;
		match &session.session_type {
			SessionType::Direct(direct_session) => {
				Ok(session.encrypt(packet).package(my_net_id, direct_session.net_id))
			},
			// TODO: Implement Routed sessions
			/* SessionType::Routed(routed_session) => {
				// Wrap routed session
				Err( RemoteNodeError::NoDirectSessionError )
			}, */
			_ => Err( RemoteNodeError::NoDirectSessionError ),
		}
	}
}

#[derive(Serialize, Deserialize, Debug)]
pub enum NodeEncryption {
	/// Handshake is sent from node wanting to establish secure tunnel to another node
	Handshake { recipient: NodeID, session_id: SessionID, signer: NodeID },
	/// When the other node receives the Handshake, they will send back an Acknowledge
	/// When the original party receives the Acknowledge, that tunnel may now be used 
	Acknowledge { session_id: SessionID, acknowledger: NodeID },
	Session { session_id: SessionID, packet: NodePacket },
}

impl NodeEncryption {
	pub fn package(&self, src_addr: InternetID, dest_addr: InternetID) -> InternetPacket {
		InternetPacket {
			src_addr,
			data: serde_json::to_vec(&self).expect("Failed to encode json"),
			dest_addr,
		}
	}
	pub fn unpackage(packet: &InternetPacket) -> Result<Self, serde_json::Error> {
		serde_json::from_slice(&packet.data)
	}
}