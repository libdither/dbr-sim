#![allow(dead_code)]

use std::{cmp::Reverse, mem::{Discriminant, discriminant}, collections::HashMap};

use ta::{indicators::{SimpleMovingAverage, StandardDeviation}, Next};
use thiserror::Error;
use priority_queue::PriorityQueue;

use crate::internet::{InternetID, InternetPacket};
use crate::node::{SessionID, NodeID, RouteScalar, RouteCoord, NodePacket, types::{NodeEncryption, NUM_NODE_PACKETS}};

/// Number that uniquely identifies a ping request so that multiple Pings may be sent at the same time
pub type PingID = u64;

const MAX_PENDING_PINGS: usize = 25;

#[derive(Derivative)]
#[derivative(Debug)]
pub struct SessionTracker {
	#[derivative(Debug="ignore")]
	ping_queue: PriorityQueue<PingID, Reverse<usize>>, // Tuple represents (ID of ping, priority by reversed time sent) 
	pub dist_avg: RouteScalar,
	#[derivative(Debug="ignore")]
	dist_dev: RouteScalar,
	#[derivative(Debug="ignore")]
	ping_avg: SimpleMovingAverage, // Moving average of ping times
	#[derivative(Debug="ignore")]
	ping_dev: StandardDeviation,
	pub ping_count: usize,
}
impl SessionTracker {
	fn new() -> Self {
		Self {
			ping_queue: PriorityQueue::with_capacity(MAX_PENDING_PINGS),
			dist_avg: 0,
			dist_dev: 0,
			ping_avg: SimpleMovingAverage::new(10).unwrap(),
			ping_dev: ta::indicators::StandardDeviation::new(10).unwrap(),
			ping_count: 0,
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
			//self.dist_dev = self.ping_dev.next(distance) as RouteScalar;
			self.ping_count += 1;
			Ok(self.dist_avg)
		} else { Err(SessionError::UnknownPingID { ping_id }) }
	}
	pub fn pending_pings(&self) -> usize { self.ping_queue.len() }
}

/// Represents directly connected session over public Network
#[derive(Debug)]
pub struct DirectSession {
	/// Network Address of remote
	pub net_id: InternetID,
	/// Some(bool) if peered, Some(true) if reciprocal peer
	pub is_peered: bool,
	pub is_incoming_peer: bool,
}
impl DirectSession {
	fn new(net_id: InternetID) -> SessionType {
		SessionType::Direct(DirectSession {
			net_id,
			is_peered: false,
			is_incoming_peer: false,
		})
	}
}

/// Represents onion-routed session through different Dither nodes
#[derive(Debug)]
pub struct RoutedSession {
	pub hops: usize, // Desired number of hops in the routed session
	/// Resolved nodes with their own RoutedSession which messages can be passed through
	/// First NodeID in the list must correspond to a Direct session, the rest will be routed sessions
	pub proxy_nodes: Vec<(SessionID, RouteCoord)>,
	/// Peer Network ID that this Session is routed out of
	pub outgoing_net_id: InternetID,
}

#[derive(Debug)]
pub enum SessionType {
	Direct(DirectSession),
	Routed(RoutedSession),
}

#[derive(Error, Debug)]
pub enum SessionError {
    #[error("There is no previous ping sent out with ID: {ping_id:?} or ping was forgotten")]
	UnknownPingID { ping_id: PingID },
	#[error("This session is not a direct session")]
	NotDirectType
}

/// Represents a Remote Connection, Direct or Routed
#[derive(Derivative)]
#[derivative(Debug)]
pub struct RemoteSession {
	/// All connections must have a SessionID for symmetric encryption
	pub session_id: SessionID,
	/// Direct Session or Routed Session
	pub session_type: SessionType,
	/// Tracks ping times to a remote node
	#[derivative(Debug="ignore")]
	pub tracker: SessionTracker,
	/// Keep track of times certain packets were last received from remote node
	#[derivative(Debug="ignore")]
	pub last_packet_times: HashMap<(Discriminant<NodePacket>, NodeID), usize>, // Maps Packets to time last sent
}
impl RemoteSession {
	pub fn new(session_id: SessionID, session_type: SessionType) -> Self {
		Self {
			session_id,
			session_type,
			tracker: SessionTracker::new(),
			last_packet_times: HashMap::with_capacity(NUM_NODE_PACKETS),
		}
	}
	pub fn from_address(session_id: SessionID, return_net_id: InternetID) -> Self { Self::new(session_id, DirectSession::new(return_net_id)) }
	pub fn direct(&self) -> Result<&DirectSession, SessionError> {
		if let SessionType::Direct(direct) = &self.session_type { Ok(direct) } else { Err(SessionError::NotDirectType) }
	}
	pub fn direct_mut(&mut self) -> Result<&mut DirectSession, SessionError> {
		if let SessionType::Direct(direct) = &mut self.session_type { Ok(direct) } else { Err(SessionError::NotDirectType) }
	}
	pub fn set_peer(&mut self, toggle: bool) {
		if let SessionType::Direct(direct_session) = &mut self.session_type {
			direct_session.is_peered = toggle;
		}
	}
	pub fn record_peer_notify(&mut self, rank: usize) {
		if let SessionType::Direct(direct_session) = &mut self.session_type {
			direct_session.is_incoming_peer = rank != usize::MAX;
		}
	}
	pub fn is_peer(&self) -> bool { if let SessionType::Direct(direct_session) = &self.session_type { direct_session.is_peered } else { false } }
	

	/// Returns how long ago (in ticks) a packet was last sent or None if packet has never been sent
	pub fn check_packet_time(&mut self, packet: &NodePacket, sending_node_id: NodeID, current_time: usize) -> Option<usize> {
		if let Some(last_time) = self.last_packet_times.get_mut(&(discriminant(packet), sending_node_id)) {
			let difference = current_time - *last_time;
			*last_time = current_time;
			Some(difference)
		} else { 
			self.last_packet_times.insert((discriminant(packet), sending_node_id), current_time); None
		}
	}
	/// Generate InternetPacket from NodePacket doing whatever needs to be done to route it through the network securely
	pub fn gen_packet(&self, packet: NodePacket) -> Result<InternetPacket, SessionError> {
		match &self.session_type {
			SessionType::Direct(direct_session) => {
				let encrypted = NodeEncryption::Session { session_id: self.session_id, packet };
				Ok(encrypted.package(direct_session.net_id))
			},
			SessionType::Routed(routed_session) => {
				let mut encrypted = NodeEncryption::Session { session_id: self.session_id, packet };
				for (session_id, route_coord) in &routed_session.proxy_nodes {
					encrypted = encrypted.wrap_traverse(*session_id, route_coord.clone());
				}
				Ok(encrypted.package(routed_session.outgoing_net_id))
			},
		}
	}
	pub fn dist(&self) -> RouteScalar {
		return self.tracker.dist_avg;
	}
}