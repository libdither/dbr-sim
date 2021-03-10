#![allow(dead_code)]

use std::{cmp::Reverse, mem::{Discriminant, discriminant}, collections::HashMap};

use ta::{indicators::{SimpleMovingAverage, StandardDeviation}, Next};
use thiserror::Error;
use priority_queue::PriorityQueue;

use crate::internet::{InternetID, InternetPacket};
use crate::node::{SessionID, NodeID, RouteScalar, RouteCoord, NodePacket};
use crate::node::types::NUM_NODE_PACKETS;

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
			self.dist_dev = self.ping_dev.next(distance) as RouteScalar;
			self.ping_count += 1;
			Ok(self.dist_avg)
		} else { Err(SessionError::UnknownPingID { ping_id }) }
	}
	pub fn distance(&self) -> RouteScalar {
		self.dist_avg
	}
	/// Returns Some if the connection has been tested enough
	/// Returns Some(true) if it is a viable connection
	pub fn is_viable(&self) -> Option<bool> {
		if self.ping_count >= 1 {
			Some(self.dist_dev < 1)
		} else { None }
	}
	pub fn pending_pings(&self) -> usize { self.ping_queue.len() }
}
/// Represents session that is routed directly (through the internet)
#[derive(Default, Debug)]
pub struct PeerSession {
	pub reciprocal: bool,
}
impl PeerSession {
	fn new() -> Self { Self { reciprocal: false } }
}
/// Represents session that is routed through alternate nodes
#[derive(Debug)]
pub struct RoutedSession {
	remote_route: RouteCoord,
	intermediate_nodes: Vec<(NodeID, RouteCoord)>,
}

#[derive(Debug, Derivative)]
#[derivative(Default(bound=""))]
pub enum SessionType {
	// Default connection
	#[derivative(Default)]
	Normal,
	/// Another node who has notified that they consider me to be a close peer
	/// * `usize`: How many other nodes are closer
	IncomingPeer(usize),
	// Connection that is very close, designated as a peer 
	Peer(PeerSession),
	// Routed session
	Routed(RoutedSession),
}
impl SessionType {
	fn peer() -> Self { Self::Peer(PeerSession::new()) }
}

#[derive(Error, Debug)]
pub enum SessionError {
    #[error("There is no previous ping sent out with ID: {ping_id:?} or ping was forgotten")]
	UnknownPingID { ping_id: PingID },
	#[error("This session is not a Peer session")]
	NoPeerSession,
}

#[derive(Derivative)]
#[derivative(Debug)]
pub struct RemoteSession {
	pub session_id: SessionID, // All connections must have a SessionID for encryption
	pub session_type: SessionType, //  Sessions can either be Routed through other nodes or Directly Connected
	pub tracker: SessionTracker,
	pub return_net_id: InternetID,
	#[derivative(Debug="ignore")]
	pub last_packet_times: HashMap<(Discriminant<NodePacket>, NodeID), usize>, // Maps Packets to time last sent

	pub is_testing: bool, // Is this session currently in the process of being tested
}
impl RemoteSession {
	pub fn new(session_id: SessionID, session_type: SessionType, return_net_id: InternetID) -> Self {
		Self {
			session_id,
			session_type,
			tracker: SessionTracker::new(),
			return_net_id,
			last_packet_times: HashMap::with_capacity(NUM_NODE_PACKETS),
			is_testing: false,
		}
	}
	pub fn from_id(session_id: SessionID, return_net_id: InternetID) -> Self { Self::new(session_id, SessionType::Normal, return_net_id) }

	/// Test if viable and note that testing is commencing
	pub fn test_direct(&mut self) -> Option<bool> {
		self.is_testing = true;
		let is_viable = self.tracker.is_viable();
		if let Some(_) = is_viable { self.is_testing = false };
		is_viable
	}

	pub fn is_peer(&self) -> bool { if let SessionType::Peer(_) = self.session_type { true } else { false } }
	pub fn record_peer_notify(&mut self, rank: usize) { 
		match &mut self.session_type {
			SessionType::Normal => self.session_type = SessionType::IncomingPeer(rank),
			SessionType::IncomingPeer(num) => if rank == usize::MAX { self.session_type = SessionType::Normal } else { *num = rank },
			SessionType::Peer(peer_session) => peer_session.reciprocal = true,
			_ => {},
		}
	}

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
			SessionType::Normal | SessionType::Peer(_) | SessionType::IncomingPeer(_) => {
				Ok(packet.encrypt(self.session_id).package(self.return_net_id))
			},
			_ => todo!(),
		}
	}
}