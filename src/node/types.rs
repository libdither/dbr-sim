use crate::internet::{InternetID, InternetPacket};

pub use crate::node::session::{RemoteSession, SessionError, SessionType};
use crate::node::session::PingID;

use thiserror::Error;

/// Hash uniquely identifying a node (represents the Multihash of the node's Public Key)
pub type NodeID = u32;
/// Number uniquely identifying a session, represents a Symmetric key
pub type SessionID = u32;
/// Coordinate that represents a position of a node relative to other nodes in 2D space.
pub type RouteScalar = u64;
pub type RouteCoord = (RouteScalar, RouteScalar);

/// Packets that are sent between nodes in this protocol.
#[derive(Serialize, Deserialize, Debug, Clone)]
pub enum NodePacket {
	/// Sent to other Nodes. Expects PingResponse returned
	Ping(PingID), // Random number uniquely identifying this ping request
	/// PingResponse packet, time between Ping and PingResponse is measured
	PingResponse(PingID), // Acknowledge Ping(u64), sends back originally sent number

	/// Request Direct Connection
	PeerRequest,

	/// Request to a peer for them to request their peers to ping me
	RequestPings(usize), // usize: max number of pings

	/// Tell a peer that this node wants a ping (implying a potential direct connection)
	WantPing(NodeID, InternetID),
	/// Sent when node accepts a WantPing Request
	/// * `NodeID`: NodeID of Node who send the request in response to a RequestPings
	AcceptWantPing(NodeID),
	/// Sent when node has a new peer that it thinks another node should connect to, prompts a Bootstrap request from other node
	/// * `NodeID`: NodeID of new node who connected as a direct peer
	NewPeersHint(NodeID),

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
	#[error("Received handshake but earlier handshake request was already pending")]
	SimultaneousHandshake,
	#[error("Session Error")]
	SessionError(#[from] SessionError),
}
#[derive(Debug)]
pub struct RemoteNode {
	pub node_id: NodeID, // The ID of the remote node
	handshake_pending: Option<(usize, SessionID)>, // is Some(current_time, session_id) if handshake request is pending acknowledgement
	session: Option<RemoteSession>, // Session object, is None if no connection is active
}
impl RemoteNode {
	pub fn new(node_id: NodeID) -> Self {
		Self {
			node_id,
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
	/// This function creates a NodeEncryption::Handshake object to be sent to a peer that secure communication should be established with
	pub fn gen_handshake(&mut self, my_node_id: NodeID, current_time: usize) -> NodeEncryption {
		// Generate Handshake to send out
		let session_id: SessionID = rand::random();
		self.handshake_pending = Some((current_time, session_id));
		NodeEncryption::Handshake { recipient: self.node_id, session_id, signer: my_node_id, time_sent: current_time }
	}
	/// Make note of handshake, create session & send back acknowledgement
	pub fn gen_acknowledgement(&mut self, recipient: NodeID, session_id: SessionID, time_generated: usize, my_node_id: NodeID, return_net_id: InternetID) -> Result<NodeEncryption, RemoteNodeError> {
		if recipient != my_node_id { return Err(RemoteNodeError::UnknownAckRecipient { recipient }) }
		// Check if already sent a handshake
		if let Some((own_time_sent, _)) = self.handshake_pending {
			// If I sent it first, return and wait for Acknowledgement
			if time_generated > own_time_sent { return Err(RemoteNodeError::SimultaneousHandshake) }
		}
		// Otherwise gen acknowledgement and session
		self.session = Some(RemoteSession::from_id(session_id, return_net_id));
		Ok(NodeEncryption::Acknowledge { session_id, acknowledger: recipient })
	}
	/// Receive Acknowledgement of previously sent handshake and enable RemoteSession if Acknowledgement was requested
	pub fn validate_session(&mut self, session_id: SessionID, return_net_id: InternetID) -> Result<(), RemoteNodeError> {
		// Check if there is actually a handshake pending
		if let Some((_, sent_session_id)) = self.handshake_pending {
			// Check if right acknowledgement was received
			if sent_session_id == session_id {
				self.handshake_pending = None;
				self.session = Some(RemoteSession::from_id(session_id, return_net_id));
				Ok(())
			} else { Err( RemoteNodeError::UnknownAck { passed: session_id } ) }
		} else { Err(RemoteNodeError::NoPendingHandshake) }
	}
	/// Wrap packet and push to `outgoing` Vec
	pub fn add_packet(&self, packet: NodePacket, outgoing: &mut Vec<InternetPacket>) -> Result<(), RemoteNodeError> {
		Ok(outgoing.push(self.session()?.gen_packet(packet)?))
	}
}

#[derive(Serialize, Deserialize, Debug)]
pub enum NodeEncryption {
	/// Handshake is sent from node wanting to establish secure tunnel to another node
	Handshake { recipient: NodeID, session_id: SessionID, signer: NodeID, time_sent: usize },
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