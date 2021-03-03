
use std::collections::HashMap;

use crate::internet::{InternetID, InternetPacket};

pub const AREA: i32 = 100;
const VARIANCE: i32 = 0;
use rand::Rng;

/* // Network Sim structuring calculators
pub trait LatencyCalculator: Default {
	fn new(rng: &mut impl rand::Rng) -> Self;
	fn generate(&self, other: &Self, rng: &mut impl rand::Rng) -> usize;
} */
#[derive(Default, Debug)]
pub struct EuclidianLatencyCalculator {
	pub variance: i32,
	pub position: (i32, i32),
}
impl EuclidianLatencyCalculator {
	fn new (rng: &mut impl Rng) -> Self {
		let radius = AREA/2;
		EuclidianLatencyCalculator {
			variance: if VARIANCE != 0 { VARIANCE } else { 1 },
			position: (rng.gen_range(-radius, radius), rng.gen_range(-radius, radius)),
		}
	}
	fn generate(&self, other: &Self, rng: &mut impl Rng) -> usize {
		let dx = self.position.0 - other.position.0;
		let dy = self.position.1 - other.position.1;
		let dist = ((dx*dx + dy*dy) as f64).sqrt() as i32;
		(dist + rng.gen_range( -self.variance, self.variance) ) as usize
	}
}

/// Internet router
#[derive(Default, Debug)]
pub struct InternetRouter {
	/// Map linking Node pairs to speed between them (supports differing 2-way speeds)
	pub speed_map: HashMap<InternetID, EuclidianLatencyCalculator>,
	/// Map linking destination `Node`s to inbound packets
	pub packet_map: HashMap<InternetID, Vec<(InternetPacket, isize)>>,
}
impl InternetRouter {
	pub fn add_packets(&mut self, packets: Vec<InternetPacket>, rng: &mut impl Rng) {
		for packet in packets {
			
			self.speed_map.entry(packet.src_addr).or_insert_with(||EuclidianLatencyCalculator::new(rng));
			self.speed_map.entry(packet.dest_addr).or_insert_with(||EuclidianLatencyCalculator::new(rng));
			//use std::ops::Index;
			// This shouldn't panic since I set it right there ^^^
			let latency = self.speed_map[&packet.src_addr].generate(&self.speed_map[&packet.dest_addr], rng) as isize;

			// Add packet to packet stream
			if let Some(packet_stream) = self.packet_map.get_mut(&packet.dest_addr) {
				packet_stream.push((packet, latency));
			} else {
				self.packet_map
					.insert(packet.dest_addr, vec![(packet, latency)]);
			}
		}
	}
	pub fn tick_node(&mut self, destination: InternetID) -> Vec<InternetPacket> {
		if let Some(packets) = self.packet_map.get_mut(&destination) {
			packets.iter_mut().for_each(|item| item.1 -= 1); // Decrement ticks
			//let tmp_packets = packets.iter().filter(|x| x.1 < 0).map(|x| x.0)
			
			packets.drain_filter(|x| x.1 <= 0).map(|x| x.0).collect()
		} else {
			return Vec::new();
		}
	}
}