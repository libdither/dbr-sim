use std::cmp::Reverse;

use ta::{indicators::{SimpleMovingAverage, StandardDeviation}, Next};
use thiserror::Error;
use priority_queue::PriorityQueue;

use crate::internet::{InternetID, InternetPacket};
use crate::node::{SessionID, NodeID, RouteScalar, RouteCoord, NodePacket};

/// Number that uniquely identifies a ping request so that multiple Pings may be sent at the same time
pub type PingID = u64;

const MAX_PENDING_PINGS: usize = 25;

#[derive(Derivative)]
#[derivative(Debug)]
pub struct SessionTracker {
	#[derivative(Debug="ignore")]
	ping_queue: PriorityQueue<PingID, Reverse<usize>>, // Tuple represents (ID of ping, priority by reversed time sent) 
	dist_avg: RouteScalar,
	#[derivative(Debug="ignore")]
	dist_dev: RouteScalar,
	#[derivative(Debug="ignore")]
	ping_avg: SimpleMovingAverage, // Moving average of ping times
	#[derivative(Debug="ignore")]
	ping_dev: StandardDeviation,
}
impl SessionTracker {
	fn new() -> Self {
		Self {
			ping_queue: PriorityQueue::with_capacity(MAX_PENDING_PINGS),
			dist_avg: 0,
			dist_dev: 0,
			ping_avg: SimpleMovingAverage::new(10).unwrap(),
			ping_dev: ta::indicators::StandardDeviation::new(10).unwrap(),
		}
	}
	// Generate Ping Packet
	pub fn gen_ping(&mut self, gen_time: usize) -> PingID {
		let ping_id: PingID = rand::random();
		self.ping_queue.push(ping_id, Reverse(gen_time));
		// There shouldn't be more than 25 pings pending
		if self.ping_queue.len() >= MAX_PENDING_PINGS {
			self.ping_queue.pop();
		}
		ping_id
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
	/// Returns Some if the connection has been tested enough
	/// Returns Some(true) if it is a viable connection
	pub fn is_viable(&self) -> Option<bool> {
		if self.ping_queue.len() >= 5 {
			Some(self.dist_dev < 1)
		} else { None }
	}
	pub fn pending_pings(&self) -> usize { self.ping_queue.len() }
}
/// Represents session that is routed directly (through the internet)
#[derive(Default, Debug)]
pub struct DirectSession {
	pub net_id: InternetID, // Internet Address
	pub was_requested: bool,
}
impl DirectSession {
	fn new(net_id: InternetID) -> Self { Self { net_id, was_requested: false } }
	fn with_request(mut self) -> Self { self.was_requested = true; self }
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
	Return(InternetID),
}
impl SessionType {
	pub fn default(net_id: InternetID) -> Self { Self::Return(net_id) } 
	pub fn direct(net_id: InternetID) -> Self { Self::Direct(DirectSession::new(net_id)) }
}

#[derive(Error, Debug)]
pub enum SessionError {
    #[error("There is no previous ping sent out with ID: {ping_id:?} or ping was forgotten")]
	UnknownPingID { ping_id: PingID },
	#[error("This session is not Direct")]
	NoDirectSessionError,
}

#[derive(Debug)]
pub struct RemoteSession {
	pub session_id: SessionID, // All connections must have a SessionID for encryption
	pub session_type: SessionType, //  Sessions can either be Routed through other nodes or Directly Connected
	pub tracker: SessionTracker,
}
impl RemoteSession {
	pub fn new(session_id: SessionID, session_type: SessionType) -> Self {
		Self { session_id, session_type, tracker: SessionTracker::new() }
	}
	pub fn new_return(net_id: InternetID) -> Self { Self::new(rand::random(), SessionType::default(net_id)) }
	pub fn new_direct(net_id: InternetID) -> Self { Self::new(rand::random(), SessionType::direct(net_id)) }
	pub fn request_direct(&mut self) {
		if let SessionType::Return(net_id) = self.session_type {
			self.session_type = SessionType::Direct(DirectSession::new(net_id).with_request());
		}
	}
	pub fn is_direct(&self) -> bool { match self.session_type { SessionType::Direct(_) => true, _ => false} }
	pub fn direct(&self) -> Result<&DirectSession, SessionError> { match &self.session_type { SessionType::Direct(direct) => Ok(direct), _ => Err(SessionError::NoDirectSessionError) } }
	pub fn direct_mut(&mut self) -> Result<&mut DirectSession, SessionError> { match &mut self.session_type { SessionType::Direct(direct) => Ok(direct), _ => Err(SessionError::NoDirectSessionError), } }
	/// Generate InternetPacket from NodePacket doing whatever needs to be done to route it through the network securely
	pub fn gen_packet(&self, packet: NodePacket) -> Result<InternetPacket, SessionError> {
		match &self.session_type {
			SessionType::Direct(direct_session) => {
				Ok(packet.encrypt(self.session_id).package(0, direct_session.net_id))
			},
			SessionType::Return(net_id) => {
				Ok(packet.encrypt(self.session_id).package(0, *net_id))
			}
			_ => todo!(),
		}
	}
}