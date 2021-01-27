
use std::collections::HashMap;
use crate::internet::{InternetID, InternetPacket};

// Network Sim structuring calculators
pub trait LatencyCalculator: Default {
	fn generate(rng: &mut impl rand::Rng) -> Self;
	fn calculate(&self, other: &Self, rng: &mut impl rand::Rng) -> usize;
}
#[derive(Default, Debug)]
pub struct EuclidianLatencyCalculator {
	variance: usize,
	position: (i32, i32),
}
impl LatencyCalculator for EuclidianLatencyCalculator {
	fn generate(rng: &mut impl rand::Rng) -> Self {
		EuclidianLatencyCalculator {
			variance: rng.gen_range(0, 5),
			position: rng.gen(),
		}
	}
	fn calculate(&self, other: &Self, rng: &mut impl rand::Rng) -> usize {
		let dx = self.position.0 - other.position.0;
		let dy = self.position.1 - other.position.1;
		((dx*dx + dy*dy) as f64).sqrt() as usize + rng.gen_range(0, self.variance)
	}
}

/// Internet router
#[derive(Default, Debug)]
pub struct InternetRouter<LC: LatencyCalculator> {
	/// Map linking Node pairs to speed between them (supports differing 2-way speeds)
	speed_map: HashMap<InternetID, LC>,
	/// Map linking destination `Node`s to inbound packets
	packet_map: HashMap<InternetID, Vec<(InternetPacket, usize)>>,
}
impl<LC: LatencyCalculator> InternetRouter<LC> {
	pub fn add_packets(&mut self, packets: Vec<InternetPacket>) {
		let mut rng = rand::thread_rng();
		for packet in packets {
			let index = (packet.src_addr, packet.dest_addr);

			self.speed_map.entry(packet.src_addr).or_insert_with(||LC::generate(&mut rng));
			self.speed_map.entry(packet.dest_addr).or_insert_with(||LC::generate(&mut rng));
			//use std::ops::Index;
			// This shouldn't panic since I set it right there ^^^
			let latency = self.speed_map[&packet.src_addr].calculate(&self.speed_map[&packet.dest_addr], &mut rng);
			//let latency = src_calc.calculate(dest_calc, &mut rng);

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