
use std::collections::HashMap;
use std::ops::Range;

const VARIANCE: isize = 2;
use nalgebra::Point2;
use rand::Rng;

use super::{CustomNode, NetAddr, NetSimPacket, NetSimPacketVec};

/* // Network Sim structuring calculators
pub trait LatencyCalculator: Default {
	fn new(rng: &mut impl rand::Rng) -> Self;
	fn generate(&self, other: &Self, rng: &mut impl rand::Rng) -> usize;
} */
#[derive(Debug, Serialize, Deserialize)]
pub struct RouterNode {
	pub uuid: NetAddr,
	pub variance: isize,
	pub position: Point2<f32>,
	pub distance_cache: HashMap<NetAddr, isize>,
}
impl RouterNode {
	fn random(uuid: NetAddr, range: &(Range<i32>, Range<i32>), rng: &mut impl Rng) -> Self {
		// let radius = AREA/2;
		Self {
			uuid,
			variance: VARIANCE,
			position: Point2::new(rng.gen_range(range.0.clone()), rng.gen_range(range.1.clone())).map(|d|d as f32),
			distance_cache: HashMap::new(),
		}
	}
	fn generate(&mut self, other_uuid: NetAddr, other_position: Point2<f32>, rng: &mut impl Rng) -> isize {
		let dist = *self.distance_cache.entry(other_uuid).or_insert(nalgebra::distance(&self.position, &other_position) as isize);
		dist as isize + rng.gen_range(-self.variance..self.variance)
	}
}

/// Internet router
#[derive(Debug, Serialize, Deserialize)]
pub struct NetSimRouter<CN: CustomNode> {
	pub field_dimensions: (Range<i32>, Range<i32>),
	/// Map linking Node pairs to speed between them (supports differing 2-way speeds)
	pub node_map: HashMap<NetAddr, RouterNode>,
	/// Map linking destination `Node`s to inbound packets
	#[serde(skip)]
	pub packet_map: HashMap<NetAddr, Vec<(NetSimPacket<CN>, isize)>>,
}
impl<CN: CustomNode> NetSimRouter<CN> {
	pub fn new(field_dimensions: (Range<i32>, Range<i32>)) -> Self {
		Self {
			field_dimensions,
			node_map: Default::default(),
			packet_map: Default::default(),
		}
	}
	pub fn add_node(&mut self, net_addr: NetAddr, rng: &mut impl Rng) {
		self.node_map.entry(net_addr).or_insert(RouterNode::random(net_addr, &self.field_dimensions, rng));
	}
	pub fn add_packets(&mut self, packets: NetSimPacketVec<CN>, rng: &mut impl Rng) {
		for packet in packets {
			let dest = self.node_map.entry(packet.dest_addr).or_insert(RouterNode::random(packet.dest_addr, &self.field_dimensions, rng));
			let (dest_uuid, dest_position) = (dest.uuid, dest.position);
			let src = self.node_map.entry(packet.src_addr).or_insert(RouterNode::random(packet.src_addr, &self.field_dimensions, rng));
			
			// Calculate latency
			let latency = src.generate(dest_uuid, dest_position, rng);

			// Add packet to packet stream
			if let Some(packet_stream) = self.packet_map.get_mut(&packet.dest_addr) {
				packet_stream.push((packet, latency));
			} else {
				self.packet_map
					.insert(packet.dest_addr, vec![(packet, latency)]);
			}
		}
	}
	pub fn tick_node(&mut self, destination: NetAddr) -> NetSimPacketVec<CN> {
		if let Some(packets) = self.packet_map.get_mut(&destination) {
			packets.iter_mut().for_each(|item| item.1 -= 1); // Decrement ticks
			// Filter out packets that should be passed
			packets.drain_filter(|x| x.1 <= 0).map(|x| x.0).collect()
		} else {
			return NetSimPacketVec::new();
		}
	}
}