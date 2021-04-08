use super::{InternetPacket, Node, NodeError, NodeID, NodePacket, RemoteSession, RouteCoord, SessionError, SessionID, session::SessionType};

use thiserror::Error;

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

#[derive(Debug, Derivative, Serialize, Deserialize)]
#[derivative(Hash, PartialEq, Eq)]
pub struct RemoteNode {
	// The ID of the remote node
	pub node_id: NodeID,
	// Received Route Coordinate of the Remote Node
	#[derivative(PartialEq="ignore", Hash="ignore")]
	pub route_coord: Option<RouteCoord>,
	// If handshake is pending: Some(pending_session_id, time_sent_handshake, packets_to_send)
	#[derivative(PartialEq="ignore", Hash="ignore")]
	#[serde(skip)]
	pub pending_session: Option<Box< (SessionID, usize, Vec<NodePacket>, SessionType) >>,
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