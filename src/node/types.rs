use crate::internet::{InternetID, InternetPacket};
use crate::node::Node;

pub type NodeID = u8;
pub type SessionID = u128;

pub type RouteCoord = (usize, usize);


#[derive(Serialize, Deserialize, Debug)]
pub enum NodePacket {
	// Sent to other Nodes. Expects PingResponse returned
	Ping,
	// PingResponse packet, time between Ping and PingResponse is measured
	PingResponse,

	// Request to a peer for them to request their peers to ping me
	RequestPings(u32), // u32: max number of pings
	// Tell a peer that this node wants a ping (implying a potential direct connection)
	WantPing(InternetID, NodeID),

	/// Request to establish a 2-way route between InternetID and this node through another node
	/// Vec<u8> is an encrypted packet (this can contain anything)
	Route(NodeID, Vec<u8>), 
	RouteError()
}


/*
impl NodePacket {
	pub fn package(self, node: &Node, dest: InternetID) -> InternetPacket  {
		
	}
	pub fn unpackage(node: &Node, packet: InternetPacket) -> Result<Self, PacketParseError> {
		let encryption: NodeEncryption = serde_json::from_slice(&packet.data).expect("Failed to decode json");
		match encryption {
			NodeEncryption::Handshake(node_id, session_id) => {
				if let Some(remote) = node.sessions.get(&session_id) {
					Ok(encrypted.data)
				} else {
					Err(PacketParseError::SymmetricError { session_id })
				}
			},
			NodeEncryption::Session(session_id, packet) => {
				if let Some(remote) = node.peers.get(&node_id) {
					Ok(packet)
				} else {
					Err(PacketParseError::AsymmetricError { node_id })
				}
			}
		}
	}
}
*/

#[derive(Debug, Default)]
struct RemoteSession {
	//pub_key: PublicKey,
	//noise_session: Option<snow::TransportState>,
	session_id: SessionID, // All connections must have a SessionID for encryption
	/// If this session is outdated, caused by enough time passing, manually triggered, or in the middle of new Handshake
	outdated_session: bool, 
	net_id: Option<InternetID>, // Only for directly connected nodes (nodes that are nearby)
	distance: usize, // Distance value based on latency, network speed, and other factors

	last_ping: usize, // Time sent previous ping
}
impl RemoteSession {
	fn new(session_id: SessionID) -> Self { Self {session_id, ..Default::default()} }
}
#[derive(Debug)]
pub struct RemoteNode {
	node_id: NodeID, // The ID of the remote node
	route_coord: Option<RouteCoord>, // Last queried Route Coordinates
	session: Option<RemoteSession>, // Session object, is None if no connection is active
}

use thiserror::Error;
#[derive(Error, Debug)]
pub enum RemoteNodeError {
    #[error("There is no active session with the node: {node_id:?}")]
	NoSessionError { node_id: NodeID },
	#[error("Session ID passed: {passed:?} does not match existing session ID")]
    InvalidSessionError { passed: SessionID },
    #[error("Cannot package packet because RemoteNode does not contain InternetID")]
	UnknownNetIdError
}

impl RemoteNode {
	pub fn new(node_id: NodeID) -> Self {
		Self {
			node_id,
			route_coord: None,
			session: None,
		}
	}
	pub fn session_active(&self) -> bool {
		self.session.is_some()
	}
	/// This function creates a NodeEncryption::Handshake object to be sent to a peer that secure communication should be established with
	pub fn gen_handshake(&mut self, me: &Node) -> NodeEncryption {
		let session_id = rand::random::<SessionID>();
		let session = self.session.get_or_insert(RemoteSession::default());
		session.session_id = session_id;
		session.outdated_session = true;
		NodeEncryption::Handshake { recipient: self.node_id, session_id, signer: me.node_id }
	}
	/// Acknowledge a NodeEncryption::Handshake and generate a NodeEncryption::Acknowledge to send back
	pub fn gen_acknowledgement(&mut self, recipient: NodeID, session_id: SessionID, signer: NodeID) -> NodeEncryption {
		self.session.get_or_insert(RemoteSession::new(session_id)).session_id = session_id; // Set session ID
		NodeEncryption::Acknowledge { session_id, acknowledger: recipient }
	}
	/// Receive Acknowledgement of previously sent handshake and enable RemoteSession
	pub fn validate_handshake(&mut self, session_id: SessionID, acknowledger: NodeID) -> Result<SessionID, RemoteNodeError> {
		if let Some(session) = &mut self.session {
			if session.session_id == session_id {
				session.outdated_session = false;
				Ok(session.session_id)
			} else {
				Err( RemoteNodeError::InvalidSessionError { passed: session_id } )
			}
		} else {
			Err( RemoteNodeError::NoSessionError { node_id: self.node_id } )
		}
	}
	pub fn encrypt(&self, packet: NodePacket) -> Result<NodeEncryption, RemoteNodeError> {
		if let Some(session) = &self.session {
			Ok( NodeEncryption::Session { session_id: session.session_id, packet } )
		} else { Err( RemoteNodeError::NoSessionError { node_id: self.node_id } ) }
	}
	/// Encrypt and generate packet
	pub fn gen_packet(&self, node_net_id: InternetID, packet: NodePacket) -> Result<InternetPacket, RemoteNodeError> {
		if let Some(session) = &self.session {
			if let Some(net_id) = session.net_id {
				Ok(self.encrypt(packet)?.package(node_net_id, net_id))
			} else {
				Err( RemoteNodeError::UnknownNetIdError )
			}
		} else {
			Err( RemoteNodeError::NoSessionError { node_id: self.node_id } )
		}
	}
}

#[derive(Serialize, Deserialize, Debug)]
pub enum NodeEncryption {
	/// Handshake is sent from node wanting to establish secure tunnel to another node
	Handshake { recipient: NodeID, session_id: SessionID, signer: NodeID },
	/// When the other node receives the Handshake, they will send back an Acknowledge
	/// When the original party receives the Acknowledge, that tunnel may not be used 
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